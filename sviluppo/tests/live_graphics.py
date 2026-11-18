"""Opt-in end-to-end contextual graphic with real Ollama and Z-Image Turbo.

Retains the synthetic project and output in tests/artifacts/live-graphics for
visual review. Does not alter existing user projects or installed workflows.
"""
import json
from pathlib import Path
import subprocess
import sys
import time
sys.path.insert(0,str(Path(__file__).resolve().parent))
from live import request
from smoke import ROOT,command,free_port


def main():
    queue=request('http://127.0.0.1:8188/queue')
    assert not queue['queue_running'] and not queue['queue_pending'],'ComfyUI occupied; no test submitted'
    work=Path(__file__).resolve().parent/'artifacts/live-graphics'
    work.mkdir(parents=True,exist_ok=True)
    source=work/'synthetic.mp4'
    if not source.exists():
        command('ffmpeg','-v','error','-f','lavfi','-i','color=navy:s=160x90:r=30:d=8','-c:v','libx264','-pix_fmt','yuv420p',source)
    data=work/'data'
    port=free_port()
    base='http://127.0.0.1:'+str(port)
    log=(work/'server.log').open('w')
    process=subprocess.Popen([str(ROOT/'app/nl-video-suite.exe'),'--port',str(port),'--data-dir',str(data)],stdout=log,stderr=log)
    try:
        for _ in range(100):
            try:
                request(base+'/api/projects')
                break
            except OSError:time.sleep(.1)
        def api(path,method='GET',body=None):return request(base+path,method,body)
        def wait(job):
            start=time.monotonic();last=0
            while time.monotonic()-start<2400:
                j=api('/api/jobs/'+job['id'])
                if j['status'] in ('succeeded','failed'):
                    assert j['status']=='succeeded',j['message']
                    return j
                if time.monotonic()-last>20:
                    print(j['kind']+': '+j['message']+' ('+str(round(time.monotonic()-start))+'s)',flush=True);last=time.monotonic()
                time.sleep(1)
            raise AssertionError('Timeout; check ComfyUI before rerunning')
        p=api('/api/projects','POST',{'title':'Prova grafiche contestuali: legno e resina','aspect_ratio':'9:16'})
        prefix='/api/projects/'+p['id']
        wait(api(prefix+'/assets','POST',{'path':str(source)}))
        p=api(prefix);asset=p['assets'][0]
        api(prefix+'/transcripts/import','POST',{'asset_id':asset['id'],'transcript':{'segments':[{'start':0,'end':8,'text':'Oggi lavoriamo una tavola di legno e versiamo la resina trasparente. Le venature del legno restano visibili sotto la superficie lucida.'}]}})
        clip=api(prefix+'/timeline/clips','POST',{'asset_id':asset['id'],'duration_ms':8000})
        wait(api('/api/workflows/scan','POST',{'roots':[r'K:\AI\Regista_AI\workflow_comfy\image']}))
        workflow=next(w for w in api('/api/workflows') if w['name']=='z-image-turbo')
        planned=wait(api(prefix+'/graphics/suggest','POST',{'auto_generate':True,'workflow_id':workflow['id']}))
        print('TOPIC: '+planned['result']['topic'],flush=True)
        print('PROMPT: '+planned['result']['graphics'][0]['prompt'],flush=True)
        jobs=[wait(j) for j in planned['result']['generation_jobs']]
        project=api(prefix)
        overlays=[c for c in project['timeline']['clips'] if c['id']!=clip['id']]
        assert overlays and overlays[0]['duration_ms']>0
        report={'topic':planned['result']['topic'],'graphics':planned['result']['graphics'],'outputs':[file for j in jobs for file in j['output_paths']],'project_id':project['id'],'overlays':overlays}
        (work/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
        print('PASS real contextual graphic generated and linked to the source clip',flush=True)
    finally:
        process.terminate();process.wait(timeout=20);log.close()


if __name__=='__main__':main()
