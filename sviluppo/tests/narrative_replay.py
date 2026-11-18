"""Replay the ingestion of a really produced ComfyUI image, without inference.

The previous real run produced `NL_Suite_Library_Test_00001_.png` (kept in the
interrupted artifact) but the harness died before the suite imported it. This
test serves those exact bytes from a replay ComfyUI and exercises the true path
`submit_comfy -> /history -> /view -> adopt_comfy_download -> reference.asset_id`.

No model, node, weight or GPU is used. It is not a substitute for a real
end-to-end run: it proves the suite's ingestion and library linkage steps.
The fake `/history` answers 500 once on purpose, so the bounded retry is also
covered by the same run.

Usage: runtime/python/python.exe sviluppo/tests/narrative_replay.py [exe]
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from objects_http import FakeBase, free_port, serve

ROOT = Path(__file__).resolve().parents[2]
REAL_IMAGE = ROOT / "sviluppo/tests/artifacts/live-library-031531bc/comfy-output-observed.png"
FILENAME = "NL_Suite_Library_Test_00001_.png"


class ReplayComfy(FakeBase):
    png = b""
    prompts = []
    history_calls = 0
    injected_failure = threading.Event()

    def do_GET(self):
        def handle():
            if self.path == "/object_info":
                return self.reply({
                    "CLIPTextEncode": {"input": {"required": {"text": ["STRING"]}}, "output": ["CONDITIONING"]},
                    "EmptyFlux2LatentImage": {"input": {"required": {
                        "width": ["INT", {"default": 1024, "min": 16, "max": 16384, "step": 16}],
                        "height": ["INT", {"default": 1024, "min": 16, "max": 16384, "step": 16}],
                    }}, "output": ["LATENT"]},
                    "VAEDecode": {"input": {"required": {"samples": ["LATENT"]}}, "output": ["IMAGE"]},
                    "SaveImage": {"input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING"]}},
                                  "output_node": True, "output": []},
                })
            if self.path == "/system_stats":
                return self.reply({"system": {"test_double": True}, "devices": []})
            if self.path.startswith("/history/"):
                type(self).history_calls += 1
                if not type(self).injected_failure.is_set():
                    type(self).injected_failure.set()
                    # One transient failure: the suite must retry, not abandon work.
                    return self.reply({"error": "transient replay failure"}, 500)
                prompt_id = self.path.rsplit("/", 1)[1]
                return self.reply({prompt_id: {"status": {"completed": True, "status_str": "success"},
                    "outputs": {"save": {"images": [{"filename": FILENAME, "type": "output", "subfolder": ""}]}}}})
            if self.path.startswith("/view?"):
                query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
                assert query["filename"][0] == FILENAME, query
                return self.reply(self.png, content_type="image/png")
            return self.reply({"error": "unsupported fake endpoint"}, 404)
        self.checked(handle)

    def do_POST(self):
        def handle():
            assert self.path == "/prompt", self.path
            value = self.body()
            type(self).prompts.append(value["prompt"])
            return self.reply({"prompt_id": "replay-" + str(len(type(self).prompts))})
        self.checked(handle)


def request(url, method="GET", body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method,
        headers={"Content-Type": "application/json"} if data else {})
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.load(response)


def main():
    exe = Path(sys.argv[1]).resolve(strict=True) if len(sys.argv) > 1 else ROOT / "sviluppo/target/release/nl-video-suite.exe"
    assert REAL_IMAGE.is_file(), "Real ComfyUI output missing: " + str(REAL_IMAGE)
    original = REAL_IMAGE.read_bytes()
    original_hash = hashlib.sha256(original).hexdigest()
    ReplayComfy.png = original
    comfy, comfy_url = serve(ReplayComfy)
    with tempfile.TemporaryDirectory(prefix="nl-suite-replay-") as folder:
        test = Path(folder).resolve(strict=True)
        data, workflows = test / "data", test / "workflows"
        data.mkdir(); workflows.mkdir()
        (data / "settings.json").write_text(json.dumps({
            "comfy_url": comfy_url, "ollama_url": comfy_url, "legacy_sources": {}}), encoding="utf-8")
        (workflows / "replay-image.json").write_text(json.dumps({
            "positive": {"class_type": "CLIPTextEncode", "_meta": {"title": "Positive (Input:prompt)"},
                         "inputs": {"text": "old"}},
            "latent": {"class_type": "EmptyFlux2LatentImage", "inputs": {"width": 1024, "height": 576}},
            "decode": {"class_type": "VAEDecode", "inputs": {"samples": ["latent", 0]}},
            "save": {"class_type": "SaveImage", "inputs": {"images": ["decode", 0], "filename_prefix": "replay"}},
        }, indent=2), encoding="utf-8")
        port = free_port(); base = "http://127.0.0.1:" + str(port)
        log = (test / "server.log").open("w", encoding="utf-8")
        process = subprocess.Popen([str(exe), "--port", str(port), "--data-dir", str(data)],
            cwd=ROOT, stdout=log, stderr=log)
        try:
            for _ in range(100):
                try:
                    request(base + "/api/projects")
                    break
                except OSError:
                    time.sleep(.1)
            def api(path, method="GET", body=None):
                return request(base + path, method, body)
            scan = api("/api/workflows/scan", "POST", {"roots": [str(workflows)]})
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                scanned = api("/api/jobs/" + scan["id"])
                if scanned["status"] in ("succeeded", "failed"):
                    break
                time.sleep(.25)
            assert scanned["status"] == "succeeded", scanned
            workflow = next(w for w in api("/api/workflows") if w["name"] == "replay-image")
            project = api("/api/projects", "POST", {"title": "Replay ingestion", "aspect_ratio": "16:9"})
            prefix = "/api/projects/" + project["id"]
            production = api(prefix + "/narrative")
            production["references"] = [{
                "id": str(uuid.uuid4()), "kind": "location", "name": "Sfondo di replay",
                "notes": "", "prompt": "Provided by the previous real ComfyUI run.",
                "workflow": {"workflow_id": workflow["id"], "text_input": "positive.text",
                             "inputs": {}, "asset_inputs": {}},
            }]
            production = api(prefix + "/narrative", "PUT", {"production": production})
            job = api(prefix + "/narrative/references/generate", "POST",
                      {"reference_id": production["references"][0]["id"]})
            deadline = time.monotonic() + 120
            while time.monotonic() < deadline:
                done = api("/api/jobs/" + job["id"])
                if done["status"] in ("succeeded", "failed"):
                    break
                time.sleep(.25)
            assert done["status"] == "succeeded", done
            assert ReplayComfy.history_calls >= 2, "The transient /history failure was not retried"
            assert ReplayComfy.prompts, "No prompt reached the replay server"
            graph = ReplayComfy.prompts[0]
            # 1.5x of the 1920x1080 delivery frame, aligned to 16.
            assert graph["latent"]["inputs"]["width"] == 2880, graph["latent"]
            assert graph["latent"]["inputs"]["height"] == 1616, graph["latent"]
            assert graph["positive"]["inputs"]["text"] == "Provided by the previous real ComfyUI run."
            production = api(prefix + "/narrative")
            reference = production["references"][0]
            assert reference["asset_id"] and not reference["approved"], reference
            project = api(prefix)
            asset = next(a for a in project["assets"] if a["id"] == reference["asset_id"])
            assert asset["kind"] == "image", asset
            copied = (data / "projects" / project["id"] / "assets" / asset["file_name"]).read_bytes()
            assert hashlib.sha256(copied).hexdigest() == original_hash, "Replayed bytes were altered"
            assert not project["timeline"]["clips"] and not production["scenes"]
            leftover = [p.name for p in (data / "generated" / reference["job_id"]).iterdir() if p.suffix != ".json"]
            assert not leftover, "Downloaded media left behind in the generated folder: " + str(leftover)
            print("PASS replay ingestion: real ComfyUI bytes -> project asset -> review, retry exercised", flush=True)
        finally:
            process.terminate(); process.wait(timeout=20); log.close()
            comfy.shutdown(); comfy.server_close()
            assert not ReplayComfy.failures, ReplayComfy.failures


if __name__ == "__main__":
    main()
