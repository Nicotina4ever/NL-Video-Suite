"""Real HTTP/FFmpeg/ClipsAI-bridge flow regression; explicitly simulated AI.

Only disposable synthetic media and a seeded transcript cache are used. This
does not certify ASR accuracy, LLM creativity or a real ComfyUI installation.
"""
import hashlib
from http.server import ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parent))
from smoke import FakeServices, ROOT, PYTHON, command, free_port


class SimulatedAI(FakeServices):
    clip_calls = 0
    graphics_calls = 0
    expected_clip = None
    fail_clipping = False

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        if self.path == "/prompt":
            assert body["prompt"]["1"]["inputs"]["text"] == "graphic test prompt"
            return self.reply({"prompt_id": "test-prompt"})
        assert self.path == "/api/chat", self.path  # no global /api/generate unload
        assert body["keep_alive"] == 0 and body["options"]["num_gpu"] == 0
        if "Sei un editor video" in body["messages"][0]["content"]:
            type(self).graphics_calls += 1
            contexts = json.loads(body["messages"][1]["content"])
            assert len(contexts) == 1 and contexts[0]["clip_id"] == self.expected_clip, contexts
            value = {"topic":"Illustrazione del parlato", "keywords":["microfono"], "graphics":[{
                "source_clip_id":contexts[0]["clip_id"],"offset_ms":100,"duration_ms":700,
                "prompt":"graphic test prompt","caption":"Grafica di prova","evidence":contexts[0]["timed_speech"][0]["text"]}]}
        else:
            type(self).clip_calls += 1
            if self.fail_clipping:
                self.send_response(401)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                return self.wfile.write(b'{"error":"fixture authentication required"}')
            value = {"clips":[{"title":"Proposta sintetica", "start_seg":1,"end_seg":3,"score":8,"reason":"Fixture, non valutazione artistica"}]}
        return self.reply({"message":{"content":json.dumps(value)}})


def main():
    exe = Path(sys.argv[1]).resolve() if len(sys.argv)>1 else ROOT / "sviluppo/target/release/nl-video-suite.exe"
    with tempfile.TemporaryDirectory(prefix="nl-suite-flow-http-") as directory:
        test = Path(directory).resolve()
        assert test.parent == Path(tempfile.gettempdir()).resolve()
        data, workflows = test/"data", test/"workflows"
        data.mkdir(); workflows.mkdir()
        source = test/"synthetic-video.mp4"
        command(ROOT/"app/tools/ffmpeg.exe", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=navy:s=160x90:r=10:d=25", "-f", "lavfi", "-i", "sine=frequency=440:duration=25", "-c:v", "libx264", "-threads", "1", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", source)
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        SimulatedAI.png = command(ROOT/"app/tools/ffmpeg.exe", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=green:s=96x64", "-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "pipe:1")
        ai = ThreadingHTTPServer(("127.0.0.1",0), SimulatedAI)
        threading.Thread(target=ai.serve_forever,daemon=True).start()
        ai_url = "http://127.0.0.1:"+str(ai.server_port)
        (data/"settings.json").write_text(json.dumps({"comfy_url":ai_url,"ollama_url":ai_url,"llm_model":"fake-fixture-model", "ollama_execution":"cpu","workflow_roots":[str(workflows)],"legacy_sources":{}}),encoding="utf-8")
        (workflows/"image.json").write_text(json.dumps({"1":{"class_type":"CLIPTextEncode","_meta":{"title":"Prompt (Input:prompt)"},"inputs":{"text":"original"}},"2":{"class_type":"SaveImage","inputs":{"filename_prefix":"fixture"}}}),encoding="utf-8")
        port, server, log = free_port(), None, (test/"server.log").open("w",encoding="utf-8")
        base = f"http://127.0.0.1:{port}"
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

        def http(path,method="GET",body=None,status=200):
            request=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None, method=method, headers={"Content-Type":"application/json"} if body is not None else {})
            try: response=opener.open(request,timeout=20)
            except urllib.error.HTTPError as error: response=error
            content=response.read()
            assert response.status==status,(method,path,response.status,content.decode(errors="replace"))
            return json.loads(content) if content else None

        def launch():
            process=subprocess.Popen([str(exe),"--port",str(port),"--data-dir",str(data)],cwd=ROOT,env={**os.environ,"NL_SUITE_ROOT":str(ROOT)},stdout=log,stderr=log)
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                try: http("/api/projects");return process
                except OSError: assert process.poll() is None;time.sleep(.05)
            raise AssertionError("server startup timeout")

        def wait(job,success=True):
            deadline=time.monotonic()+80
            while time.monotonic()<deadline:
                result=http("/api/jobs/"+job["id"])
                if result["status"] in ("succeeded","failed"):
                    assert (result["status"]=="succeeded")==success,result
                    return result
                time.sleep(.05)
            raise AssertionError("job timeout: "+str(job))

        try:
            server=launch()
            p=http("/api/projects","POST",{"title":"Synthetic guided-flow regression","aspect_ratio":"16:9"},201)
            prefix="/api/projects/"+p["id"]
            wait(http(prefix+"/assets","POST",{"path":str(source)},202))
            p=http(prefix);asset=p["assets"][0]
            # A previous timeline item must survive; graphic analysis must not
            # include it when the guided flow requests only its own cuts.
            http(prefix+"/timeline/clips","POST",{"asset_id":asset["id"],"trim_in_ms":0,"duration_ms":1000},201)
            work=data/"projects"/p["id"]/"analysis"/asset["id"];work.mkdir(parents=True)
            words=[{"start":i*.5,"end":i*.5+.45,"word":"microfono." if i%8==7 else "voce"} for i in range(48)]
            transcript={"language":"it","duration":25,"segments":[{"start":0,"end":24,"text":"Un microfono per la voce"}],"words":words}
            installed=data/"projects"/p["id"]/"assets"/asset["file_name"]
            seed="import os,sys,json,pathlib;os.environ['CLIPSAI_WHISPER_MODEL']=str(pathlib.Path('models/asr/faster-whisper-large-v3').resolve());sys.path.insert(0,str(pathlib.Path('engines/clipsai').resolve()));import motore;v,w,p=sys.argv[1:];x=json.loads(p);x['_fingerprint']=motore.video_fingerprint(v);x['_signature']=motore._cache_signature('it');pathlib.Path(w,pathlib.Path(v).stem+'_trascrizione.json').write_text(json.dumps(x),encoding='utf-8')"
            command(PYTHON,"-c",seed,installed,work,json.dumps(transcript))
            http(prefix+"/flows","POST",{"asset_id":asset["id"],"operation":"transcribe"},400)
            flow=http(prefix+"/flows","POST",{"asset_id":asset["id"],"operation":"clips"},202)
            path=prefix+"/flows/"+flow["flow"]["id"]
            analyzed=wait(flow["analysis_job"])
            assert analyzed["result"]["clips"], analyzed
            flow=http(prefix+"/flows")[0];assert flow["phase"]=="review_cuts",flow
            http(path+"/export","POST",status=400)
            p=http(prefix); assert len(p["timeline"]["clips"])==1
            # Stale revision and invalid source bounds must never append cuts.
            body={"items":[{"title":"Reviewed edit","segments":[[0,1],[2,3]]}],"expected_updated_at":p["updated_at"]}
            invalid={**body,"items":[{"start":0,"end":26}]}
            http(path+"/apply","POST",invalid,400)
            flow=http(path+"/apply","POST",body)
            p=http(prefix);assert len(p["timeline"]["clips"])==3
            assert [c["start_ms"] for c in p["timeline"]["clips"]]==[0,1000,2000]
            assert flow["phase"]=="ready"
            # Apply is idempotent even with its old expected revision.
            http(path+"/apply","POST",body);assert len(http(prefix)["timeline"]["clips"])==3
            print("PASS actual bridge/cache analysis, explicit review, append timing, invalid bounds, idempotent apply",flush=True)
            # Run an independent single-clip flow for contextual graphics.
            second=http(prefix+"/flows","POST",{"asset_id":asset["id"],"operation":"clips"},202);wait(second["analysis_job"])
            secondpath=prefix+"/flows/"+second["flow"]["id"]
            p=http(prefix);second=http(secondpath+"/apply","POST",{"items":[{"title":"For graphics","start":4,"end":6}],"expected_updated_at":p["updated_at"]})
            SimulatedAI.expected_clip=second["flow"]["applied_clip_ids"][0]
            workflow_id=http("/api/workflows")[0]["id"]
            second=http(secondpath+"/graphics","POST",{"workflow_id":workflow_id});wait(second["graphics_job"])
            second=next(f for f in http(prefix+"/flows") if f["flow"]["id"]==second["flow"]["id"])
            for job in second["generation_jobs"]:wait(job)
            second=next(f for f in http(prefix+"/flows") if f["flow"]["id"]==second["flow"]["id"])
            assert second["phase"]=="review_graphics",second
            http(secondpath+"/export","POST",status=400)
            before=len(http(prefix)["timeline"]["clips"])
            generation=second["generation_jobs"][0]
            http(secondpath+"/review","POST",{"job_id":generation["id"],"skip":True})
            assert len(http(prefix)["timeline"]["clips"])==before
            http(secondpath+"/review","POST",{"job_id":generation["id"]})
            http(secondpath+"/review","POST",{"job_id":generation["id"]})
            assert len(http(prefix)["timeline"]["clips"])==before+1
            exported=http(secondpath+"/export","POST");wait(exported["export_job"])
            print("PASS scoped graphic analysis, sequential jobs, review gate, exclude/approve/idempotence, real FFmpeg export",flush=True)
            # Persistent flow records survive a restart without resubmitting AI.
            calls=SimulatedAI.clip_calls
            server.terminate();server.wait(timeout=10);server=launch()
            restored=http(prefix+"/flows");assert any(f["phase"]=="complete" for f in restored)
            assert SimulatedAI.clip_calls==calls
            # The vendored engine catches this provider error; our bridge must
            # turn its empty proposal list into a truthful failed job.
            SimulatedAI.fail_clipping=True
            failed=http(prefix+"/flows","POST",{"asset_id":asset["id"],"operation":"clips"},202)
            result=wait(failed["analysis_job"],success=False)
            assert "provider LLM ha fallito" in result["message"] and "autenticazione" in result["message"]
            assert hashlib.sha256(source.read_bytes()).hexdigest()==source_hash
            print("PASS persistent restart, no hidden resubmit, provider failure not swallowed, source immutability",flush=True)
        finally:
            if server is not None and server.poll() is None:server.terminate();server.wait(timeout=20)
            log.close();ai.shutdown();ai.server_close()


if __name__=="__main__":main()
