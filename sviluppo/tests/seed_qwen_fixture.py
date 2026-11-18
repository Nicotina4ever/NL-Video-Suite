"""Seed a declared synthetic cache receipt, never run an ASR model or runner."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

root, project, source, config_path = map(Path, sys.argv[1:])
spec = importlib.util.spec_from_file_location('qwen_fixture', root/'sviluppo/bridge/qwen_asr_bridge.py')
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)
config = json.loads(config_path.read_text(encoding='utf-8'))
identities = project/'asr-identities'
identities.mkdir(exist_ok=True)
models = {k: bridge.identity(Path(config[k]).resolve(), identities) for k in ('runner', 'model', 'aligner')}
media = bridge.identity(source.resolve(), identities)
parameters = dict(language='it', chunk_seconds=config['chunk_seconds'], threads=config['threads'], noise_db=-38, silence_seconds=.5, overlap_ms=750)
key = hashlib.sha256(json.dumps(dict(models={k:v['sha256'] for k,v in models.items()}, source=media['sha256'], parameters=parameters, version=bridge.VERSION), sort_keys=True).encode()).hexdigest()
cache = project/'analysis/qwen'/key
cache.mkdir(parents=True, exist_ok=True)
words = [dict(text='voce' if i%3 else 'microfono', start_ms=i*200, end_ms=i*200+150) for i in range(24)]
value = dict(language='it', words=words, segments=[dict(text=' '.join(w['text'] for w in words), start_ms=0, end_ms=4750)], metadata=dict(engine='qwen3_asr_cpp', adapter_version=bridge.VERSION, models=models, parameters=parameters, source_sha256=media['sha256'], alignment_completed=True, inference_chunks=0, fixture_simulated_cache=True), cache_hit=True)
(cache/'result.json').write_text(json.dumps(value), encoding='utf-8')
(cache/'silences.json').write_text('[]', encoding='utf-8')
print(json.dumps(dict(cache=str(cache), inference=False, synthetic_receipt=True)))
