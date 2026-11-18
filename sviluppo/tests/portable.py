"""Extract and exercise the actual portable archive in a different directory.

The extraction is retained in da_eliminare as a recoverable test copy. Tests
use a restricted PATH, empty model caches, unavailable AI services and no
downloads. This is a same-PC relocation test, not a clean-machine guarantee.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.request
import uuid
import zipfile
sys.path.insert(0,str(Path(__file__).resolve().parent))
from smoke import ROOT,free_port

def main():
    archive=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else sorted((ROOT/'distribuzione').glob('*.zip'))[-1]
    destination=ROOT/'da_eliminare'/('verifica portable con spazi '+uuid.uuid4().hex[:8])
    destination.mkdir(parents=True)
    print('Estrazione reale ZIP64 in '+str(destination),flush=True)
    with zipfile.ZipFile(archive) as package:
        names=package.namelist()
        assert not any('/sviluppo/' in n or '/da_eliminare/' in n or '/target/' in n for n in names)
        assert 'NL_Video_Suite/data/settings.json' in names
        for entry in package.infolist():
            target=(destination/entry.filename).resolve()
            assert target.is_relative_to(destination.resolve()),entry.filename
            package.extract(entry,destination)
    extracted=destination/'NL_Video_Suite'
    manifest=json.loads((extracted/'PACKAGE-MANIFEST.json').read_text(encoding='utf-8'))
    assert manifest['includes_personal_data'] is False
    for name in ['app/nl-video-suite.exe','models/asr/faster-whisper-large-v3/model.bin','models/alignment/italian-xlsr53/pytorch_model.bin']:
        record=next(v for v in manifest['files'] if v['path']==name)
        digest=hashlib.sha256()
        with (extracted/name).open('rb') as source:
            for chunk in iter(lambda:source.read(4*1024*1024),b''):digest.update(chunk)
        assert digest.hexdigest()==record['sha256'],name
    config=extracted/'data/settings.json'
    settings=json.loads(config.read_text());settings['comfy_url']='http://127.0.0.1:9';settings['ollama_url']='http://127.0.0.1:9'
    config.write_text(json.dumps(settings),encoding='utf-8')
    environment=os.environ.copy();environment.pop('NL_SUITE_ROOT',None)
    system=Path(os.environ['SystemRoot'])
    environment['PATH']=str(system/'System32')+';'+str(system)
    environment['HF_HOME']=str(extracted/'models/huggingface')
    environment['TORCH_HOME']=str(extracted/'models/torch')
    environment['HF_HUB_OFFLINE']='1';environment['TRANSFORMERS_OFFLINE']='1';environment['PYTHONDONTWRITEBYTECODE']='1'
    environment['HTTP_PROXY']='http://127.0.0.1:9';environment['HTTPS_PROXY']='http://127.0.0.1:9';environment['NO_PROXY']='127.0.0.1,localhost'
    video=destination/'synthetic.mp4';speech=destination/'voce.wav'
    subprocess.run([str(extracted/'app/tools/ffmpeg.exe'),'-v','error','-f','lavfi','-i','color=navy:s=96x64:r=30:d=1','-c:v','libx264','-pix_fmt','yuv420p',str(video)],check=True,env=environment)
    subprocess.run([str(system/'System32/WindowsPowerShell/v1.0/powershell.exe'),'-NoProfile','-File',str(Path(__file__).resolve().parent/'create_test_voice.ps1'),'-Output',str(speech)],check=True,env=environment)
    port=free_port();base=f'http://127.0.0.1:{port}';opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def api(path,method='GET',body=None):
        req=urllib.request.Request(base+path,json.dumps(body).encode() if body is not None else None,method=method,headers={'Content-Type':'application/json'} if body is not None else {})
        return json.load(opener.open(req,timeout=20))
    def wait(job):
        deadline=time.monotonic()+900
        while time.monotonic()<deadline:
            value=api('/api/jobs/'+job['id'])
            if value['status'] in ('failed','succeeded'):
                assert value['status']=='succeeded',value['message'];return value
            time.sleep(1)
        raise AssertionError('Portable job timeout')
    log=(destination/'server.log').open('w')
    process=subprocess.Popen([str(extracted/'app/nl-video-suite.exe'),'--port',str(port)],cwd=system/'System32',env=environment,stdout=log,stderr=log)
    try:
        for _ in range(100):
            try:health=api('/api/health');break
            except OSError:time.sleep(.1)
        assert Path(health['installation_root'])==extracted
        assert health['version']==manifest['version'],(health['version'],manifest['version'])
        assert health['ffmpeg'] and not health['ollama'] and not health['comfyui']
        runtime=api('/api/runtime');assert runtime['asr_model_available'] and runtime['italian_alignment_available']
        assert all(not Path(w['path']).is_absolute() for w in api('/api/workflows'))
        project=api('/api/projects','POST',{'title':'Collaudo portable indipendente'});prefix='/api/projects/'+project['id']
        for source in (video,speech):wait(api(prefix+'/assets','POST',{'path':str(source)}))
        project=api(prefix);picture=next(a for a in project['assets'] if a['kind']=='video');voice=next(a for a in project['assets'] if a['kind']=='audio')
        api(prefix+'/timeline/clips','POST',{'asset_id':picture['id'],'duration_ms':1000})
        exported=wait(api(prefix+'/export','POST',{}));assert exported['output_paths']
        print('PASS portable: root rilevata dall exe, PATH ridotto, FFmpeg incluso, catalogo relativo, export reale',flush=True)
        transcribed=wait(api(prefix+'/pipeline','POST',{'asset_id':voice['id'],'operation':'transcribe','language':'it'}))
        result=transcribed['result'];assert len(result['words'])>=10
        assert result['_nl_suite_alignment']['completed'] is True,result['_nl_suite_alignment']
        assert str(extracted) in result['_nl_suite_alignment']['model_path']
        report={'version':health['version'],'archive':archive.name,'archive_files':len(names),'installation':str(extracted),'restricted_path':True,'external_services_unavailable':True,'offline_italian_transcript':' '.join(s['text'] for s in result['segments']),'words':len(result['words']),'alignment':result['_nl_suite_alignment'],'scope':'Actual ZIP extraction, same Windows PC, not a clean-PC certification'}
        artifacts=Path(__file__).resolve().parent/'artifacts';artifacts.mkdir(exist_ok=True)
        (artifacts/'portable-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
        print('PASS portable: WhisperX + allineamento italiano reali offline, senza D: o cache utente',flush=True)
    finally:
        process.terminate();process.wait(timeout=30);log.close()
    (destination/'LEGGIMI-TEST.txt').write_text('Copia estratta soltanto per il collaudo, con media sintetici. Eliminabile manualmente; il pacchetto ZIP originale e la suite operativa restano intatti.\n',encoding='utf-8')

if __name__=='__main__':main()
