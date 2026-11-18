"""Read-only live-ComfyUI preflight of installed presets; never submits /prompt.

Writes a bounded diagnosis report. Technical readiness is not an image-quality
test and does not establish sufficient VRAM for neural execution.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.request
sys.path.insert(0,str(Path(__file__).resolve().parent))
from smoke import ROOT, free_port

def main():
    exe=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else ROOT/'app/nl-video-suite.exe'
    def remote(url,body=None):
        request=urllib.request.Request(url,json.dumps(body).encode() if body is not None else None,headers={'Content-Type':'application/json'} if body is not None else {})
        return json.load(urllib.request.urlopen(request,timeout=60))
    # Existing ComfyUI is only queried, not started/reconfigured/cleared.
    remote('http://127.0.0.1:8188/system_stats')
    with tempfile.TemporaryDirectory(prefix='nl-doctor-') as directory:
        port=free_port();base=f'http://127.0.0.1:{port}';data=Path(directory)/'data'
        data.mkdir()
        (data/'settings.json').write_text(json.dumps({'comfy_url':'http://127.0.0.1:8188','ollama_url':'http://127.0.0.1:11434','legacy_sources':{}}))
        with (Path(directory)/'server.log').open('w') as log:
            process=subprocess.Popen([str(exe),'--port',str(port),'--data-dir',str(data)],cwd=ROOT,stdout=log,stderr=log)
            try:
                for _ in range(100):
                    try: remote(base+'/api/health');break
                    except OSError: time.sleep(.1)
                workflows=remote(base+'/api/workflows')
                selected=[w for w in workflows if '/image/' in w['path'].replace('\\','/') or ('LTX-I2V-1280' in w['name'])]
                report=[]
                for workflow in selected:
                    diagnosis=remote(base+'/api/workflows/diagnose',{'workflow_id':workflow['id']})
                    report.append({'name':workflow['name'],'path':workflow['path'],**diagnosis})
                    errors=[i['code']+':'+i['node_id']+'.'+i['input'] for i in diagnosis['issues'] if i['severity']=='error']
                    print(('READY' if diagnosis['ready'] else 'BLOCKED')+' '+workflow['name']+' '+', '.join(errors[:8]),flush=True)
                artifacts=Path(__file__).resolve().parent/'artifacts';artifacts.mkdir(exist_ok=True)
                (artifacts/'workflow-audit.json').write_text(json.dumps({'scope':'read-only live schema check, no neural execution or quality certification','workflows':report},ensure_ascii=False,indent=2),encoding='utf-8')
                assert any(w['name']=='image_flux2_text_to_image_9b' and w['ready'] for w in report),'Inspect installed Flux2 readiness in report'
                print('PASS independent live preflight of installed presets; no GPU sampling',flush=True)
            finally:
                process.terminate();process.wait(timeout=20)

if __name__=='__main__':main()
