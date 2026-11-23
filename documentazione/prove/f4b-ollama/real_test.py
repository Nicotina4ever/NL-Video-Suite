"""End-to-end check of Find Shorts with AI on the GTA6 project.

Run the release app with `--control 9876` and the same FILMCRAFT_CONTROL_TOKEN, then:
    python real_test.py [model]
The script imports the NL project, starts the AI ranking job, waits for it and prints
the resulting bin/markers. It never overwrites the user's project on disk.
"""

import json
import os
import socket
import sys
import time

TOKEN = os.environ["FILMCRAFT_CONTROL_TOKEN"]
PROJECT = r"K:\AI\NL_Video_Suite\sviluppo\anteprima_creativa\data\projects\Mi ficco in GTA6--3dd6fb69\project.json"
OUT = r"K:\AI\NL_Video_Suite\documentazione\prove\f4b-ollama"
MODEL = sys.argv[1] if len(sys.argv) > 1 else "granite4.2:3b"


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
    print("import", call("engine.execute", command="file.importNlProject", params={"path": PROJECT}).get("ok"), flush=True)
    started = call(
        "engine.execute",
        command="sequence.findShortsAi",
        params={"model": MODEL, "minSeconds": 10, "maxSeconds": 45, "count": 3, "wait": False},
    )
    print("start", json.dumps(started.get("result") or started, ensure_ascii=False), flush=True)

    deadline = time.time() + 1800
    finished = False
    while time.time() < deadline:
        time.sleep(10)
        try:
            jobs = call("engine.execute", command="jobs.list", params={}).get("result") or []
        except OSError as e:
            print("app gone", e, flush=True)
            return
        ai = [j for j in jobs if "Find Shorts AI" in (j.get("label") or "")]
        if not ai:
            continue
        job = ai[-1]
        print("poll", job.get("status"), job.get("done"), job.get("total"), job.get("finished"), str(job.get("result"))[:400], flush=True)
        if job.get("finished"):
            finished = True
            break
    print("finished", finished, flush=True)
    if not finished:
        return

    try:
        project = call("engine.execute", command="project.inspect", params={}).get("result") or {}
        bins = [c for c in project.get("root", {}).get("children", []) if c.get("name") == "AI Shorts"]
        clips = bins[0]["children"] if bins else []
        print("AI Shorts", len(clips), [c.get("name") for c in clips], flush=True)
        sequence = call("engine.execute", command="sequence.inspect", params={}).get("result") or {}
        print("markers", [(m.get("name"), m.get("comment")) for m in (sequence.get("markers") or [])], flush=True)
        print("shot", call("ui.screenshot", path=os.path.join(OUT, "ai-shorts.png")).get("ok"), flush=True)
        print("close", call("engine.execute", command="file.closeProject", params={"force": True}).get("ok"), flush=True)
        print("quit", call("engine.execute", command="app.quit", params={}).get("ok"), flush=True)
    except OSError as e:
        print("post error", e, flush=True)


if __name__ == "__main__":
    main()
