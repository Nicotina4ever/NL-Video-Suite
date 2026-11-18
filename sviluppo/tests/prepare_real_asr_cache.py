"""Reuse the already verified 8s Qwen result, refusing model/runner changes."""
from pathlib import Path
import hashlib
import json
import shutil
import sys

old, project, source = map(Path, sys.argv[1:])
identities = [json.loads(p.read_text()) for p in (old/'asr-identities').glob('*.json')]
for identity in identities:
    path = Path(identity['path'])
    if path.suffix.lower() not in ('.gguf', '.exe'):
        continue
    stat = path.stat()
    if stat.st_size != identity['bytes'] or stat.st_mtime_ns != identity['mtime_ns']:
        raise RuntimeError('Cached real Qwen model/runner changed; refuse another inference: '+str(path))
sha = hashlib.sha256(source.read_bytes()).hexdigest()
ready = list((old/'analysis/qwen').glob('*/result.json'))
if len(ready) != 1 or json.loads(ready[0].read_text())['metadata']['source_sha256'] != sha:
    raise RuntimeError('Authentic sample does not match cached inference')
for folder in ('analysis/qwen', 'asr-identities'):
    for path in (old/folder).rglob('*'):
        if path.is_file() and path.stat().st_size > 4*1024**2:
            raise RuntimeError('Refusing a heavy copy: '+str(path))
    shutil.copytree(old/folder, project/folder, dirs_exist_ok=True)
print(json.dumps(dict(real_qwen_reused=True, source_sha256=sha, new_inference=False)))
