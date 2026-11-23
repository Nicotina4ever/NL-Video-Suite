"""Live check of the Ace Step music bridge on a disposable demo project.

Run the release app with `--control 9876` and FILMCRAFT_CONTROL_TOKEN. ComfyUI must be idle
and Ollama unloaded. The generated audio is imported into the demo project, never saved.
"""

import json
import os
import socket
import time

TOKEN = os.environ["FILMCRAFT_CONTROL_TOKEN"]
OUT = r"K:\AI\NL_Video_Suite\documentazione\prove\f6-audio"
PROMPT = "warm cinematic soul, 92 bpm, Rhodes piano, brushed drums, soft bass, intimate late-night mix"


def call(method, **params):
    with socket.create_connection(("127.0.0.1", 9876), timeout=60) as s:
        f = s.makefile("rw", encoding="utf-8")
        f.write(json.dumps({"id": 1, "token": TOKEN, "method": method, "params": params}) + "\n")
        f.flush()
        return json.loads(f.readline())


def main():
    for _ in range(120):
        try:
            if call("ui.inspect").get("ok"):
                break
        except OSError:
            pass
        time.sleep(0.5)
    os.makedirs(OUT, exist_ok=True)
    print("demo", call("engine.execute", command="file.openDemoProject", params={}).get("ok"), flush=True)
    started = call(
        "engine.execute",
        command="audio.generateMusic",
        params={
            "prompt": PROMPT,
            "workflow": "workflows/regista/audio/audio_ace_step1_5_xl_sft.json",
            "seconds": 30,
            "wait": False,
            "timeoutSeconds": 2400,
        },
    )
    print("start", json.dumps(started.get("result") or started, ensure_ascii=False), flush=True)
    deadline = time.time() + 2400
    finished = False
    while time.time() < deadline:
        time.sleep(5)
        try:
            jobs = call("engine.execute", command="jobs.list", params={}).get("result") or []
        except OSError as e:
            print("app gone", e, flush=True)
            return
        job = next((j for j in jobs if "ComfyUI" in (j.get("label") or "")), None)
        if job is None:
            continue
        print("poll", job.get("status"), job.get("finished"), str(job.get("result"))[:300], flush=True)
        if job.get("finished"):
            finished = True
            break
    print("finished", finished, flush=True)
    if not finished:
        return
    time.sleep(3)
    try:
        project = call("engine.execute", command="project.inspect", params={}).get("result") or {}
        items = project.get("root", {}).get("children", [])
        print("project", [(c.get("name"), c.get("kind")) for c in items][-12:], flush=True)
        print("shot", call("ui.screenshot", path=os.path.join(OUT, "music.png")).get("ok"), flush=True)
        print("quit", call("engine.execute", command="app.quit", params={}).get("ok"), flush=True)
    except OSError as e:
        print("post error", e, flush=True)


if __name__ == "__main__":
    main()
