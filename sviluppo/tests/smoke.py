"""Executable regression suite: HTTP + real FFmpeg + installed legacy parsers.

All media and project records are synthetic and live in a temporary directory.
Ollama and ComfyUI are simulated explicitly: this tests protocol handling and
asset ownership, not model quality or compatibility of installed GPU workflows.
"""
import array
import contextlib
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]
if ROOT.name == 'sviluppo':
    ROOT = ROOT.parent
PYTHON = ROOT / "runtime/python/python.exe"


def command(*args):
    return subprocess.run(list(map(str, args)), check=True, capture_output=True).stdout


def free_port():
    with socket.socket() as connection:
        connection.bind(("127.0.0.1", 0))
        return connection.getsockname()[1]


class FakeServices(BaseHTTPRequestHandler):
    png = b""
    prompt_count = 0
    story_gate = None
    def log_message(self, *args):
        pass

    def reply(self, value, content_type="application/json"):
        body = json.dumps(value).encode() if content_type == "application/json" else value
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/object_info":
            return self.reply({
                "CLIPTextEncode":{"input":{"required":{"text":["STRING"]}},"output":["CONDITIONING"]},
                "SaveImage":{"input":{"required":{"filename_prefix":["STRING"]}},"output_node":True,"output":[]},
                "EmptyLTXVLatentVideo":{"input":{"required":{"width":["INT",{"max":16384}]}},"output":["LATENT"]},
                "LTXVSeparateAVLatent":{"input":{"required":{"av_latent":["LATENT"]}},"output":["LATENT","LATENT"]},
                "SaveVideo":{"input":{"required":{"samples":["LATENT"]}},"output_node":True,"output":[]}
            })
        if self.path.startswith("/view"):
            return self.reply(self.png, "image/png")
        if self.path.startswith("/history/"):
            return self.reply({"test-prompt": {"status": {"completed": True, "status_str": "success"}, "outputs": {"2": {"images": [{"filename": "synthetic.png", "type": "output", "subfolder": ""}]}}}})
        self.reply({"models": [{"name": "synthetic-test-model"}]})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        if self.path == "/prompt":
            type(self).prompt_count += 1
            assert body["prompt"]["1"]["inputs"]["text"] in ("test prompt","graphic test prompt")
            return self.reply({"prompt_id": "test-prompt"})
        if "Sei un editor video" in body["messages"][0]["content"]:
            contexts=json.loads(body["messages"][1]["content"])
            if 'no-graphics-test' in body['messages'][0]['content']:
                return self.reply({'message':{'content':json.dumps({'topic':'Performance vocale','keywords':[],'graphics':[]})}})
            return self.reply({"message":{"content":json.dumps({"topic":"Tema del parlato","keywords":["parlato"],"graphics":[{"source_clip_id":contexts[0]["clip_id"],"offset_ms":0,"duration_ms":500,"prompt":"graphic test prompt","caption":"Grafica contestuale","evidence":contexts[0]["timed_speech"][0]["text"]}]})}})
        if type(self).story_gate is not None:
            type(self).story_gate.wait(timeout=10)
        self.reply({"message": {"content": json.dumps({"premise": "Una storia sintetica", "characters": [], "locations": [], "scenes": [{"title": "Scena di prova", "description": "Un test."}]})}})


def main():
    exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "app/nl-video-suite.exe"
    with tempfile.TemporaryDirectory(prefix="nl-suite-test-") as directory:
        test = Path(directory)
        data = test / "data"
        data.mkdir()
        video, image, audio = test / "source.mp4", test / "green.png", test / "tone.wav"
        command("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=red:s=96x64:r=30:d=1", "-f", "lavfi", "-i", "color=blue:s=96x64:r=30:d=1", "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0[v]", "-map", "[v]", "-c:v", "libx264", "-pix_fmt", "yuv420p", video)
        command("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=lime:s=96x64", "-frames:v", "1", image)
        command("ffmpeg", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", audio)
        master_hash = hashlib.sha256(video.read_bytes()).hexdigest()
        FakeServices.png = image.read_bytes()
        services = ThreadingHTTPServer(("127.0.0.1", 0), FakeServices)
        thread = threading.Thread(target=services.serve_forever, daemon=True)
        thread.start()
        service_url = "http://127.0.0.1:" + str(services.server_port)
        settings = {"comfy_url": service_url, "ollama_url": service_url,
                    "llm_model": "synthetic-test-model", "legacy_sources": {
                        "calliope": str(test / "absent-calliope"), "regista": str(test / "absent-regista"),
                        "clipsai": str(test / "absent-clipsai"), "zp_shorts": str(test / "absent-zp")}}
        (data / "settings.json").write_text(json.dumps(settings), encoding="utf-8")
        port = free_port()
        base = "http://127.0.0.1:" + str(port)
        log = (test / "server.log").open("w")
        server = subprocess.Popen([str(exe), "--port", str(port), "--data-dir", str(data)], stdout=log, stderr=log)

        def http(path, method="GET", body=None, status=200, headers=None, raw=False):
            payload = json.dumps(body).encode() if body is not None else None
            req = urllib.request.Request(base + path, data=payload, method=method,
                headers={**({"Content-Type": "application/json"} if body is not None else {}), **(headers or {})})
            try:
                response = urllib.request.urlopen(req, timeout=40)
            except urllib.error.HTTPError as error:
                response = error
            content = response.read()
            assert response.status == status, (path, response.status, content.decode(errors="replace"))
            return (content if raw else json.loads(content)) if content else None

        def wait_job(job):
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                value = http("/api/jobs/" + job["id"])
                if value["status"] in ("succeeded", "failed"):
                    assert value["status"] == "succeeded", value["message"]
                    return value
                time.sleep(0.1)
            raise AssertionError("job timeout: " + job["id"])

        try:
            for _ in range(100):
                try:
                    http("/api/projects")
                    break
                except (OSError, urllib.error.URLError):
                    if server.poll() is not None:
                        raise RuntimeError((test / "server.log").read_text())
                    time.sleep(0.05)
            assert b"Analisi e clipping" in http("/", raw=True)
            assert b"initialise" in http("/app.js", raw=True)
            runtime = http("/api/runtime")
            assert runtime["engine_runtime_installed"] and not runtime["legacy_workflow_references"], runtime
            assert runtime["old_projects_safe_to_delete"] is False
            original_settings=http('/api/settings')
            editable={key:original_settings[key] for key in ('comfy_url','ollama_url','llm_model')}
            changed=http('/api/settings','PUT',{**editable,'llm_model':'modello-locale-test','ollama_url':editable['ollama_url']+'/'})
            assert changed['llm_model']=='modello-locale-test'
            assert http('/api/settings')['llm_model']=='modello-locale-test'
            assert changed['legacy_sources']==original_settings['legacy_sources']
            assert changed['python_executable']==original_settings['python_executable']
            assert json.loads((data/'settings.json').read_text())['llm_model']=='modello-locale-test'
            http('/api/settings','PUT',{**editable,'comfy_url':'file:///K:/AI'},400)
            http('/api/settings','PUT',{**editable,'llm_model':''},400)
            http('/api/settings','PUT',{**editable,'engine_root':'external'},422,raw=True)
            http('/api/settings','PUT',editable)
            assert b'Guida portable' in http('/guida',raw=True)
            print('PASS live service settings: validate, persist, apply immediately, preserve engine/provenance',flush=True)
            print("PASS suite-owned runtime and engines with unavailable legacy application paths", flush=True)
            http("/api/projects", "POST", {"title": "invalid", "aspect_ratio": "oops"}, 400)
            p = http("/api/projects", "POST", {"title": "Regressione sintetica", "aspect_ratio": "16:9"}, 201)
            pid = p["id"]
            prefix = "/api/projects/" + pid
            FakeServices.story_gate=threading.Event()
            queued_story=http(prefix+'/story/generate','POST',{'premise':'Test coda occupata'},202)
            try:
                http('/api/settings','PUT',editable,409)
            finally:
                FakeServices.story_gate.set()
            wait_job(queued_story)
            FakeServices.story_gate=None
            print('PASS settings guard: running/queued work cannot switch services mid-job',flush=True)
            assert p['creative_brief']==''
            brief='Documentario naturalistico. Palette terra, verde salvia; niente neon.'
            revised=http(prefix+'/creative-brief','PUT',{'creative_brief':brief,'expected_updated_at':p['updated_at']})
            assert revised['creative_brief']==brief
            http(prefix+'/creative-brief','PUT',{'creative_brief':'x'*2001},400)
            http(prefix+'/creative-brief','PUT',{'creative_brief':'stale','expected_updated_at':p['updated_at']},409)
            other=http('/api/projects','POST',{'title':'Altro stile'},201)
            assert other['creative_brief']=='' and http(prefix)['creative_brief']==brief
            for source in (video, image, audio):
                wait_job(http(prefix + "/assets", "POST", {"path": str(source)}, 202))
            p = http(prefix)
            v, i, a = (next(x for x in p["assets"] if x["kind"] == kind) for kind in ("video", "image", "audio"))
            assert http(prefix + "/assets/" + v["id"] + "/file", status=206, headers={"Range": "bytes=0-15"}, raw=True) == video.read_bytes()[:16]
            http(prefix + "/assets/" + v["id"] + "/file", method="HEAD", raw=True)
            http(prefix + "/timeline/clips", "POST", {"asset_id": v["id"], "trim_in_ms": 5000, "duration_ms": 1000}, 400)
            tracks = {track["kind"]: track["id"] for track in p["timeline"]["tracks"]}
            def clip(asset, track, start, trim, duration, volume=1):
                return {"id": str(uuid.uuid4()), "asset_id": asset["id"], "track_id": tracks[track], "start_ms": start, "trim_in_ms": trim, "duration_ms": duration, "label": "test", "volume": volume, "opacity": 1}
            timeline = p["timeline"]
            timeline["clips"] = [clip(v,"video",400,1000,600,0),clip(i,"overlay",700,0,300),clip(a,"audio",500,0,1000)]
            old_revision = p["updated_at"]
            p = http(prefix + "/timeline", "PUT", {"timeline": timeline, "expected_updated_at": old_revision})
            http(prefix + "/timeline", "PUT", {"timeline": timeline, "expected_updated_at": old_revision}, 409)
            export = wait_job(http(prefix + "/export", "POST", status=202))
            exported = Path(export["output_paths"][0])
            info = json.loads(command("ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", exported))
            assert abs(float(info["format"]["duration"]) - 1.5) < 0.1
            assert info["streams"][0]["width"] == 1920
            def pixel(second):
                return list(command("ffmpeg", "-v", "error", "-ss", second, "-i", exported, "-frames:v", "1", "-vf", "scale=1:1", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1")[:3])
            black, blue, green = pixel(0.1), pixel(0.6), pixel(0.85)
            assert max(black) < 10, black
            assert blue[2] > blue[0] + 80, blue
            assert green[1] > green[0] + 80 and green[1] > green[2] + 80, green
            pcm = array.array("h", command("ffmpeg", "-v", "error", "-i", exported, "-vn", "-ar", "8000", "-ac", "1", "-f", "s16le", "pipe:1"))
            rms = lambda values: math.sqrt(sum(x*x for x in values)/len(values))
            assert rms(pcm[:2400]) < 5
            assert rms(pcm[5000:6500]) > 100
            print("PASS compositor: trim, gaps, image overlay, independent audio, duration, range requests", flush=True)
            transcript = {"segments": [{"start": .25, "end": 1.25, "text": "Parlato di prova"}], "words": [{"start": .25, "end": .7, "word": "Parlato"},{"start": 1.05, "end": 1.2, "word": "prova"}]}
            t = http(prefix + "/transcripts/import", "POST", {"asset_id": v["id"], "transcript": transcript})
            assert b"00:00:00,250 --> 00:00:01,250" in http(prefix + "/transcripts/" + t["id"] + "/srt", raw=True)
            print("PASS transcript import and SRT timestamps", flush=True)
            # Seed only the synthetic source's cache, using the installed engine's own signature.
            work = data / "projects" / pid / "analysis" / v["id"]
            work.mkdir(parents=True)
            installed_video = data / "projects" / pid / "assets" / v["file_name"]
            seed = {**transcript, "language": "it", "duration": 2}
            command(PYTHON, ROOT / 'sviluppo/tests/whisper_cache_fixture.py', installed_video, work, json.dumps(seed))
            transcribed = wait_job(http(prefix + "/pipeline", "POST", {"asset_id": v["id"], "operation": "transcribe"}, 202))
            assert transcribed["result"]["words"][0]["word"] == "Parlato"
            print("PASS real ClipsAI bridge on cache-hit (no ASR/model inference)", flush=True)
            workflow_root = test / "workflows"
            workflow_root.mkdir()
            workflow = {"1": {"class_type": "CLIPTextEncode", "_meta":{"title":"Prompt (Input:prompt)"}, "inputs": {"text": "old"}}, "2": {"class_type": "SaveImage", "inputs": {"filename_prefix": "test"}}}
            (workflow_root / "untagged.json").write_text(json.dumps(workflow))
            wait_job(http("/api/workflows/scan", "POST", {"roots": [str(workflow_root)]}, 202))
            wf = http("/api/workflows")[0]
            assert http("/api/workflows/" + wf["id"])["1"]["class_type"] == "CLIPTextEncode"
            wait_job(http("/api/comfy/submit", "POST", {"workflow_path": wf["path"], "project_id": pid, "inputs": {"1.text": "test prompt"}}, 202))
            p = http(prefix)
            imported = [x for x in p["assets"] if x.get("source_path", "").startswith("ComfyUI job ") and x["source_path"].endswith(": synthetic.png")]
            assert len(imported) == 1
            assert (data / 'projects' / pid / 'assets' / imported[0]['file_name']).read_bytes() == FakeServices.png
            story_job = wait_job(http(prefix + "/story/generate", "POST", {"premise": "test"}, 202))
            assert story_job["result"]["scenes"][0]["title"] == "Scena di prova"
            assert http(prefix)["story"]["scenes"] == []  # model drafts must not overwrite editorial state
            print("PASS simulated ComfyUI lifecycle + output import and Ollama draft", flush=True)
            planned=wait_job(http(prefix+"/graphics/suggest","POST",{"auto_generate":True,"workflow_id":wf["id"]},202))
            generated=planned["result"]["generation_jobs"][0]
            wait_job(generated)
            p=http(prefix)
            graphics=[c for c in p["timeline"]["clips"] if c["label"]=="Grafica contestuale"]
            assert len(graphics)==1 and graphics[0]["start_ms"]==400 and graphics[0]["duration_ms"]==500
            print("PASS topic -> contextual graphic -> automatic ComfyUI queue -> source-linked overlay",flush=True)
            # Review is a real gate: output asset exists but no timeline change
            # until explicit approval, including idempotent repeated approval.
            count=len(p["timeline"]["clips"])
            review=wait_job(http("/api/comfy/submit","POST",{"workflow_path":wf["path"],"project_id":pid,"inputs":{"1.text":"graphic test prompt"},"placement":{"source_clip_id":p["timeline"]["clips"][0]["id"],"offset_ms":0,"duration_ms":100,"label":"Da approvare"},"review_before_insert":True},202))
            assert len(http(prefix)["timeline"]["clips"])==count
            assert review["result"]["_nl_suite"]["approved"] is False
            for _ in range(2):
                http(prefix+"/graphics/approve","POST",{"job_id":review["id"]})
            assert len(http(prefix)["timeline"]["clips"])==count+1
            print("PASS graphic review gate + explicit idempotent approval",flush=True)
            broken={"a":{"class_type":"EmptyLTXVLatentVideo","inputs":{"width":1280}},"b":{"class_type":"LTXVSeparateAVLatent","inputs":{"av_latent":["a",0]}},"c":{"class_type":"SaveVideo","inputs":{"samples":["b",0]}}}
            broken_path=workflow_root/"broken-ltx.json"
            broken_path.write_text(json.dumps(broken))
            before=hashlib.sha256(broken_path.read_bytes()).hexdigest()
            wait_job(http("/api/workflows/scan","POST",{"roots":[str(workflow_root)]},202))
            bw=next(w for w in http('/api/workflows') if w['name']=='broken-ltx')
            report=http('/api/workflows/diagnose','POST',{'workflow_id':bw['id']})
            assert not report['ready'] and report['repairs']
            prompts=FakeServices.prompt_count
            blocked=http('/api/comfy/submit','POST',{'workflow_path':bw['path'],'project_id':pid},202)
            for _ in range(100):
                blocked=http('/api/jobs/'+blocked['id'])
                if blocked['status']=='failed': break
                time.sleep(.05)
            assert blocked['status']=='failed' and FakeServices.prompt_count==prompts
            repaired=http('/api/workflows/repair','POST',{'workflow_id':bw['id'],'selected_repairs':[r['id'] for r in report['repairs']]})
            assert repaired['report']['ready'] and repaired['original_modified'] is False
            assert hashlib.sha256(broken_path.read_bytes()).hexdigest()==before
            repaired_graph=http('/api/workflows/'+repaired['workflow']['id'])
            assert 'b' not in repaired_graph and repaired_graph['c']['inputs']['samples']==['a',0]
            print('PASS live-schema doctor + preflight blocks submission + non-destructive repair copy',flush=True)
            http(prefix+'/creative-brief','PUT',{'creative_brief':'no-graphics-test'})
            empty=wait_job(http(prefix+'/graphics/suggest','POST',{'auto_generate':True,'workflow_id':wf['id']},202))
            assert empty['result']['graphics']==[] and not empty['result'].get('generation_jobs')
            print('PASS no forced graphics when planner recommends preserving the performance',flush=True)
            # ZP: use the installed parser to preserve approval and interior KEEP cuts.
            zp = test / "candidates.json"
            zp.write_text(json.dumps({"candidates": [{"id": "candidate-test", "title": "keep", "start": 0, "end": 2, "approved": True, "excluded_ranges": [{"id":"cut_001", "start": .5, "end": 1}]}]}))
            migration_p = http("/api/projects", "POST", {"title": "ZP migration"}, 201)
            mid = migration_p["id"]
            wait_job(http("/api/projects/" + mid + "/assets", "POST", {"path": str(video)}, 202))
            migration_p = http("/api/projects/" + mid)
            wait_job(http("/api/projects/" + mid + "/legacy-import", "POST", {"system":"zp_shorts", "path":str(zp), "asset_id":migration_p["assets"][0]["id"]}, 202))
            imported = http("/api/projects/" + mid)
            assert [x["duration_ms"] for x in imported["timeline"]["clips"]] == [500,1000]
            print("PASS installed ZP parser: approval + interior exclusions", flush=True)
            regista = test / "regista"
            regista.mkdir()
            (regista / "config.json").write_text(json.dumps({"topic": "test"}))
            (regista / "timeline_engine.json").write_text(json.dumps({"tracks":[{"id":"video","type":"video"}],"clips":[{"id":"clip","track_id":"video","start":.25,"end":1.25,"duration":1,"trim_in":.5,"asset_path":str(video)}]}))
            mp = http("/api/projects", "POST", {"title":"Regista migration"},201)
            wait_job(http("/api/projects/" + mp["id"] + "/legacy-import", "POST", {"system":"regista","path":str(regista)},202))
            imported = http("/api/projects/" + mp["id"])
            assert imported["timeline"]["clips"][0]["start_ms"] == 250
            assert imported["timeline"]["clips"][0]["trim_in_ms"] == 500
            print("PASS Regista import: timing, trim and media ownership", flush=True)
            db = test / "calliope.db"
            with sqlite3.connect(db) as c:
                c.executescript("CREATE TABLE projects(id INTEGER,title TEXT,idea TEXT); CREATE TABLE story_beats(project_id INTEGER); CREATE TABLE characters(project_id INTEGER,name TEXT,appearance TEXT); CREATE TABLE locations(project_id INTEGER,name TEXT,description TEXT); CREATE TABLE items(project_id INTEGER); CREATE TABLE scenes(project_id INTEGER,id INTEGER,order_index INTEGER); CREATE TABLE clips(project_id INTEGER,scene_id INTEGER,order_index INTEGER,clip_path TEXT,duration_sec INTEGER,description TEXT);")
                c.execute("INSERT INTO projects VALUES(1,'test','idea')")
                c.execute("INSERT INTO scenes VALUES(1,10,0)")
                c.execute("INSERT INTO clips VALUES(1,10,0,?,1,'shot')",(str(video),))
            c.close()
            db_hash = hashlib.sha256(db.read_bytes()).hexdigest()
            mp = http("/api/projects", "POST", {"title":"Calliope migration"},201)
            wait_job(http("/api/projects/" + mp["id"] + "/legacy-import", "POST", {"system":"calliope","path":str(db),"source_project_id":1},202))
            assert len(http("/api/projects/" + mp["id"])["timeline"]["clips"]) == 1
            assert hashlib.sha256(db.read_bytes()).hexdigest() == db_hash
            print("PASS Calliope read-only SQLite import", flush=True)
            assert hashlib.sha256(video.read_bytes()).hexdigest() == master_hash
            persisted = json.loads((data / "projects" / pid / "project.json").read_text())
            assert len(persisted["timeline"]["clips"]) == count+1
            print("PASS source immutability and project persistence", flush=True)
        finally:
            server.terminate()
            server.wait(timeout=20)
            log.close()
            services.shutdown()


if __name__ == "__main__":
    main()
