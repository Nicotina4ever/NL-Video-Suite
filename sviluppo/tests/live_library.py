"""Opt-in single real ComfyUI background reference test. No downloads or LLM.

Separate project/settings, existing Flux2 Klein 9B workflow, no auto-approval.
Run with --run-real-comfy only when the user has made ComfyUI available.

A closed console used to abort the run through the periodic telemetry print;
finally then terminated the suite while it was still polling ComfyUI, so a
produced image was never imported. Output is now failure-proof, an expired
deadline raises an explicit TimeoutError and any failure records a traceback.
--verify-existing <work> reopens a previous artifact folder to check that a
restart reports the truth instead of inventing a result; it never submits.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import traceback
import uuid
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from live import request
from smoke import free_port
from live_narrative import available_ram

ROOT = Path(__file__).resolve().parents[2]
EXE = ROOT / 'sviluppo/target/release/nl-video-suite.exe'


def say(text):
    """Telemetry must never abort a paid GPU run on a detached console."""
    try:
        print(text, flush=True)
    except (OSError, ValueError):
        pass


def option(name):
    if name not in sys.argv:
        return None
    index = sys.argv.index(name)
    assert index + 1 < len(sys.argv), name + ' requires a path'
    return Path(sys.argv[index + 1]).resolve()


def launch(data, work):
    log = (work / 'server.log').open('a', encoding='utf-8')
    port = free_port()
    process = subprocess.Popen([str(EXE), '--port', str(port), '--data-dir', str(data)], cwd=ROOT,
        env={**os.environ, 'NL_SUITE_ROOT': str(ROOT)}, stdout=log, stderr=log)
    return process, log, 'http://127.0.0.1:' + str(port)


def wait(api, job, timeout):
    """Poll one job; an expired deadline names the job instead of failing later."""
    deadline = time.monotonic() + timeout
    last = 0.0
    samples = []
    while time.monotonic() < deadline:
        done = api('/api/jobs/' + job['id'])
        if time.monotonic() - last > 20:
            last = time.monotonic()
            try:
                gpu = subprocess.check_output(['nvidia-smi', '--query-gpu=memory.used',
                    '--format=csv,noheader,nounits'], text=True, stderr=subprocess.DEVNULL).strip()
            except (OSError, subprocess.SubprocessError):
                gpu = 'unavailable'
            samples.append({'free_ram_gib': round(available_ram() / 1024 ** 3, 2), 'gpu_mib': gpu})
            say(done['status'] + ': ' + done['message'] + ' | ' + json.dumps(samples[-1]))
        if done['status'] in ('failed', 'succeeded'):
            done['memory_samples'] = samples
            return done
        time.sleep(1)
    raise TimeoutError('ComfyUI job still active after ' + str(timeout) + 's; check it before retrying: ' + job['id'])


def verify_existing(work):
    """Reopen an interrupted artifact: the suite must not claim an import it lost."""
    data = work / 'data'
    assert (data / 'projects').is_dir(), 'Not an interrupted live-library artifact: ' + str(work)
    report = {'real_inference': False, 'resumed': True, 'review_required': True, 'work': str(work)}
    process, log, base = launch(data, work)
    def api(tail, method='GET', body=None): return request(base + tail, method, body)
    try:
        for _ in range(100):
            assert process.poll() is None, 'Test server exited; inspect its log'
            try: api('/api/projects'); break
            except OSError: time.sleep(.1)
        projects = api('/api/projects')
        assert projects, 'No project in the interrupted artifact'
        project = projects[0]
        prefix = '/api/projects/' + project['id']
        report['project_id'] = project['id']
        jobs = api('/api/jobs')
        report['jobs'] = [{'id': j['id'], 'kind': j['kind'], 'status': j['status'], 'message': j['message']} for j in jobs]
        assert jobs and all(j['status'] != 'running' for j in jobs), 'A restarted suite left jobs running: ' + json.dumps(report['jobs'])
        assert all('Interrotto dal riavvio' in j['message'] for j in jobs), 'Restart did not explain the interruption'
        production = api(prefix + '/narrative')
        report['production_status'] = production['status']
        assert production['status'] == 'interrupted', production['status']
        assert production['references'] and production['references'][0]['asset_id'] is None, 'A lost result was reported as imported'
        assets = api(prefix)['assets']
        report['asset_count'] = len(assets)
        assert not assets, 'Files were registered without an imported job: ' + json.dumps(assets)
        report['passed'] = True
        say('PASS resumed artifact: interrupted jobs are reported honestly, no asset invented')
    finally:
        (work / 'report-resume.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
        process.terminate(); process.wait(timeout=20); log.close()


def main():
    existing = option('--verify-existing')
    if existing:
        return verify_existing(existing)
    assert '--run-real-comfy' in sys.argv, 'Explicit opt-in required'
    queue = request('http://127.0.0.1:8188/queue')
    assert not queue['queue_running'] and not queue['queue_pending'], 'ComfyUI busy: no submission'
    assert available_ram() > 7 * 1024**3, 'Not enough free RAM for this test'
    work = option('--work-dir') or (ROOT / 'sviluppo/tests/artifacts' / ('live-library-' + str(uuid.uuid4())[:8]))
    data = work / 'data'; data.mkdir(parents=True, exist_ok=True)
    (data / 'settings.json').write_text(json.dumps({'comfy_url': 'http://127.0.0.1:8188', 'ollama_url': 'http://127.0.0.1:11434', 'llm_model': 'qwen2.5:14b-instruct', 'legacy_sources': {}}), encoding='utf-8')
    say('ARTIFACTS ' + str(work))
    process, log, base = launch(data, work)
    report = {'real_inference': True, 'model': 'existing workflow Flux2 Klein 9B', 'review_required': True, 'work': str(work)}
    def api(tail, method='GET', body=None): return request(base + tail, method, body)
    try:
        for _ in range(100):
            assert process.poll() is None, 'Test server exited; inspect its log'
            try: api('/api/projects'); break
            except OSError: time.sleep(.1)
        p = api('/api/projects', 'POST', {'title': 'Collaudo libreria · sfondo originale', 'aspect_ratio': '16:9'})
        prefix = '/api/projects/' + p['id']; report['project_id'] = p['id']
        wf = next(w for w in api('/api/workflows') if w['name'] == 'image_flux2_text_to_image_9b')
        wf_path = ROOT / wf['path']; before = hashlib.sha256(wf_path.read_bytes()).hexdigest()
        prod = api(prefix + '/narrative')
        # Dimensions are deliberately not overridden: the suite derives them from
        # the project video (or its delivery format) at 1.5x, capped at 4096.
        prod['references'] = [{'id': str(uuid.uuid4()), 'kind': 'location', 'name': 'Studio narrativo di un faro', 'notes': 'Ambiente originale per una storia, senza personaggi né marchi.', 'prompt': 'Wide interior background plate of a small lighthouse study at dawn. An empty oak desk on the left, a closed notebook, an unlit brass lamp. Curved pale stone walls, one round window on the right showing the distant sea. Soft cool morning light through the window, subtle warm reflections on wood. Quiet cinematic editorial illustration, restrained teal and amber accents, clear negative space in the centre for a future character. No people, no faces, no text, no logos, no lettering. One continuous room, natural perspective.', 'workflow': {'workflow_id': wf['id'], 'text_input': '75:74.text', 'inputs': {'9.filename_prefix': 'NL_Suite_Library_Test'}, 'asset_inputs': {}}}]
        prod = api(prefix + '/narrative', 'PUT', {'production': prod})
        job = api(prefix + '/narrative/references/generate', 'POST', {'reference_id': prod['references'][0]['id']})
        report['job'] = wait(api, job, 1500)
        assert report['job']['status'] == 'succeeded', report['job']
        prod = api(prefix + '/narrative'); p = api(prefix)
        r = prod['references'][0]; assert r['asset_id'] and not r['approved']
        a = next(a for a in p['assets'] if a['id'] == r['asset_id'])
        assert not p['timeline']['clips'] and not prod['scenes']
        assert hashlib.sha256(wf_path.read_bytes()).hexdigest() == before
        report['workflow_sha256'] = before
        report['output'] = str(data / 'projects' / p['id'] / 'assets' / a['file_name'])
        report['media'] = a.get('media')
        report['passed'] = True
        say('PASS real reference generation; output awaits review: ' + report['output'])
    except Exception as error:
        report['passed'] = False; report['error'] = str(error); report['traceback'] = traceback.format_exc()
        try: report['server_log_tail'] = (work / 'server.log').read_text(encoding='utf-8', errors='replace')[-4000:]
        except OSError: pass
        raise
    finally:
        (work / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
        process.terminate(); process.wait(timeout=20); log.close()


if __name__ == '__main__': main()
