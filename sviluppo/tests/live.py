"""Opt-in real-service verification. Creates only synthetic test media.

Requires idle local ComfyUI + Ollama and installed ClipsAI models. ComfyUI
round-trips a PNG (LoadImage -> SaveImage, no model sampling). WhisperX
transcribes synthetic Italian speech. Ollama writes a real storyboard draft.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import urllib.request
sys.path.insert(0,str(Path(__file__).resolve().parent))
from smoke import ROOT, command, free_port


def request(url, method="GET", body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method, headers={"Content-Type":"application/json"} if data else {})
    with urllib.request.urlopen(req, timeout=15) as response:
        return json.load(response)


def main():
    comfy_only="--comfy-only" in sys.argv
    queue = request("http://127.0.0.1:8188/queue")
    assert not queue["queue_running"] and not queue["queue_pending"], "ComfyUI is busy: live test not submitted"
    assert not request("http://127.0.0.1:11434/api/ps")["models"], "Ollama already has a loaded model: leave current session alone"
    report = {}
    with tempfile.TemporaryDirectory(prefix="nl-live-") as directory:
        work = Path(directory)
        data = work / "data"
        data.mkdir()
        (data / "settings.json").write_text(json.dumps({
            "comfy_url":"http://127.0.0.1:8188", "ollama_url":"http://127.0.0.1:11434",
            "legacy_sources": {name:str(work / ("absent-" + name)) for name in ("calliope","regista","clipsai","zp_shorts")}
        }), encoding="utf-8")
        image = work / "reference.png"
        command("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=blue:s=128x128", "-frames:v", "1", image)
        speech = work / "voce.wav"
        speech_path = str(speech).replace("'", "''")
        script = "$ErrorActionPreference='Stop';Add-Type -AssemblyName System.Speech;$s=New-Object System.Speech.Synthesis.SpeechSynthesizer;$s.SelectVoice('Microsoft Elsa Desktop');$s.SetOutputToWaveFile('" + speech_path + "');$s.Speak('Questa è una prova della nuova applicazione. La voce resta chiara e i sottotitoli seguono il parlato.');$s.Dispose()"
        command("powershell", "-NoProfile", "-Command", script)
        port = free_port()
        base = "http://127.0.0.1:" + str(port)
        log = (work / "server.log").open("w")
        process = subprocess.Popen([str(ROOT/"app/nl-video-suite.exe"), "--port",str(port),"--data-dir",str(data)],stdout=log,stderr=log)
        try:
            for _ in range(100):
                try:
                    request(base + "/api/projects")
                    break
                except OSError:
                    time.sleep(.1)
            def api(path, method="GET", body=None):
                return request(base+path,method,body)
            def wait(job, timeout=600):
                deadline = time.monotonic()+timeout
                while time.monotonic()<deadline:
                    value=api("/api/jobs/"+job["id"])
                    if value["status"] in ("failed","succeeded"):
                        assert value["status"]=="succeeded",value["message"]
                        return value
                    time.sleep(.5)
                raise AssertionError("live job timeout")
            p=api("/api/projects","POST",{"title":"Verifica reale sintetica"})
            prefix="/api/projects/"+p["id"]
            wait(api(prefix+"/assets","POST",{"path":str(image)}))
            wait(api(prefix+"/assets","POST",{"path":str(speech)}))
            p=api(prefix)
            picture=next(a for a in p["assets"] if a["kind"]=="image")
            voice=next(a for a in p["assets"] if a["kind"]=="audio")
            workflows=work/"workflows"
            workflows.mkdir()
            (workflows/"roundtrip.json").write_text(json.dumps({"1":{"class_type":"LoadImage","inputs":{"image":"reference.png"}},"2":{"class_type":"SaveImage","inputs":{"images":["1",0],"filename_prefix":"NL_Suite_Verifica"}}}))
            wait(api("/api/workflows/scan","POST",{"roots":[str(workflows)]}))
            wf=api("/api/workflows")[0]
            generated=wait(api("/api/comfy/submit","POST",{"workflow_path":wf["path"],"project_id":p["id"],"asset_inputs":{"1.image":picture["id"]}}))
            assert generated["output_paths"]
            report["comfyui"]={"result":"passed","scope":"real upload, prompt, history, download, project asset registration; no neural sampling"}
            print("PASS real ComfyUI: reference upload -> SaveImage -> download -> project",flush=True)
            if comfy_only:
                return
            transcribed=wait(api(prefix+"/pipeline","POST",{"operation":"transcribe","asset_id":voice["id"],"language":"it"}))
            text=" ".join(s["text"] for s in transcribed["result"]["segments"])
            assert len(text)>20,text
            report["whisperx"]={"result":"passed","transcript":text,"words":len(transcribed["result"].get("words",[])),"runtime":"suite-owned, legacy application paths unavailable"}
            if transcribed['result'].get('_nl_suite_alignment'):
                assert transcribed['result']['_nl_suite_alignment']['completed'] is True
                report['whisperx']['alignment']=transcribed['result']['_nl_suite_alignment']
            print("PASS real WhisperX: "+text,flush=True)
            story=wait(api(prefix+"/story/generate","POST",{"premise":"Una bibliotecaria trova una chiave in un libro e scopre un giardino segreto. Racconto breve con conclusione positiva, quattro scene."}))
            assert story["result"].get("scenes")
            report["ollama"]={"result":"passed","model":"qwen2.5:14b-instruct","scene_count":len(story["result"]["scenes"])}
            print("PASS real Ollama storyboard: "+str(len(story["result"]["scenes"]))+" scenes",flush=True)
        finally:
            process.terminate()
            process.wait(timeout=20)
            log.close()
            artifacts=Path(__file__).resolve().parent/"artifacts"
            artifacts.mkdir(exist_ok=True)
            (artifacts/("live-comfy-preflight-report.json" if comfy_only else "live-report.json")).write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding="utf-8")


if __name__=="__main__":
    main()
