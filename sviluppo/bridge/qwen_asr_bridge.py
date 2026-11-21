"""CPU GGUF adapter for the already installed qwen3-asr.cpp Windows CLI.

No installs/downloads/Torch. Energy VAD, bounded overlapping chunks, cached
word alignment, absolute offsets and real progress shared with the Rust queue.
"""
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import ctypes

VERSION = 'qwen-gguf-cpu-v1'


def free_ram():
    if os.name != 'nt': return None
    class Memory(ctypes.Structure):
        _fields_ = [('length', ctypes.c_ulong), ('load', ctypes.c_ulong)] + [(key, ctypes.c_ulonglong) for key in ('total', 'available', 'total_page', 'available_page', 'total_virtual', 'available_virtual', 'extended')]
    value = Memory(); value.length = ctypes.sizeof(value)
    return value.available if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(value)) else None


def peak_working_set(child):
    if os.name != 'nt': return None
    class Counters(ctypes.Structure):
        _fields_ = [('cb', ctypes.c_ulong), ('faults', ctypes.c_ulong)] + [(key, ctypes.c_size_t) for key in ('peak_working', 'working', 'peak_paged', 'paged', 'peak_nonpaged', 'nonpaged', 'pagefile', 'peak_pagefile')]
    value = Counters(); value.cb = ctypes.sizeof(value)
    handle = ctypes.c_void_p(int(child._handle))
    return value.peak_working if ctypes.windll.psapi.GetProcessMemoryInfo(handle, ctypes.byref(value), value.cb) else None


def write_json(path, value):
    temporary = path.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False), encoding='utf-8')
    temporary.replace(path)


def sha256(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def identity(path, cache):
    stamp = {'path': str(path), 'bytes': path.stat().st_size, 'mtime_ns': path.stat().st_mtime_ns}
    key = hashlib.sha256(str(path).encode()).hexdigest()
    record = cache / (key + '.json')
    if record.is_file():
        old = json.loads(record.read_text(encoding='utf-8'))
        if all(old.get(k) == v for k, v in stamp.items()):
            return old
    stamp['sha256'] = sha256(path)
    write_json(record, stamp)
    return stamp


def chunk_plan(duration, silences, seconds=30):
    """Core ownership is disjoint; contexts overlap for words at chunk joins.

    Returns (chunks, skipped): a core entirely under the silence threshold is
    recorded as a skipped span, never silently dropped.
    """
    chunks = []
    skipped = []
    cursor = 0
    while cursor < duration:
        end = min(cursor + seconds * 1000, duration)
        if end < duration:
            pauses = [(a + b) // 2 for a, b in silences if cursor + seconds * 500 <= (a + b) // 2 <= end]
            if pauses:
                end = max(pauses)
        # Skip an entirely silent core. Never deduplicate repeated spoken words.
        if not any(a <= cursor and b >= end for a, b in silences):
            chunks.append({'core_start': cursor, 'core_end': end, 'start': max(0, cursor - 750), 'end': min(duration, end + 750)})
        else:
            skipped.append([cursor, end])
        cursor = end
    return chunks, skipped


def skip_facts(skipped):
    """Bounded, honest report of spans the plan skipped as silent."""
    return {
        'skipped_silent_spans': skipped[:200],
        'skipped_silent_span_count': len(skipped),
        'skipped_silent_seconds': round(sum(b - a for a, b in skipped) / 1000, 2),
    }


def merge_words(chunks):
    result = []
    for chunk, words in chunks:
        length = chunk['end'] - chunk['start']
        for w in words:
            text = str(w.get('word', '')).strip()
            a, b = w.get('start'), w.get('end')
            if not text or not isinstance(a, (int, float)) or not isinstance(b, (int, float)) or not math.isfinite(a) or not math.isfinite(b):
                continue
            a, b = round(a * 1000), round(b * 1000)
            if a < 0 or b <= a or b > length + 100:
                continue
            start, end = a + chunk['start'], min(b, length) + chunk['start']
            midpoint = (start + end) / 2
            if chunk['core_start'] <= midpoint < chunk['core_end']:
                result.append({'text': text, 'start_ms': start, 'end_ms': end})
    result.sort(key=lambda w: (w['start_ms'], w['end_ms']))
    return result


def alignment_diagnostics(cache, duration, seconds):
    """Retain text the aligner could not time, without fabricating word ranges.

    Sidecar versioning leaves the inference cache and saved v1 transcript intact.
    Can enrich an old cache without another model run.
    """
    saved = cache / 'alignment-diagnostics-v1.json'
    if saved.is_file(): return json.loads(saved.read_text(encoding='utf-8'))
    pauses = cache / 'silences.json'
    if not pauses.is_file(): return {'available': False, 'reason': 'missing_chunk_plan'}
    chunks, _ = chunk_plan(duration, json.loads(pauses.read_text()), seconds)
    unplaced = []; texts = []
    for index, chunk in enumerate(chunks):
        output = cache / f'chunk-{index:05d}.json'
        if not output.is_file(): return {'available': False, 'reason': 'missing_chunk_json'}
        owned = []
        for word in json.loads(output.read_text(encoding='utf-8')).get('words', []):
            text = str(word.get('word', '')).strip()
            if not text: continue
            a, b = word.get('start'), word.get('end')
            finite = all(isinstance(v, (int, float)) and math.isfinite(v) for v in (a, b))
            at = None
            if finite:
                a, b = round(a * 1000), round(b * 1000)
                midpoint = (a + b) / 2 + chunk['start']
                if not chunk['core_start'] <= midpoint < chunk['core_end']: continue
                at = max(0, min(duration, a + chunk['start']))
            owned.append(text)
            if not finite or a < 0 or b <= a or b > chunk['end'] - chunk['start'] + 100:
                unplaced.append({'text': text, 'at_ms': at, 'chunk_index': index,
                                 'reason': 'invalid_or_zero_word_duration', 'timing_verified': False})
        texts.append({'chunk_index': index, 'core_start_ms': chunk['core_start'],
                      'core_end_ms': chunk['core_end'], 'text': ' '.join(owned)})
    result = {'version': 1, 'available': True, 'unplaced_words': unplaced,
              'unplaced_word_count': len(unplaced), 'raw_owned_chunk_texts': texts,
              'note': 'Recognized text without usable word timing is retained separately; no invented durations. Chunk context seams still require listening.'}
    write_json(saved, result)
    return result


def run(request):
    work = Path(request['work_dir']); work.mkdir(parents=True, exist_ok=True)
    progress = work / 'progress.json'
    def report(stage, **kw): write_json(progress, {'stage': stage, **kw})
    config = request['config']
    source = Path(request['source']).resolve(strict=True)
    paths = {k: Path(config[k]).resolve(strict=True) for k in ('runner', 'model', 'aligner')}
    for k in ('model', 'aligner'):
        with paths[k].open('rb') as stream:
            if stream.read(4) != b'GGUF': raise RuntimeError('File non GGUF: ' + str(paths[k]))
    report('Identificazione dei modelli e del media')
    identities = work.parent.parent / 'asr-identities'; identities.mkdir(exist_ok=True)
    models = {k: identity(p, identities) for k, p in paths.items()}
    source_identity = identity(source, identities)
    parameters = {'language': request['language'], 'chunk_seconds': config['chunk_seconds'], 'threads': config['threads'], 'noise_db': -38, 'silence_seconds': .5, 'overlap_ms': 750}
    key = hashlib.sha256(json.dumps({'models': {k:v['sha256'] for k,v in models.items()}, 'source': source_identity['sha256'], 'parameters': parameters, 'version': VERSION}, sort_keys=True).encode()).hexdigest()
    cache = Path(request['cache_dir']) / key; cache.mkdir(parents=True, exist_ok=True)
    ready = cache / 'result.json'
    if ready.is_file():
        result = json.loads(ready.read_text(encoding='utf-8')); result['cache_hit'] = True
        result['metadata']['alignment_diagnostics'] = alignment_diagnostics(cache, request['duration_ms'], config['chunk_seconds'])
        pauses = cache / 'silences.json'
        if 'skipped_silent_spans' not in result['metadata'] and pauses.is_file():
            # Silent-skip facts for caches written before the report existed:
            # recomputed from cached silence analysis, no new inference.
            _, skipped = chunk_plan(request['duration_ms'], json.loads(pauses.read_text()), config['chunk_seconds'])
            result['metadata'].update(skip_facts(skipped))
            result['metadata']['skipped_reported_from_cached_silences'] = True
        report('Trascrizione e allineamento già presenti in cache', completed_chunks=0, total_chunks=0)
        return result
    ffmpeg = request['ffmpeg']
    silence_file = cache / 'silences.json'
    duration = request['duration_ms']
    if silence_file.is_file():
        silences = json.loads(silence_file.read_text())
    else:
        report('Analisi delle pause audio con FFmpeg')
        probe = subprocess.run([ffmpeg, '-nostdin', '-hide_banner', '-threads', '2', '-i', str(source), '-vn', '-af', 'silencedetect=noise=-38dB:d=0.5', '-f', 'null', '-'], capture_output=True, timeout=max(120, duration/1000))
        if probe.returncode: raise RuntimeError(probe.stderr.decode('utf-8', errors='replace')[-2000:])
        start = None; silences = []
        for event, value in re.findall(r'silence_(start|end):\s*([0-9.]+)', probe.stderr.decode('utf-8', errors='replace')):
            at = min(duration, round(float(value)*1000))
            if event == 'start': start = at
            elif start is not None: silences.append([start, at]); start = None
        if start is not None: silences.append([start, duration])
        write_json(silence_file, silences)
    chunks, skipped = chunk_plan(duration, silences, config['chunk_seconds'])
    outputs = []; started = time.monotonic(); ran = 0; peak_rss = None; min_free_ram = free_ram()
    env = {**os.environ, 'HF_HUB_OFFLINE': '1', 'TRANSFORMERS_OFFLINE': '1'}
    for index, chunk in enumerate(chunks):
        report('Trascrizione e allineamento Qwen in CPU', completed_chunks=index, total_chunks=len(chunks), source_start_ms=chunk['core_start'])
        output = cache / f'chunk-{index:05d}.json'
        if not output.is_file():
            audio = cache / f'chunk-{index:05d}.wav'
            subprocess.run([ffmpeg, '-nostdin', '-v', 'error', '-y', '-ss', str(chunk['start']/1000), '-i', str(source), '-t', str((chunk['end']-chunk['start'])/1000), '-vn', '-ac', '1', '-ar', '16000', str(audio)], capture_output=True, check=True, timeout=90)
            temporary = output.with_suffix('.pending.json')
            command = [str(paths['runner']), '-m', str(paths['model']), '--aligner-model', str(paths['aligner']), '-f', str(audio), '--transcribe-align', '-t', str(config['threads']), '--max-tokens', '1024', '-o', str(temporary)]
            if request['language'] != 'auto': command += ['-l', {'it':'Italian', 'en':'English', 'es':'Spanish'}.get(request['language'], request['language'])]
            with (cache / f'chunk-{index:05d}.log').open('wb') as log:
                available = free_ram()
                required = models['model']['bytes'] + models['aligner']['bytes'] + (int(request.get('reserve_ram_mib', 2048)) + 512) * 1024**2
                if available is not None and available < required:
                    raise RuntimeError(f'RAM insufficiente prima del chunk {index}: {available/1024**3:.2f} GiB liberi, stima con riserva {required/1024**3:.2f} GiB. Cache incrementale conservata; nessun modello/processo esterno chiuso.')
                child = subprocess.Popen(command, cwd=paths['runner'].parent, env=env, stdout=log, stderr=subprocess.STDOUT)
                deadline = time.monotonic() + 600
                while child.poll() is None:
                    if time.monotonic() > deadline:
                        child.kill(); child.wait(); raise TimeoutError('Timeout runner Qwen del chunk ' + str(index))
                    peak = peak_working_set(child)
                    if peak is not None: peak_rss = max(peak_rss or 0, peak)
                    free = free_ram()
                    if free is not None: min_free_ram = min(min_free_ram or free, free)
                    time.sleep(.1)
            if child.returncode or not temporary.is_file(): raise RuntimeError('Runner Qwen non completato; vedi log del chunk ' + str(index))
            parsed = json.loads(temporary.read_text(encoding='utf-8'))
            if not isinstance(parsed.get('words'), list): raise RuntimeError('Runner senza word timestamps JSON: nessun fallback inventato')
            temporary.replace(output); audio.unlink(); ran += 1
        words = json.loads(output.read_text(encoding='utf-8'))['words']
        outputs.append((chunk, words))
    words = merge_words(outputs)
    if not words: raise RuntimeError('Nessun parlato con timestamp valido rilevato da Qwen')
    segments = []
    for index in range(0, len(words), 18):
        group = words[index:index+18]
        segments.append({'text': ' '.join(w['text'] for w in group), 'start_ms': group[0]['start_ms'], 'end_ms': max(w['end_ms'] for w in group)})
    result = {'language': request['language'], 'segments': segments, 'words': words, 'metadata': {'engine': 'qwen3_asr_cpp', 'adapter_version': VERSION, 'alignment_completed': True, 'models': models, 'source_sha256': source_identity['sha256'], 'parameters': parameters, 'elapsed_seconds': round(time.monotonic()-started, 3), 'chunks': len(chunks), 'inference_chunks': ran, 'peak_runner_rss_bytes': peak_rss, 'min_free_ram_bytes': min_free_ram, 'confidence': None}, 'cache_hit': False}
    result['metadata'].update(skip_facts(skipped))
    result['metadata']['alignment_diagnostics'] = alignment_diagnostics(cache, duration, config['chunk_seconds'])
    write_json(ready, result)
    report('Trascrizione Qwen completata', completed_chunks=len(chunks), total_chunks=len(chunks))
    return result


if __name__ == '__main__':
    request_path, output_path = map(Path, sys.argv[1:3])
    try:
        write_json(output_path, run(json.loads(request_path.read_text(encoding='utf-8'))))
    except Exception as error:
        write_json(output_path, {'error': str(error)})
        raise
