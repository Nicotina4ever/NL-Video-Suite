"""One-time assembly; sources stay untouched. No pip install/upgrade."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

ROOT=Path(__file__).resolve().parents[1]
if ROOT.name=='sviluppo':ROOT=ROOT.parent
REVISION='dab04a3e00d8326052f3fb22a6ff276b822f6131'
REPO='jonatasgrosman/wav2vec2-large-xlsr-53-italian'

def download(url,target):
    if target.exists():return
    target.parent.mkdir(parents=True,exist_ok=True)
    partial=target.with_name(target.name+'.partial')
    with urllib.request.urlopen(url,timeout=60) as response,partial.open('wb') as output:
        size=0
        while True:
            chunk=response.read(4*1024*1024)
            if not chunk:break
            output.write(chunk);size+=len(chunk)
            if size%(128*1024*1024)<len(chunk):print(f'Download {target.name}: {size//(1024*1024)} MiB',flush=True)
    partial.replace(target)

def main():
    tools=ROOT/'app/tools';tools.mkdir(parents=True,exist_ok=True)
    # Resolve WinGet symlinks to copy actual binaries, never portable links.
    ffmpeg=Path(shutil.which('ffmpeg')).resolve()
    ffroot=ffmpeg.parent.parent
    for name in ('ffmpeg.exe','ffprobe.exe'):
        target=tools/name
        if not target.exists():shutil.copy2(ffmpeg.parent/name,target)
    notices=ROOT/'licenses/ffmpeg';notices.mkdir(parents=True,exist_ok=True)
    for name in ('LICENSE','README.txt'):
        if (ffroot/name).is_file():shutil.copy2(ffroot/name,notices/name)
    asr_source=Path(r'D:\Faster-Whisper-XXL\Faster-Whisper-XXL\_models\faster-whisper-large-v3')
    asr=ROOT/'models/asr/faster-whisper-large-v3'
    if not asr.exists():shutil.copytree(asr_source,asr)
    alignment=ROOT/'models/alignment/italian-xlsr53'
    for name in ('config.json','preprocessor_config.json','special_tokens_map.json','vocab.json','pytorch_model.bin','README.md'):
        download(f'https://huggingface.co/{REPO}/resolve/{REVISION}/{name}',alignment/name)
    download('https://www.apache.org/licenses/LICENSE-2.0.txt',ROOT/'licenses/models/ALIGNMENT-APACHE-2.0.txt')
    download('https://raw.githubusercontent.com/openai/whisper/main/LICENSE',ROOT/'licenses/models/WHISPER-MIT.txt')
    manifest={'version':'0.4.0','alignment':{'repository':REPO,'revision':REVISION,'license_declared':'apache-2.0'},'files':[]}
    for directory in (asr,alignment,tools):
        for file in sorted(directory.rglob('*')):
            if file.is_file():
                digest=hashlib.sha256()
                with file.open('rb') as source:
                    for chunk in iter(lambda:source.read(4*1024*1024),b''):digest.update(chunk)
                manifest['files'].append({'path':file.relative_to(ROOT).as_posix(),'bytes':file.stat().st_size,'sha256':digest.hexdigest()})
    (ROOT/'models/manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
    print('Tool e modelli portable preparati; nessun originale modificato.',flush=True)

if __name__=='__main__':main()
