"""Narrative HTTP regression: synthetic media, FAKE AI, real FFmpeg export.

No model is loaded or downloaded. This verifies orchestration, timing, review,
ownership and persistence, not the creative quality of any installed model.
"""
import contextlib
from email.parser import BytesParser
from email.policy import default as email_policy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import wave

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
from objects_http import FakeBase, free_port, png_bytes, serve

ROOT = Path(__file__).resolve().parents[2]


class FakeNarrativeLLM(FakeBase):
    requests = []
    started = threading.Event()
    release = threading.Event()

    def do_GET(self):
        return self.reply({"models": [{"name": "fake-story-model"}]})

    def do_POST(self):
        def handle():
            assert self.path == "/api/chat"
            value = self.body()
            type(self).requests.append(value)
            assert value["model"] == "fake-story-model"
            assert value["keep_alive"] == 0 and value["options"]["num_gpu"] == 0
            assert value["format"]["properties"]["scenes"]["minItems"] == 2
            type(self).started.set()
            assert type(self).release.wait(15)
            return self.reply({"message": {"content": json.dumps({"title": "Bozza sintetica", "scenes": [
                {"title": "Scena uno", "narration": "Una prova di voce italiana.",
                 "visual_prompt": "Subject: one illustrative microphone. Composition: centered. Lighting: soft. Visual treatment: clean editorial. Palette: violet. Camera: close-up. Constraints: no text or logos.", "duration_ms": 700},
                {"title": "Scena due", "narration": "La conclusione della prova.",
                 "visual_prompt": "Subject: an illustrative open book. Composition: centered. Lighting: soft. Visual treatment: clean editorial. Palette: violet. Camera: close-up. Constraints: no text or logos.", "duration_ms": 800},
            ]}, ensure_ascii=False)}})
        self.checked(handle)


class FakeNarrativeComfy(FakeBase):
    prompts = []
    png = png_bytes()
    audio = b""
    uploads = []
    expected_uploads = {}
    definitions = {
        "CLIPTextEncode": {"input": {"required": {"text": ["STRING"]}}, "output": ["CONDITIONING"]},
        "FixtureNarrator": {"input": {"required": {"text": ["STRING"]}}, "output": ["AUDIO"]},
        "FixtureModelLoader": {"input": {"required": {"model_name": [["unchanged-fixture.safetensors"]]}}, "output": ["MODEL"]},
        "SaveImage": {"input": {"required": {"filename_prefix": ["STRING"]}}, "output_node": True, "output": []},
        "SaveAudio": {"input": {"required": {"audio": ["AUDIO"], "filename_prefix": ["STRING"]}}, "output_node": True, "output": []},
        "LoadImage": {"input": {"required": {"image": ["STRING", {"image_upload": True}]}}, "output": ["IMAGE"]},
        "LoadAudio": {"input": {"required": {"audio": ["STRING", {"audio_upload": True}]}}, "output": ["AUDIO"]},
        "LoadVideo": {"input": {"required": {"file": ["STRING", {"video_upload": True}]}}, "output": ["VIDEO"]},
    }

    def do_GET(self):
        def handle():
            if self.path == "/object_info":
                return self.reply(self.definitions)
            if self.path == "/system_stats":
                return self.reply({"system": {"test_double": True}, "devices": []})
            if self.path.startswith("/history/"):
                prompt_id = self.path.rsplit("/", 1)[1]
                index = int(prompt_id.split("-")[-1])
                graph = type(self).prompts[index]["prompt"]
                is_audio = "tts" in graph
                return self.reply({prompt_id: {"status": {"completed": True, "status_str": "success"}, "outputs": {"save": {
                    "audio" if is_audio else "images": [{"filename": "fixture.wav" if is_audio else "fixture.png", "type": "output", "subfolder": ""}]
                }}}})
            if self.path.startswith("/view?"):
                filename = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)["filename"][0]
                return self.reply(self.audio if filename.endswith(".wav") else self.png,
                                  content_type="audio/wav" if filename.endswith(".wav") else "image/png")
            return self.reply({"error": "unsupported fake endpoint"}, 404)
        self.checked(handle)

    def do_POST(self):
        def handle():
            if self.path == "/upload/image":
                length = int(self.headers["Content-Length"])
                payload = self.rfile.read(length)
                message = BytesParser(policy=email_policy).parsebytes(
                    ("Content-Type: " + self.headers["Content-Type"] + "\r\n\r\n").encode() + payload)
                part = next(p for p in message.iter_parts() if p.get_param("name", header="content-disposition") == "image")
                extension = Path(part.get_filename()).suffix
                binary = part.get_payload(decode=True)
                assert binary == type(self).expected_uploads[extension], "Binary reference upload was changed"
                name = "uploaded-" + str(len(type(self).uploads)) + extension
                type(self).uploads.append(name)
                return self.reply({"name": name, "subfolder": "suite-test", "type": "input"})
            assert self.path == "/prompt"
            value = self.body()
            graph = value["prompt"]
            assert graph["model"]["inputs"]["model_name"] == "unchanged-fixture.safetensors"
            if "positive" in graph:
                assert graph["negative"]["inputs"]["text"] == "KEEP NEGATIVE EXACTLY"
                assert "Subject:" in graph["positive"]["inputs"]["text"]
                for node, field, suffix in [("ref_image", "image", ".png"), ("ref_audio", "audio", ".wav"), ("ref_video", "file", ".mp4")]:
                    reference = graph[node]["inputs"][field]
                    assert reference.startswith("suite-test/uploaded-") and reference.endswith(suffix), reference
            else:
                assert graph["tts"]["inputs"]["text"] == "Una prova di voce italiana."
            index = len(type(self).prompts)
            type(self).prompts.append(value)
            return self.reply({"prompt_id": "synthetic-narrative-" + str(index)})
        self.checked(handle)


def main():
    exe = Path(sys.argv[1]).resolve(strict=True) if len(sys.argv) > 1 else ROOT / "sviluppo/target/release/nl-video-suite.exe"
    ffmpeg = ROOT / "app/tools/ffmpeg.exe"
    ffprobe = ROOT / "app/tools/ffprobe.exe"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryDirectory(prefix="nl-suite-narrative-http-") as folder:
        test = Path(folder).resolve(strict=True)
        assert test.parent == Path(tempfile.gettempdir()).resolve(strict=True) and test.name.startswith("nl-suite-narrative-http-")
        data, workflows = test / "data", test / "workflows"
        data.mkdir(); workflows.mkdir()
        original = test / "synthetic-original.png"
        original.write_bytes(png_bytes())
        original_hash = hashlib.sha256(original.read_bytes()).hexdigest()
        short_video = test / "synthetic-short.mp4"
        subprocess.run([str(ffmpeg), "-v", "error", "-f", "lavfi", "-i", "color=red:s=96x64:r=30:d=0.3", "-c:v", "libx264", "-pix_fmt", "yuv420p", str(short_video)], check=True, capture_output=True)
        audio = io.BytesIO()
        with wave.open(audio, "wb") as out:
            out.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
            out.writeframes(b"\x00\x00" * 20800)
        FakeNarrativeComfy.audio = audio.getvalue()
        audio_reference = test / "synthetic-reference.wav"
        audio_reference.write_bytes(audio.getvalue())
        FakeNarrativeComfy.expected_uploads = {".png": original.read_bytes(), ".wav": audio.getvalue(), ".mp4": short_video.read_bytes()}
        model = {"class_type": "FixtureModelLoader", "inputs": {"model_name": "unchanged-fixture.safetensors"}}
        (workflows / "synthetic-image.json").write_text(json.dumps({
            "positive": {"class_type": "CLIPTextEncode", "_meta": {"title": "Positive (Input:prompt)"}, "inputs": {"text": "old"}},
            "negative": {"class_type": "CLIPTextEncode", "_meta": {"title": "Negative"}, "inputs": {"text": "KEEP NEGATIVE EXACTLY"}},
            "model": model, "save": {"class_type": "SaveImage", "inputs": {"filename_prefix": "synthetic"}},
            "ref_image": {"class_type": "LoadImage", "inputs": {"image": "preserved.png"}},
            "ref_audio": {"class_type": "LoadAudio", "inputs": {"audio": "preserved.wav"}},
            "ref_video": {"class_type": "LoadVideo", "inputs": {"file": "preserved.mp4"}},
        }), encoding="utf-8")
        (workflows / "synthetic-tts.json").write_text(json.dumps({
            "tts": {"class_type": "FixtureNarrator", "_meta": {"title": "Narration (Input:text)"}, "inputs": {"text": "old"}},
            "model": model, "save": {"class_type": "SaveAudio", "inputs": {"audio": ["tts", 0], "filename_prefix": "synthetic"}},
        }), encoding="utf-8")
        llm, llm_url = serve(FakeNarrativeLLM)
        comfy, comfy_url = serve(FakeNarrativeComfy)
        (data / "settings.json").write_text(json.dumps({"comfy_url": comfy_url, "ollama_url": llm_url,
            "llm_model": "fake-default", "role_llm_models": {"story": "fake-story-model"},
            "legacy_sources": {}, "workflow_roots": [str(workflows)], "ollama_execution": "cpu"}), encoding="utf-8")
        port = free_port(); base = "http://127.0.0.1:" + str(port)
        log = (test / "server.log").open("w", encoding="utf-8")
        env = {**os.environ, "NL_SUITE_ROOT": str(ROOT)}
        server = subprocess.Popen([str(exe), "--port", str(port), "--data-dir", str(data)], cwd=test, env=env, stdout=log, stderr=log)

        def http(path, method="GET", body=None, status=200):
            request = urllib.request.Request(base + path, method=method, data=json.dumps(body).encode() if body is not None else None,
                                             headers={"Content-Type": "application/json"} if body is not None else {})
            try:
                response = opener.open(request, timeout=25)
            except urllib.error.HTTPError as error:
                response = error
            content = response.read()
            assert response.status == status, (method, path, response.status, content.decode(errors="replace"))
            return json.loads(content) if content else None

        def wait_job(job, success=True):
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                value = http("/api/jobs/" + job["id"])
                if value["status"] in ("succeeded", "failed"):
                    assert (value["status"] == "succeeded") == success, value
                    return value
                time.sleep(.08)
            raise AssertionError("Job timeout " + job["id"])

        try:
            deadline = time.monotonic() + 15
            while True:
                try:
                    http("/api/projects"); break
                except (OSError, urllib.error.URLError):
                    assert server.poll() is None, (test / "server.log").read_text(encoding="utf-8")
                    assert time.monotonic() < deadline
                    time.sleep(.05)
            p = http("/api/projects", "POST", {"title": "Synthetic narrative regression"}, 201)
            prefix = "/api/projects/" + p["id"]; endpoint = prefix + "/narrative"
            legacy_story = {"premise": "Legacy must survive", "characters": [], "locations": [], "beats": [], "scenes": [{"title": "Legacy", "description": "Do not overwrite", "custom_field": "KEEP"}]}
            http(prefix + "/story", "PUT", legacy_story)
            for source in [original, short_video, audio_reference]:
                wait_job(http(prefix + "/assets", "POST", {"path": str(source)}, 202))
            p = http(prefix)
            image = next(a for a in p["assets"] if a["kind"] == "image")
            video = next(a for a in p["assets"] if a["kind"] == "video")
            audio_asset = next(a for a in p["assets"] if a["kind"] == "audio")
            original_clip = http(prefix + "/timeline/clips", "POST", {"asset_id": image["id"], "start_ms": 0, "duration_ms": 500}, 201)
            prod = http(endpoint)
            assert prod["scenes"][0]["narration"] == "Do not overwrite"
            assert prod == http(endpoint), "legacy adaptation IDs were not frozen"
            http(endpoint + "/plan", "POST", {"premise": "Test", "scene_count": 2}, 400)
            job = http(endpoint + "/plan", "POST", {"premise": "Una storia sintetica", "scene_count": 2, "replace": True}, 202)
            assert FakeNarrativeLLM.started.wait(10)
            http(endpoint, "PUT", {"production": prod}, 400)
            http(endpoint + "/plan", "POST", {"premise": "Test", "scene_count": 2, "replace": True}, 400)
            FakeNarrativeLLM.release.set(); wait_job(job)
            prod = http(endpoint)
            assert len(prod["scenes"]) == 2 and not prod["script_approved"]
            assert len(http(prefix)["timeline"]["clips"]) == 1
            http(endpoint + "/generate", "POST", {"visuals": True}, 400)
            catalog = {w["name"]: w["id"] for w in http("/api/workflows")}
            for scene in prod["scenes"]:
                scene["visual_workflow"] = {"workflow_id": catalog["synthetic-image"], "text_input": "positive.text",
                    "asset_inputs": {"ref_image.image": image["id"], "ref_audio.audio": audio_asset["id"], "ref_video.file": video["id"]}}
            prod["scenes"][0]["voice_mode"] = "generate"
            prod["scenes"][0]["voice_workflow"] = {"workflow_id": catalog["synthetic-tts"], "text_input": "tts.text"}
            stale = json.loads(json.dumps(prod))
            prod = http(endpoint, "PUT", {"production": prod})
            http(endpoint, "PUT", {"production": stale}, 409)
            prod["script_approved"] = True
            prod = http(endpoint, "PUT", {"production": prod})
            assert prod["script_approved"]
            generated = http(endpoint + "/generate", "POST", {"visuals": True, "voices": True}, 202)
            wait_job(generated)
            assert len(FakeNarrativeComfy.prompts) == 3, "missing or duplicate sequential workflow children"
            assert len(FakeNarrativeComfy.uploads) == 6, "missing binary image/audio/video reference uploads"
            prod = http(endpoint)
            assert all(s["visual_asset_id"] and not s["visual_approved"] for s in prod["scenes"])
            assert prod["scenes"][0]["voice_asset_id"] and not prod["scenes"][0]["voice_approved"]
            assert len(http(prefix)["timeline"]["clips"]) == 1, "generated media auto-inserted before approval"
            http(endpoint + "/assemble", "POST", {"append": True}, 400)
            http(endpoint + "/generate", "POST", {"visuals": True, "voices": True}, 400)
            assert len(FakeNarrativeComfy.prompts) == 3, "existing output was silently re-generated"
            for scene in prod["scenes"]:
                prod = http(endpoint + "/approve", "POST", {"scene_id": scene["id"], "kind": "visual", "asset_id": scene["visual_asset_id"]})
            scene = prod["scenes"][0]; generated_image = scene["visual_asset_id"]
            http(endpoint + "/approve", "POST", {"scene_id": scene["id"], "kind": "voice", "asset_id": image["id"]}, 400)
            prod = http(endpoint + "/approve", "POST", {"scene_id": scene["id"], "kind": "voice", "asset_id": scene["voice_asset_id"]})
            http(endpoint + "/approve", "POST", {"scene_id": scene["id"], "kind": "visual", "asset_id": video["id"]})
            error = http(endpoint + "/assemble", "POST", {"append": True}, 400)
            assert "più corto" in error["error"] and "tagliata" in error["error"]
            http(endpoint + "/approve", "POST", {"scene_id": scene["id"], "kind": "visual", "asset_id": generated_image})
            assembled = http(endpoint + "/assemble", "POST", {"append": True})
            clips = assembled["project"]["timeline"]["clips"]
            assert len(clips) == 4 and any(c["id"] == original_clip["id"] for c in clips)
            voiced = next(c for c in clips if c["asset_id"] == scene["voice_asset_id"])
            assert voiced["start_ms"] == 500 and abs(voiced["duration_ms"] - 1300) <= 1, voiced
            assert next(c for c in clips if c["asset_id"] == generated_image)["duration_ms"] == voiced["duration_ms"]
            again = http(endpoint + "/assemble", "POST", {"append": True})
            assert again["project"]["timeline"]["clips"] == clips, "assembly was not idempotent"
            locked = again["project"]
            next(t for t in locked["timeline"]["tracks"] if t["id"] == voiced["track_id"])["locked"] = True
            locked = http(prefix + "/timeline", "PUT", {"timeline": locked["timeline"], "expected_updated_at": locked["updated_at"]})
            http(endpoint + "/assemble", "POST", {"append": True}, 400)
            next(t for t in locked["timeline"]["tracks"] if t["id"] == voiced["track_id"])["locked"] = False
            http(prefix + "/timeline", "PUT", {"timeline": locked["timeline"], "expected_updated_at": locked["updated_at"]})
            export = wait_job(http(endpoint + "/export", "POST", {"scene_timed_subtitles": True}, 202))
            output = Path(export["output_paths"][0]).resolve(strict=True)
            assert any(parent.samefile(test) for parent in output.parents)
            info = json.loads(subprocess.check_output([str(ffprobe), "-v", "error", "-show_entries", "format=duration", "-of", "json", str(output)]))
            assert abs(float(info["format"]["duration"]) - 2.6) < .15, info
            sidecar = Path(export["result"]["scene_timed_subtitles"]).resolve(strict=True)
            assert any(parent.samefile(test) for parent in sidecar.parents)
            assert "Una prova di voce italiana." in sidecar.read_text(encoding="utf-8")
            staged_media = [f for f in (data / "generated").rglob("*") if f.suffix in (".png", ".wav")]
            assert not staged_media, "Project output still duplicated in staging"
            assert export["result"]["alignment"] == "scene_timing_only_not_word_aligned"
            assert http(prefix)["story"] == legacy_story, "new narrative overwrote legacy Story"
            assert hashlib.sha256(original.read_bytes()).hexdigest() == original_hash
            prod = http(endpoint)
            prod["scenes"][0]["narration"] += " Modifica."
            changed = http(endpoint, "PUT", {"production": prod})
            assert not changed["script_approved"] and not changed["scenes"][0]["voice_approved"]
            http(endpoint + "/export", "POST", {}, 400)
            assert not FakeNarrativeLLM.failures and not FakeNarrativeComfy.failures, (FakeNarrativeLLM.failures, FakeNarrativeComfy.failures)
            print("PASS narrative: Italian script review -> 3 serial fake Comfy children -> explicit output approval -> voice-timed idempotent assembly -> real FFmpeg export/SRT; legacy/source preserved.")
        finally:
            FakeNarrativeLLM.release.set()
            server.terminate()
            with contextlib.suppress(subprocess.TimeoutExpired):
                server.wait(timeout=10)
            if server.poll() is None:
                server.kill(); server.wait()
            log.close(); llm.shutdown(); comfy.shutdown()


if __name__ == "__main__":
    main()
