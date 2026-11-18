"""Real HTTP regression for browser-compatible playback, using synthetic HEVC.

No AI service or model is launched and no user media is read. FFmpeg creates a
small disposable HEVC 10-bit / AAC fixture. The Rust server imports its own copy,
prepares a browser proxy and serves that proxy over actual HTTP byte ranges.

Usage: runtime/python/python.exe sviluppo/tests/playback_http.py [release.exe]
"""
from concurrent.futures import ThreadPoolExecutor
import contextlib
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "sviluppo/.playback-test-fixtures"
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def free_port():
    with socket.socket() as connection:
        connection.bind(("127.0.0.1", 0))
        return connection.getsockname()[1]


def sha256(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def no_reparse(path):
    assert not path.is_symlink(), path
    if os.name == "nt":
        assert not (path.lstat().st_file_attributes & 0x400), path


def remove_fixture(path):
    """Delete only the exact UUID test subtree, after validating its real scope."""
    parent = FIXTURES.resolve(strict=True)
    target = path.resolve(strict=True)
    assert uuid.UUID(target.name).hex == target.name, target
    assert target.parent == parent and target.is_relative_to(ROOT.resolve()), target
    no_reparse(FIXTURES)
    no_reparse(path)
    for directory, folders, files in os.walk(target, followlinks=False):
        no_reparse(Path(directory))
        for name in folders + files:
            no_reparse(Path(directory) / name)
    shutil.rmtree(target)


def run_checked(command, **kwargs):
    result = subprocess.run(command, capture_output=True, timeout=90, **kwargs)
    assert result.returncode == 0, (command, result.stderr.decode(errors="replace"))
    return result.stdout


def probe(executable, path):
    return json.loads(run_checked([str(executable), "-v", "error", "-show_streams",
                                   "-show_format", "-of", "json", str(path)]))


def main():
    executable = (Path(sys.argv[1]) if len(sys.argv) > 1
                  else ROOT / "sviluppo/target/release/nl-video-suite.exe").resolve(strict=True)
    version = run_checked([str(executable), "--version"], cwd=ROOT).decode().strip()
    assert tuple(map(int,version.split('.'))) >= (0,5,0), "Expected suite 0.5 or newer, got " + version
    ffmpeg = (ROOT / "app/tools/ffmpeg.exe").resolve(strict=True)
    ffprobe = (ROOT / "app/tools/ffprobe.exe").resolve(strict=True)
    FIXTURES.mkdir(exist_ok=True)
    no_reparse(FIXTURES)
    test = FIXTURES / uuid.uuid4().hex
    test.mkdir()
    server = None
    log = None
    try:
        data = test / "data"
        data.mkdir()
        # Port 9 has no intended AI service: playback requires neither provider.
        (data / "settings.json").write_text(json.dumps({
            "ollama_url": "http://127.0.0.1:9", "comfy_url": "http://127.0.0.1:9",
            "legacy_sources": {}, "workflow_roots": [],
        }), encoding="utf-8")
        source = test / "synthetic HEVC 10-bit source.mp4"
        run_checked([str(ffmpeg), "-nostdin", "-v", "error", "-n", "-threads", "2",
                     "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=60:duration=1",
                     "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=1",
                     "-map", "0:v:0", "-map", "1:a:0", "-c:v", "libx265",
                     "-preset", "ultrafast", "-crf", "34", "-pix_fmt", "yuv420p10le",
                     "-x265-params", "pools=1:frame-threads=1:log-level=error",
                     "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart", str(source)])
        before = probe(ffprobe, source)
        input_video = next(item for item in before["streams"] if item["codec_type"] == "video")
        assert input_video["codec_name"] == "hevc" and input_video["pix_fmt"] == "yuv420p10le", input_video
        assert (input_video["width"], input_video["height"]) == (1920, 1080)
        assert Fraction(input_video["avg_frame_rate"]) == 60, input_video
        original_sha = sha256(source)
        original_stat = source.stat()
        port = free_port()
        base = "http://127.0.0.1:" + str(port)
        log_path = test / "server.log"
        log = log_path.open("w", encoding="utf-8")
        environment = dict(os.environ, NL_SUITE_ROOT=str(ROOT))
        server = subprocess.Popen([str(executable), "--port", str(port), "--data-dir", str(data)],
                                  cwd=test, env=environment, stdout=log, stderr=log)

        def http(path, method="GET", body=None, statuses=(200,), headers=None, raw=False):
            payload = json.dumps(body).encode() if body is not None else None
            request_headers = dict(headers or {})
            if body is not None:
                request_headers["Content-Type"] = "application/json"
            request = urllib.request.Request(base + path, data=payload, method=method,
                                             headers=request_headers)
            try:
                response = OPENER.open(request, timeout=30)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                content = response.read()
                status = response.status
                response_headers = {name.lower(): item for name, item in response.headers.items()}
            assert status in statuses, (method, path, status, content.decode(errors="replace"))
            value = content if raw else json.loads(content) if content else None
            return value, status, response_headers

        def value(path, method="GET", body=None, statuses=(200,)):
            return http(path, method, body, statuses)[0]

        def wait_job(job_id):
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                job = value("/api/jobs/" + job_id)
                if job["status"] in ("succeeded", "failed"):
                    assert job["status"] == "succeeded", job
                    return job
                time.sleep(.05)
            raise AssertionError("Job timeout: " + job_id)

        deadline = time.monotonic() + 15
        while True:
            try:
                value("/api/projects")
                break
            except (OSError, urllib.error.URLError):
                assert server.poll() is None, log_path.read_text(encoding="utf-8", errors="replace")
                assert time.monotonic() < deadline, "Server start timeout"
                time.sleep(.05)
        project = value("/api/projects", "POST", {"title": "Synthetic HEVC playback regression"}, (201,))
        prefix = "/api/projects/" + project["id"]
        imported = value(prefix + "/assets", "POST", {"path": str(source)}, (202,))
        wait_job(imported["id"])
        project = value(prefix)
        assert len(project["assets"]) == 1 and not project["timeline"]["clips"], project
        asset = project["assets"][0]
        owned = (data / "projects" / project["id"] / "assets" / asset["file_name"]).resolve(strict=True)
        assert owned.is_relative_to(test.resolve()) and owned != source
        assert sha256(owned) == original_sha
        owned_stat = owned.stat()
        endpoint = prefix + "/assets/" + asset["id"] + "/playback"
        descriptor = value(endpoint)
        assert descriptor["mode"] == "proxy_required" and not descriptor.get("url"), descriptor
        jobs_before = {job["id"] for job in value("/api/jobs")}
        # Concurrent submissions exercise the backend gate, not client-side dedup.
        with ThreadPoolExecutor(max_workers=6) as workers:
            submissions = list(workers.map(lambda _: http(endpoint, "POST", {}, (200, 202)), range(6)))
        ids = {body["job_id"] for body, status, _ in submissions if status == 202}
        assert len(ids) == 1, submissions
        for body, status, _ in submissions:
            assert body["mode"] == ("preparing" if status == 202 else "proxy"), body
        preparation_id = next(iter(ids))
        completed = wait_job(preparation_id)
        new_jobs = [job for job in value("/api/jobs") if job["id"] not in jobs_before]
        assert len(new_jobs) == 1 and new_jobs[0]["id"] == preparation_id, new_jobs
        assert completed["kind"] == "prepare_preview", completed
        descriptor = value(endpoint)
        assert descriptor["mode"] == "proxy", descriptor
        proxy_url = descriptor["url"]
        assert proxy_url == endpoint + "/file", descriptor
        cached, cached_status, _ = http(endpoint, "POST", {}, (200,))
        assert cached_status == 200 and cached["url"] == proxy_url
        assert len(value("/api/jobs")) == len(jobs_before) + 1
        proxy_bytes, _, response_headers = http(proxy_url, raw=True)
        assert "video/mp4" in response_headers.get("content-type", ""), response_headers
        assert len(proxy_bytes) > 1000 and proxy_bytes != source.read_bytes()
        ranged, _, range_headers = http(proxy_url, statuses=(206,), headers={"Range": "bytes=0-63"}, raw=True)
        assert ranged == proxy_bytes[:64] and len(ranged) == 64
        assert range_headers["content-range"] == "bytes 0-63/" + str(len(proxy_bytes)), range_headers
        downloaded = test / "downloaded browser proxy.mp4"
        downloaded.write_bytes(proxy_bytes)
        actual = probe(ffprobe, downloaded)
        video = next(item for item in actual["streams"] if item["codec_type"] == "video")
        audio = next(item for item in actual["streams"] if item["codec_type"] == "audio")
        assert video["codec_name"] == "h264" and video["pix_fmt"] == "yuv420p", video
        assert video["width"] <= 960 and video["height"] <= 720, video
        assert video["width"] % 2 == 0 and video["height"] % 2 == 0, video
        assert 0 < Fraction(video["avg_frame_rate"]) <= 30, video
        assert audio["codec_name"] == "aac", audio
        assert abs(float(actual["format"]["duration"]) - float(before["format"]["duration"])) < .15
        for path, initial in ((source, original_stat), (owned, owned_stat)):
            assert sha256(path) == original_sha, "Original media changed: " + str(path)
            current = path.stat()
            assert current.st_size == initial.st_size and current.st_mtime_ns == initial.st_mtime_ns, path
        after = value(prefix)
        assert after["assets"] == project["assets"] and not after["timeline"]["clips"], after
        cache_directory = data / "previews" / project["id"]
        assert len(list(cache_directory.glob("*-playback-v2.mp4"))) == 1
        regenerated = value(endpoint + "?refresh=true", "POST", {}, (202,))
        assert regenerated["job_id"] != preparation_id, "Explicit regeneration must create a new job, not return the old file"
        wait_job(regenerated["job_id"])
        assert value(endpoint)["mode"] == "proxy"
        assert len(list(cache_directory.glob("*-playback-v2.mp4"))) == 1
        assert not list(cache_directory.glob("*.tmp")), "Partial proxy leaked"
        print(json.dumps({"passed": True, "version": version,
            "transport": "real HTTP to compiled Rust, real FFmpeg/ffprobe",
            "input": "synthetic HEVC 10-bit 1920x1080 60 fps + AAC",
            "output": {"codec": video["codec_name"], "pixel_format": video["pix_fmt"],
                       "width": video["width"], "height": video["height"],
                       "fps": video["avg_frame_rate"], "audio": audio["codec_name"]},
            "verified": ["proxy-required descriptor", "six concurrent submissions deduplicate to one job",
                "cached proxy reuse", "MP4 download", "HTTP 206 byte ranges", "small H.264/AAC proxy",
                "source and imported original SHA/size/mtime unchanged", "assets and timeline unchanged",
                "no partial cache files"],
            "actual_ai_services": False, "actual_user_media": False,
            "browser_decode_test": False}, indent=2))
    except Exception:
        if log:
            log.flush()
            with contextlib.suppress(OSError):
                print((test / "server.log").read_text(encoding="utf-8", errors="replace"), file=sys.stderr)
        raise
    finally:
        if server:
            server.terminate()
            with contextlib.suppress(subprocess.TimeoutExpired):
                server.wait(timeout=8)
            if server.poll() is None:
                server.kill()
                server.wait(timeout=5)
        if log:
            log.close()
        remove_fixture(test)


if __name__ == "__main__":
    main()
