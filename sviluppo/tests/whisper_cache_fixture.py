"""Seed only a declared synthetic cache using the native profile-cache contract.

No model is loaded: only dependency stat data and the existing engine signature.
The cache partition must change if model, aligner, bridge, source or language changes.
"""
import json
import os
from pathlib import Path
import sys
import uuid


def profile_key(source, model, aligner):
    def stamp(path):
        path = Path(path)
        if not path.exists():
            return {'path': None, 'length': None, 'modified': None}
        canonical = str(path.resolve())
        if os.name == 'nt' and not canonical.startswith('\\\\?\\'):
            canonical = '\\\\?\\' + canonical
        stat = path.stat()
        return {'path': canonical, 'length': stat.st_size, 'modified': str(stat.st_mtime_ns)}
    dependencies = [model, aligner, model / 'config.json', model / 'model.bin',
                    model / 'tokenizer.json', aligner / 'config.json', aligner / 'pytorch_model.bin']
    payload = {'adapter': 'whisperx-profile-cache-v1',
               'bridge': Path('sviluppo/bridge/clipsai_bridge.py').read_bytes().decode('utf-8'),
               'source': stamp(source), 'dependencies': [stamp(p) for p in dependencies], 'language': 'it'}
    return str(uuid.uuid5(uuid.NAMESPACE_OID, json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(',', ':'))))


if __name__ == '__main__':
    source, work, raw = sys.argv[1:]
    model = Path('models/asr/faster-whisper-large-v3').resolve()
    aligner = Path('models/alignment/italian-xlsr53').resolve()
    os.environ['CLIPSAI_WHISPER_MODEL'] = str(model)
    sys.path.insert(0, str(Path('engines/clipsai').resolve()))
    import motore
    directory = Path(work) / 'asr' / profile_key(source, model, aligner)
    directory.mkdir(parents=True)
    value = json.loads(raw)
    value['_fingerprint'] = motore.video_fingerprint(source)
    value['_signature'] = motore._cache_signature('it')
    (directory / (Path(source).stem + '_trascrizione.json')).write_text(json.dumps(value), encoding='utf-8')
