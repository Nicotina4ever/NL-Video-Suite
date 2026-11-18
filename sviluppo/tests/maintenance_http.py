"""Real HTTP regression for maintenance and launcher configuration (suite 0.5+).

Only synthetic files under a validated UUID fixture inside this installation are
deleted. AI services are fake local HTTP endpoints; no external service, model,
Python environment or original project is launched, stopped or altered.
"""
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid
import wave


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "sviluppo/.maintenance-test-fixtures"
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def free_port():
    with socket.socket() as connection:
        connection.bind(("127.0.0.1", 0))
        return connection.getsockname()[1]


class FakeServices(BaseHTTPRequestHandler):
    requests = []
    gate = threading.Event()
    entered_chat = threading.Event()
    malformed = False

    def log_message(self, *_):
        pass

    def reply(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        type(self).requests.append(("GET", self.path))
        if type(self).malformed:
            return self.reply({"not_the_expected_service": True})
        if self.path == "/system_stats":
            return self.reply({"devices": [], "system": {"os": "synthetic"}})
        if self.path == "/api/tags":
            return self.reply({"models": [{"name": "synthetic:latest"}]})
        if self.path == "/v1/models":
            return self.reply({"object": "list", "data": [{"id": "synthetic.gguf"}]})
        self.reply({"error": "not found"}, 404)

    def do_POST(self):
        type(self).requests.append(("POST", self.path))
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        if self.path != "/api/chat":
            return self.reply({"error": "no generation service"}, 404)
        assert body.get("model") == "synthetic:latest", body
        type(self).entered_chat.set()
        if not type(self).gate.wait(15):
            return self.reply({"error": "synthetic gate timeout"}, 500)
        self.reply({"message": {"content": json.dumps({"premise": "test", "characters": [], "locations": [], "scenes": [{"title": "Synthetic", "description": "Fixture only."}]})}})


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, contents):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(contents)
    return path


def remove_own_fixture(path):
    """Never remove the shared fixture parent, suite root or an unresolved path."""
    parent = FIXTURES.resolve()
    target = path.resolve(strict=True)
    assert uuid.UUID(target.name).hex == target.name, target
    assert target.parent == parent and target.is_relative_to(ROOT.resolve()), target
    assert not path.is_symlink(), path
    if os.name == "nt":
        assert not (path.lstat().st_file_attributes & 0x400), path
    shutil.rmtree(target)


def main():
    exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "app/nl-video-suite.exe"
    version = subprocess.run([str(exe), "--version"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
    assert tuple(map(int,version.split('.'))) >= (0,5,0), "Expected suite 0.5 or newer, found " + version
    FIXTURES.mkdir(exist_ok=True)
    test = FIXTURES / uuid.uuid4().hex
    test.mkdir()
    data = test / "data"
    data.mkdir()
    fake = ThreadingHTTPServer(("127.0.0.1", 0), FakeServices)
    thread = threading.Thread(target=fake.serve_forever, daemon=True)
    thread.start()
    service_url = "http://127.0.0.1:" + str(fake.server_port)
    settings = {"comfy_url": service_url, "ollama_url": service_url,
                "llm_model": "synthetic:latest", "llm_provider": "ollama",
                "openai_base_url": service_url + "/v1", "legacy_sources": {},
                "workflow_roots": []}
    write(data / "settings.json", json.dumps(settings).encode())
    port = free_port()
    base = "http://127.0.0.1:" + str(port)
    log_path = test / "server.log"
    log = log_path.open("w", encoding="utf-8")
    environment = os.environ.copy()
    environment["NL_SUITE_ROOT"] = str(ROOT)
    environment["NO_PROXY"] = "127.0.0.1,localhost"
    process = subprocess.Popen([str(exe), "--port", str(port), "--data-dir", str(data)],
                               cwd=ROOT, env=environment, stdout=log, stderr=log)
    checks = []

    def api(path, method="GET", body=None, status=200):
        request = urllib.request.Request(base + path,
            data=json.dumps(body).encode() if body is not None else None,
            headers={"Content-Type": "application/json"} if body is not None else {}, method=method)
        try:
            response = OPENER.open(request, timeout=25)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            content = response.read()
            assert response.status == status, (path, response.status, content.decode(errors="replace"))
            return json.loads(content) if content else None

    def wait_job(job):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            current = api("/api/jobs/" + job["id"])
            if current["status"] in ("succeeded", "failed"):
                assert current["status"] == "succeeded", current
                return
            time.sleep(0.05)
        raise AssertionError("Synthetic job did not finish")

    def cleanup(plan, confirmation="ELIMINA CACHE", status=200):
        return api("/api/storage/cleanup", "POST", {"plan_id": plan["plan_id"], "confirmation": confirmation}, status)

    def save_settings():
        editable = {key: settings[key] for key in ("comfy_url", "ollama_url", "llm_model", "llm_provider", "openai_base_url")}
        return api("/api/settings", "PUT", editable)

    try:
        for _ in range(100):
            try:
                api("/api/projects")
                break
            except (OSError, urllib.error.URLError):
                assert process.poll() is None, log_path.read_text(encoding="utf-8")
                time.sleep(0.1)
        else:
            raise AssertionError("Suite startup timeout")
        assert api("/api/health")["version"] == version

        project = api("/api/projects", "POST", {"title": "Synthetic maintenance fixture"}, 201)
        project_dir = data / "projects" / project["id"]
        source_audio = test / "synthetic-silence.wav"
        with wave.open(str(source_audio), "wb") as audio:
            audio.setnchannels(1)
            audio.setsampwidth(2)
            audio.setframerate(16000)
            audio.writeframes(b"\x00\x00" * 16000)
        wait_job(api("/api/projects/" + project["id"] + "/assets", "POST", {"path": str(source_audio)}, 202))
        asset = api("/api/projects/" + project["id"])["assets"][0]
        saved_transcript = api("/api/projects/" + project["id"] + "/transcripts/import", "POST", {
            "asset_id": asset["id"],
            "transcript": {"language": "it", "segments": [{"start": 0, "end": 1, "text": "Trascrizione sintetica da conservare."}], "words": []}})
        protected = [
            write(project_dir / "assets/do-not-delete.png", b"real asset fixture"),
            write(project_dir / "exports/do-not-delete.mp4", b"export fixture"),
            write(data / "models/do-not-delete.bin", b"model fixture"),
            write(data / "ZMateriale/do-not-delete.jpg", b"personal fixture"),
            project_dir / "project.json",
            project_dir / "assets" / asset["file_name"],
        ]
        protected_hashes = {path: digest(path) for path in protected}
        thumbnail = write(data / "previews" / project["id"] / "thumbnail.jpg", b"cached thumbnail")
        changed = write(project_dir / ".cache/previews/wave.png", b"original waveform")
        held = write(data / "previews/open.jpg", b"open cache")
        old_log = write(data / "logs/old.log", b"old synthetic log")
        write(data / "logs/recent.log", b"recent synthetic log")
        stale = time.time() - 4 * 24 * 60 * 60
        os.utime(old_log, (stale, stale))
        analysis = write(project_dir / "analysis" / asset["id"] / "clip_trascrizione.json", b'{"synthetic":true}')
        plan = api("/api/storage/preview", "POST", {"include_analysis_cache": False})
        paths = {str(Path(item["relative_path"])) for item in plan["files"]}
        assert str(thumbnail.relative_to(data)) in paths
        assert str(old_log.relative_to(data)) in paths
        assert str(analysis.relative_to(data)) not in paths
        assert plan["total_bytes"] == sum(item["bytes"] for item in plan["files"])
        cleanup(plan, confirmation="", status=400)
        assert thumbnail.exists() and old_log.exists()

        # The queue gate is a fake HTTP response, not an LLM or GPU workload.
        FakeServices.gate.clear()
        job = api("/api/projects/" + project["id"] + "/story/generate", "POST", {"premise": "Synthetic lock check"}, 202)
        assert FakeServices.entered_chat.wait(5), "Fake chat not reached"
        cleanup(plan, status=400)
        assert thumbnail.exists()
        FakeServices.gate.set()
        wait_job(job)
        # Story generation legitimately updates project.json; take a fresh baseline.
        protected_hashes[project_dir / "project.json"] = digest(project_dir / "project.json")
        checks.append("cleanup blocked while suite queue/worker is active")

        changed.write_bytes(b"different size after preview")
        new_cache = write(data / "previews/new-since-plan.jpg", b"new file not in plan")
        with held.open("rb"):
            result = cleanup(plan)
        assert not thumbnail.exists() and not old_log.exists()
        assert changed.exists() and new_cache.exists()
        expected_skips = 2 if os.name == "nt" else 1
        assert result["skipped_files"] == expected_skips, result
        if os.name == "nt":
            assert held.exists()
        assert result["deleted_bytes"] == sum(entry["bytes"] for entry in result["entries"] if entry["status"] == "deleted")
        assert (data / result["audit_path"]).is_file()
        cleanup(plan, status=400)
        assert all(digest(path) == value for path, value in protected_hashes.items())
        assert analysis.exists()
        checks.append("confirmed synthetic cleanup, stale/open/new-file skips, single-use audit and protected data")

        opt_in = api("/api/storage/preview", "POST", {"include_analysis_cache": True})
        assert any(Path(item["relative_path"]) == analysis.relative_to(data) for item in opt_in["files"])
        cleanup(opt_in)
        assert not analysis.exists()
        assert all(digest(path) == value for path, value in protected_hashes.items())
        assert api("/api/projects/" + project["id"])["transcripts"] == [saved_transcript]
        checks.append("ASR cache opt-in leaves saved project transcript/data intact")

        inventory = test / "model-inventory"
        a = write(inventory / "a.gguf", b"identical-weights")
        b = write(inventory / "nested/b.safetensors", a.read_bytes())
        write(inventory / "different.bin", b"different-weights")
        write(inventory / "ignored.txt", b"ignored")
        before = {p.relative_to(inventory): digest(p) for p in inventory.rglob("*") if p.is_file()}
        scan = api("/api/storage/models/scan", "POST", {"root": str(inventory), "min_bytes": 1})
        assert scan["read_only"] is True and len(scan["files"]) == 3
        assert all(g["status"] != "sha256_identical" for g in scan["duplicate_groups"])
        scan = api("/api/storage/models/scan", "POST", {"root": str(inventory), "min_bytes": 1, "hash_duplicates": True})
        group = next(g for g in scan["duplicate_groups"] if g["status"] == "sha256_identical")
        assert {Path(path) for path in group["paths"]} == {a, b}
        assert group["sha256"] == digest(a)
        assert before == {p.relative_to(inventory): digest(p) for p in inventory.rglob("*") if p.is_file()}
        api("/api/storage/models/scan", "POST", {"root": "relative-path"}, 400)
        checks.append("model inventory read-only; duplicates confirmed only with equal SHA-256")

        # Fake executables are inert fixture bytes, and fake HTTP health prevents spawn.
        executable_dir = test / "fake-install"
        ollama = write(executable_dir / "ollama.exe", b"MZ inert fixture; never executed")
        llama = write(executable_dir / "llama-server.exe", b"MZ inert fixture; never executed")
        comfy = executable_dir / "ComfyUI-portable"
        write(comfy / "python_embeded/python.exe", b"MZ inert fixture; never executed")
        write(comfy / "ComfyUI/main.py", b"raise AssertionError('must never be launched')")
        defaults = api("/api/services")
        assert defaults["config"]["comfy_dir"] == "" and not (data / "services.json").exists()
        assert defaults["config"]["comfy_reserved_vram_gb"] == 1.0
        chosen = {"comfy_dir": str(comfy), "ollama_executable": str(ollama), "llamacpp_executable": str(llama), "llamacpp_model": str(a)}
        saved = api("/api/services", "PUT", chosen)
        assert saved["config"]["llamacpp_gpu_layers"] == 0
        partial = api("/api/services", "PUT", {"comfy_sage_attention": True, "comfy_reserved_vram_gb": 1.5})
        assert partial["config"]["ollama_executable"] == str(ollama)
        assert partial["config"]["comfy_sage_attention"] is True
        assert partial["config"]["comfy_reserved_vram_gb"] == 1.5
        for invalid_reserve in (-1, 33):
            api("/api/services", "PUT", {"comfy_reserved_vram_gb": invalid_reserve}, 400)
        api("/api/services", "PUT", {"llamacpp_context": 131072}, 400)
        api("/api/services", "PUT", {"ollama_executable": str(executable_dir / "absent.exe")}, 400)
        assert api("/api/services")["config"] == partial["config"]
        for service in ("comfyui", "llm"):
            running = api("/api/services/start", "POST", {"service": service})
            assert running["status"] == "already_running" and running["pid"] is None
        settings["llm_provider"] = "openai_compatible"
        save_settings()
        running = api("/api/services/start", "POST", {"service": "llm"})
        assert running["provider"] == "openai_compatible" and running["status"] == "already_running"
        FakeServices.malformed = True
        api("/api/services/start", "POST", {"service": "llm"}, 400)
        FakeServices.malformed = False
        settings["openai_base_url"] = "https://127.0.0.1:8080/v1"
        save_settings()
        api("/api/services/start", "POST", {"service": "llm"}, 400)
        api("/api/services/start", "POST", {"service": "anything-else"}, 400)
        assert not (data / "services").exists(), "A launcher unexpectedly created process/log records"
        checks.append("service config partial/validation; fake-online providers never spawn; busy/HTTPS starts refused")
        report = {"suite_version": version, "checks": checks, "real_external_services_started": False,
                  "cleanup_scope": "Only synthetic UUID fixture inside installation", "status": "passed"}
        artifacts = Path(__file__).resolve().parent / "artifacts"
        artifacts.mkdir(exist_ok=True)
        (artifacts / "maintenance-http-report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        print("PASS maintenance HTTP: " + "; ".join(checks), flush=True)
    except BaseException:
        log.flush()
        print("Maintenance fixture log:\n" + log_path.read_text(encoding="utf-8", errors="replace"), file=sys.stderr)
        raise
    finally:
        FakeServices.gate.set()
        process.terminate()
        process.wait(timeout=20)
        log.close()
        fake.shutdown()
        fake.server_close()
        thread.join(timeout=5)
        remove_own_fixture(test)


if __name__ == "__main__":
    main()
