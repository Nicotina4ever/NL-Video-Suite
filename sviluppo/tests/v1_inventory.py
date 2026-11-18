"""Read-only M0 media/model inventory. Streams hashes; never copies weights/media."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def main():
    result = {'date': datetime.now(timezone.utc).isoformat(), 'read_only_sources': True, 'media': [], 'models': []}
    for source in sorted((ROOT / 'ZMateriale').iterdir()):
        if not source.is_file():
            continue
        record = {'path': str(source), 'bytes': source.stat().st_size, 'sha256': digest(source)}
        if source.suffix.lower() in ('.mp4', '.mov', '.mkv', '.wav', '.mp3', '.flac', '.m4a'):
            probe = subprocess.run([str(ROOT / 'app/tools/ffprobe.exe'), '-v', 'error', '-show_format', '-show_streams', '-of', 'json', str(source)], capture_output=True, check=True)
            value = json.loads(probe.stdout)
            record['duration_seconds'] = float(value.get('format', {}).get('duration', 0))
            record['streams'] = [{k: s[k] for k in ('codec_type', 'codec_name', 'width', 'height', 'r_frame_rate', 'sample_rate', 'channels', 'pix_fmt', 'color_space', 'color_transfer', 'tags') if k in s} for s in value.get('streams', [])]
        result['media'].append(record)
    model_root = Path('J:/ZLinkdaC/Qwen3ASR/models')
    for name in ('qwen3-asr-1.7b-q8_0.gguf', 'qwen3-forced-aligner-0.6b-q4_k_m.gguf'):
        path = model_root / name
        entry = {'path': str(path), 'exists': path.is_file(), 'inference_verified': False}
        if path.is_file():
            entry.update(bytes=path.stat().st_size, sha256=digest(path))
            with path.open('rb') as stream:
                header = stream.read(24)
            if len(header) == 24:
                magic, version, tensors, metadata = struct.unpack('<4sIQQ', header)
                entry.update(magic=magic.decode('ascii', errors='replace'), gguf_version=version, tensor_count=tensors, metadata_count=metadata)
        result['models'].append(entry)
    result['authentic_longform_available'] = any(v.get('duration_seconds', 0) >= 1800 for v in result['media'])
    result['longform_gap'] = None if result['authentic_longform_available'] else 'ZMateriale contiene reel finiti, non un video autentico 30–60min. Test longform sintetici possibili; gate autentico lungo ancora aperto.'
    out = ROOT / 'sviluppo/tests/artifacts/v1-inventory.json'
    out.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(str(out), flush=True)


if __name__ == '__main__':
    main()
