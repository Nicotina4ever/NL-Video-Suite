"""HTTP regression of timeline graphic objects with explicitly FAKE AI services.

Runs the compiled Rust server against disposable synthetic media/project data.
The word-timed transcript is imported as a fixture: no ASR model is executed.
Ollama replies and ComfyUI image bytes are simulated, not quality benchmarks.

Usage: runtime/python/python.exe sviluppo/tests/objects_http.py [path/to/exe]
"""
import contextlib
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import wave
import zlib


ROOT = Path(__file__).resolve().parents[2]
EXPECTED_VERSION = "0.7.1"
DESCRIPTION = "Immagine illustrativa di un microfono dinamico, con spazio negativo per il montaggio."
PROMPT = (
    "Subject: one generic dynamic microphone, an illustrative object rather than documentary evidence. "
    "Composition: close-up on the left third, empty right half for editorial graphics. "
    "Lighting: broad soft key light and restrained edge light. "
    "Visual treatment: tactile matte metal, clean professional editorial photography. "
    "Palette: charcoal, warm ivory and muted terracotta. "
    "Camera: eye-level close-up, natural perspective and moderate depth of field. "
    "Constraints: no text, logos, trademarks, people, invented statistics or additional objects."
)


def png_bytes():
    """A deliberately synthetic solid-colour fixture, made without AI or assets."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)
    row = b"\x00" + b"\x21\x91\x73\xff" * 96
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 96, 64, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(row * 64)) + chunk(b"IEND", b""))


def free_port():
    with socket.socket() as connection:
        connection.bind(("127.0.0.1", 0))
        return connection.getsockname()[1]


class FakeBase(BaseHTTPRequestHandler):
    failures = []

    def log_message(self, *_):
        pass

    def reply(self, value, status=200, content_type="application/json"):
        body = json.dumps(value).encode() if content_type == "application/json" else value
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def body(self):
        return json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))

    def checked(self, action):
        try:
            return action()
        except Exception as error:
            type(self).failures.append(repr(error))
            return self.reply({"error": repr(error)}, 500)


class FakeOllama(FakeBase):
    started = threading.Event()
    release = threading.Event()
    requests = []

    def do_GET(self):
        if self.path == "/api/tags":
            return self.reply({"models": [{"name": "fake-graphics-model"}]})
        return self.reply({"error": "not a simulated endpoint"}, 404)

    def do_POST(self):
        def handle():
            assert self.path == "/api/chat", self.path
            value = self.body()
            type(self).requests.append(value)
            assert value["model"] == "fake-graphics-model"
            assert value["keep_alive"] == 0
            assert value["options"]["num_gpu"] == 0
            assert value["format"]["required"] == ["description", "prompt"]
            user_text = value["messages"][-1]["content"].split("Contesto circostante")[0]
            assert "Microfono dinamico" in user_text
            assert "Materiali traslucidi e geometrie asimmetriche" in user_text
            assert "FUORI_PRIMA" not in user_text and "FUORI_DOPO" not in user_text
            assert "PARAGRAFO_INTERO" not in user_text
            type(self).started.set()
            assert type(self).release.wait(20), "test did not release simulated LLM"
            return self.reply({"model": "fake-graphics-model", "done_reason": "stop", "message": {
                "role": "assistant", "content": json.dumps({"description": DESCRIPTION, "prompt": PROMPT})}})
        self.checked(handle)


class FakeComfy(FakeBase):
    prompted = threading.Event()
    release = threading.Event()
    prompts = []
    png = png_bytes()

    def do_GET(self):
        def handle():
            if self.path == "/system_stats":
                return self.reply({"system": {"test_double": True}, "devices": []})
            if self.path == "/object_info":
                return self.reply({
                    "CLIPTextEncode": {"input": {"required": {"text": ["STRING"]}}, "output": ["CONDITIONING"]},
                    "FixtureModelLoader": {"input": {"required": {"model_name": [["keep-fixture-model.safetensors"]]}}, "output": ["MODEL"]},
                    "SaveImage": {"input": {"required": {"filename_prefix": ["STRING"]}}, "output_node": True, "output": []},
                })
            if self.path.startswith("/history/"):
                if not type(self).release.is_set():
                    return self.reply({})
                return self.reply({"synthetic-object-prompt": {"status": {"completed": True, "status_str": "success"},
                    "outputs": {"save": {"images": [{"filename": "synthetic-object.png", "type": "output", "subfolder": ""}]}}}})
            if self.path.startswith("/view?"):
                return self.reply(type(self).png, content_type="image/png")
            return self.reply({"error": "not a simulated endpoint"}, 404)
        self.checked(handle)

    def do_POST(self):
        def handle():
            assert self.path == "/prompt", self.path
            value = self.body()
            type(self).prompts.append(value)
            graph = value["prompt"]
            assert graph["positive"]["inputs"]["text"] == PROMPT
            assert graph["negative"]["inputs"]["text"] == "must remain untouched"
            assert graph["model"]["inputs"]["model_name"] == "keep-fixture-model.safetensors"
            type(self).prompted.set()
            return self.reply({"prompt_id": "synthetic-object-prompt"})
        self.checked(handle)


def serve(handler):
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, "http://127.0.0.1:" + str(server.server_port)


def main():
    exe = Path(sys.argv[1]).resolve(strict=True) if len(sys.argv) > 1 else ROOT / "sviluppo/target/release/nl-video-suite.exe"
    version = subprocess.check_output([str(exe), "--version"], cwd=str(ROOT), text=True).strip()
    assert version == EXPECTED_VERSION, "Use the current release build, got " + version
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryDirectory(prefix="nl-suite-objects-http-") as directory:
        test = Path(directory).resolve(strict=True)
        # Validate the exact disposable scope before creating any test files.
        assert test.parent == Path(tempfile.gettempdir()).resolve(strict=True)
        assert test.name.startswith("nl-suite-objects-http-")

        def owned(*parts):
            path = test.joinpath(*parts).resolve()
            assert path != test and test in path.parents, (test, path)
            return path

        data, workflows = owned("data"), owned("workflows")
        data.mkdir()
        workflows.mkdir()
        source = owned("synthetic-voice.wav")
        with wave.open(str(source), "wb") as audio:
            audio.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
            samples = [int(6000 * math.sin(2 * math.pi * 440 * i / 16000)) for i in range(96000)]
            audio.writeframes(struct.pack("<" + "h" * len(samples), *samples))
        source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        workflow = {
            "positive": {"class_type": "CLIPTextEncode", "_meta": {"title": "Positive (Input:prompt)"}, "inputs": {"text": "old"}},
            "negative": {"class_type": "CLIPTextEncode", "inputs": {"text": "must remain untouched"}},
            "model": {"class_type": "FixtureModelLoader", "inputs": {"model_name": "keep-fixture-model.safetensors"}},
            "save": {"class_type": "SaveImage", "inputs": {"filename_prefix": "fixture"}},
        }
        owned("workflows", "synthetic-object.json").write_text(json.dumps(workflow), encoding="utf-8")
        llm, llm_url = serve(FakeOllama)
        comfy, comfy_url = serve(FakeComfy)
        owned("data", "settings.json").write_text(json.dumps({
            "comfy_url": comfy_url, "ollama_url": llm_url, "llm_provider": "ollama",
            "llm_model": "fake-default-model", "role_llm_models": {"graphics": "fake-graphics-model"},
            "ollama_execution": "cpu", "legacy_sources": {}, "workflow_roots": [str(workflows)],
        }), encoding="utf-8")
        port = free_port()
        base = "http://127.0.0.1:" + str(port)
        log_path = owned("server.log")
        log = log_path.open("w", encoding="utf-8")
        env = {**os.environ, "NL_SUITE_ROOT": str(ROOT)}
        server = subprocess.Popen([str(exe), "--port", str(port), "--data-dir", str(data)],
                                  cwd=str(test), env=env, stdout=log, stderr=log)

        def http(path, method="GET", body=None, status=200, raw=False):
            payload = json.dumps(body).encode() if body is not None else None
            req = urllib.request.Request(base + path, data=payload, method=method,
                                         headers={"Content-Type": "application/json"} if body is not None else {})
            try:
                response = opener.open(req, timeout=15)
            except urllib.error.HTTPError as error:
                response = error
            content = response.read()
            assert response.status == status, (method, path, response.status, content.decode(errors="replace"))
            return content if raw else json.loads(content) if content else None

        def wait_job(job):
            deadline = time.monotonic() + 35
            while time.monotonic() < deadline:
                value = http("/api/jobs/" + job["id"])
                if value["status"] in ("succeeded", "failed"):
                    assert value["status"] == "succeeded", value
                    return value
                time.sleep(.08)
            raise AssertionError("job timeout: " + job["id"])

        try:
            deadline = time.monotonic() + 15
            while True:
                try:
                    http("/api/projects")
                    break
                except (OSError, urllib.error.URLError):
                    assert server.poll() is None, log_path.read_text(encoding="utf-8")
                    assert time.monotonic() < deadline, "server start timeout"
                    time.sleep(.05)
            p = http("/api/projects", "POST", {"title": "Synthetic object HTTP test"}, 201)
            prefix = "/api/projects/" + p["id"]
            wait_job(http(prefix + "/assets", "POST", {"path": str(source)}, 202))
            p = http(prefix)
            audio_asset = p["assets"][0]
            source_clip = http(prefix + "/timeline/clips", "POST", {
                "asset_id": audio_asset["id"], "start_ms": 10000, "trim_in_ms": 1000, "duration_ms": 3000}, 201)
            p = http(prefix + "/graphic-objects", "POST", {
                "title": "Microfono illustrativo", "start_ms": 11000, "duration_ms": 1000, "source_clip_id": source_clip["id"]})
            item = p["graphic_objects"][0]
            endpoint = prefix + "/graphic-objects/" + item["id"]
            http(endpoint + "/describe", "POST", {}, 400)
            assert not FakeOllama.requests
            http(prefix + "/transcripts/import", "POST", {"asset_id": audio_asset["id"], "transcript": {
                "language": "it", "segments": [{"start": 0, "end": 6, "text": "PARAGRAFO_INTERO"}],
                "words": [{"start": 0, "end": .4, "word": "FUORI_PRIMA"},
                          {"start": 2, "end": 2.4, "word": "Microfono"},
                          {"start": 2.45, "end": 2.9, "word": "dinamico"},
                          {"start": 3.8, "end": 4.5, "word": "FUORI_DOPO"}]}})
            http(endpoint, "PUT", {"description": "Materiali traslucidi e geometrie asimmetriche"})
            described = http(endpoint + "/describe", "POST", {}, 202)
            assert FakeOllama.started.wait(10), "simulated LLM request missing"
            http(endpoint, "PUT", {"description": "must not overwrite in-flight"}, 400)
            http(endpoint, "DELETE", status=400)
            http(endpoint + "/describe", "POST", {}, 400)
            FakeOllama.release.set()
            finished_description = wait_job(described)
            p = http(prefix)
            assert p["graphic_objects"][0]["description"] == DESCRIPTION
            assert p["graphic_objects"][0]["prompt"] == PROMPT
            assert "Microfono dinamico" in finished_description["result"]["source_text"]
            assert len(p["timeline"]["clips"]) == 1
            workflow_id = next(w["id"] for w in http("/api/workflows") if w["name"] == "synthetic-object")
            generated = http(endpoint + "/generate", "POST", {"workflow_id": workflow_id}, 202)
            assert FakeComfy.prompted.wait(10), "simulated Comfy prompt missing"
            http(endpoint + "/generate", "POST", {"workflow_id": workflow_id}, 400)
            http(endpoint + "/approve", "POST", {}, 400)
            http(endpoint, "DELETE", status=400)
            FakeComfy.release.set()
            finished_generation = wait_job(generated)
            p = http(prefix)
            reviewed = p["graphic_objects"][0]
            assert reviewed["status"] == "review"
            assert len(p["timeline"]["clips"]) == 1, "generation inserted an overlay without approval"
            generated_asset_id = reviewed["asset_id"]
            assert generated_asset_id in finished_generation["result"]["_nl_suite"]["asset_ids"]
            assert finished_generation["result"]["_nl_suite"]["graphic_object_id"] == item["id"]
            http(endpoint + "/approve", "POST", {"asset_id": audio_asset["id"]}, 400)
            for _ in range(2):
                p = http(endpoint + "/approve", "POST", {})
                assert len(p["timeline"]["clips"]) == 2, "duplicate approval appended another overlay"
                assert p["graphic_objects"][0]["status"] == "approved"
            overlay = next(c for c in p["timeline"]["clips"] if c["asset_id"] == generated_asset_id)
            assert (overlay["start_ms"], overlay["duration_ms"]) == (11000, 1000)
            assert http(prefix + "/assets/" + generated_asset_id + "/file", raw=True) == FakeComfy.png
            timeline = p["timeline"]
            next(t for t in timeline["tracks"] if t["id"] == overlay["track_id"])["locked"] = True
            p = http(prefix + "/timeline", "PUT", {"timeline": timeline, "expected_updated_at": p["updated_at"]})
            http(endpoint, "DELETE", status=400)
            next(t for t in p["timeline"]["tracks"] if t["id"] == overlay["track_id"])["locked"] = False
            p = http(prefix + "/timeline", "PUT", {"timeline": p["timeline"], "expected_updated_at": p["updated_at"]})
            # Every file tested below must be inside this unique disposable root.
            media = [(owned("data", "projects", p["id"], "assets", a["file_name"]), a["id"]) for a in p["assets"]]
            hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path, _ in media}
            history_ids = {j["id"] for j in http("/api/jobs")}
            p = http(endpoint, "DELETE")
            assert not p["graphic_objects"] and len(p["timeline"]["clips"]) == 1
            assert p["timeline"]["clips"][0]["id"] == source_clip["id"]
            assert len(p["assets"]) == 2
            assert {j["id"] for j in http("/api/jobs")} == history_ids
            for path, _ in media:
                assert hashlib.sha256(path.read_bytes()).hexdigest() == hashes[str(path)]
            assert hashlib.sha256(source.read_bytes()).hexdigest() == source_hash
            assert len(FakeOllama.requests) == 1 and len(FakeComfy.prompts) == 1
            assert not FakeBase.failures and not FakeOllama.failures and not FakeComfy.failures
            print(json.dumps({"passed": True, "version": version, "transport": "real HTTP to compiled Rust server",
                "ai_services": "explicit fake Ollama + fake ComfyUI", "actual_model_generation": False,
                "actual_asr": False, "transcript": "synthetic word-timed fixture imported over HTTP",
                "verified": ["trim-mapped speech only", "role model + per-request CPU/unload policy", "detailed editable prompt",
                    "in-flight guards", "model and negative prompt unchanged", "no overlay before review",
                    "project/job asset ownership", "idempotent approval", "locked-track guard", "delete preserves media and job history"]}, indent=2))
        except Exception:
            log.flush()
            print(log_path.read_text(encoding="utf-8", errors="replace"), file=sys.stderr)
            raise
        finally:
            FakeOllama.release.set()
            FakeComfy.release.set()
            server.terminate()
            with contextlib.suppress(subprocess.TimeoutExpired):
                server.wait(timeout=8)
            if server.poll() is None:
                server.kill()
                server.wait(timeout=5)
            log.close()
            for service in (llm, comfy):
                service.shutdown()
                service.server_close()


if __name__ == "__main__":
    main()
