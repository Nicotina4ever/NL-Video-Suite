"""Baseline P19: what the current build really shows for the NL panels.

Starts the desktop app with the control channel, opens a COPY of the GTA6 project, and records:
- the Window > Workspaces menu (is "NL Video Suite" there?),
- the panels that show the generic placeholder,
- the elements each NL panel registers,
- screenshots in `documentazione/prove/p19-nl-surface/before/`.

Run:  python documentazione/prove/p19-nl-audit/baseline.py
"""

import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(r"K:\AI\NL_Video_Suite")
EXE = ROOT / "NL Video Suite.exe"
PROJECT = ROOT / "documentazione" / "prove" / "f4-shorts" / "gta6-shorts.fcproj"
OUT = ROOT / "documentazione" / "prove" / "p19-nl-surface"
BEFORE = OUT / "before"

PORT_CANDIDATES = [9877, 9879, 9881, 9883]
PANELS = [
    "NL Home", "NL Generate", "NL Music", "NL Bridge", "NL Story", "NL Cover", "NL Queue",
    "NL Clips AI", "NL Sgrossa", "NL Montage", "NL Longform", "NL Analyze", "NL Search",
    "NL Shot Map", "NL Social", "NL Studio", "NL Subtitles", "NL Titles", "NL Graphics",
    "NL Audio", "NL Delivery", "NL Album", "NL Versions", "NL ASR", "NL Speech",
    "NL Services", "NL Settings", "NL Workflows",
]


class Client:
    def __init__(self, port, token):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=5)
        self.reader = self.sock.makefile("r", encoding="utf-8")
        self.writer = self.sock.makefile("w", encoding="utf-8")
        self.token = token
        self.next_id = 1

    def call(self, method, **params):
        request = {"id": self.next_id, "token": self.token, "method": method}
        if params:
            request["params"] = params
        self.next_id += 1
        self.writer.write(json.dumps(request) + "\n")
        self.writer.flush()
        line = self.reader.readline()
        if not line:
            raise RuntimeError("control connection closed")
        return json.loads(line)


def find_port():
    for port in PORT_CANDIDATES:
        s = socket.socket()
        try:
            s.bind(("127.0.0.1", port))
        except OSError:
            continue
        finally:
            s.close()
        return port
    raise RuntimeError("no free control port")


def wait_for_server(port, timeout=90.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return
        except OSError:
            time.sleep(0.5)
    raise RuntimeError("app did not open the control port in time")


def main():
    if not EXE.is_file():
        raise SystemExit("missing exe: %s" % EXE)
    BEFORE.mkdir(parents=True, exist_ok=True)
    data = Path(tempfile.mkdtemp(prefix="nl-p19-data-"))
    project = Path(tempfile.mkdtemp(prefix="nl-p19-project-")) / "gta6-copy.fcproj"
    project.write_bytes(PROJECT.read_bytes())
    port = find_port()
    token = secrets.token_hex(32)
    env = dict(os.environ)
    env["FILMCRAFT_CONTROL_TOKEN"] = token
    env["NL_SUITE_ROOT"] = str(ROOT)
    args = [str(EXE), "--control", str(port), "--control-input-only", "--data-dir", str(data), str(project)]
    proc = subprocess.Popen(args, env=env, cwd=str(ROOT))
    report = {
        "date": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "exe": str(EXE),
        "exe_mtime": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(EXE.stat().st_mtime)),
        "project": str(project),
        "port": port,
        "panels": {},
        "workspace_menu": [],
        "inspect": None,
    }
    try:
        wait_for_server(port)
        client = Client(port, token)
        report["inspect"] = client.call("ui.inspect")
        docking = client.call("ui.docking")
        report["docking"] = docking.get("result")
        menu = client.call("ui.menu.list")
        menu_result = menu.get("result")
        if isinstance(menu_result, list):
            items = menu_result
        elif isinstance(menu_result, dict):
            items = menu_result.get("items", [])
        else:
            items = []
        report["workspace_menu"] = [
            item.get("label") for item in items
            if item.get("path") == ["Window", "Workspaces"]
        ]
        for panel in PANELS:
            shown = client.call("ui.panel.show", panel=panel)
            time.sleep(0.35)
            elements = client.call("ui.elements", prefix="nl.")
            elements_inner = elements.get("result", {})
            if isinstance(elements_inner, dict):
                rows = elements_inner.get("elements", [])
            else:
                rows = elements_inner if isinstance(elements_inner, list) else []
            labels = [row.get("label", "") for row in rows if isinstance(row, dict)]
            screenshot = BEFORE / (panel.replace(" ", "_") + ".png")
            shot = client.call("ui.screenshot", path=str(screenshot), panel=panel)
            focused = client.call("ui.inspect")
            report["panels"][panel] = {
                "show_ok": bool(shown.get("ok")),
                "show_error": shown.get("error"),
                "element_count": len(rows),
                "labels": labels[:40],
                "placeholder_text": any("coming in a later milestone" in label for label in labels),
                "screenshot": str(screenshot),
                "screenshot_ok": bool(shot.get("ok")),
                "focused_panel": (focused.get("result", {}) or {}).get("ui", {}).get("focused"),
            }
        client.call("app.quit")
    finally:
        try:
            proc.wait(timeout=20)
        except Exception:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except Exception:
                proc.kill()
    report["summary"] = {
        "panels": len(report["panels"]),
        "with_placeholder": sorted(p for p, v in report["panels"].items() if v["placeholder_text"]),
        "with_no_elements": sorted(p for p, v in report["panels"].items() if v["element_count"] == 0),
        "workspace_listed": "NL Video Suite" in report["workspace_menu"],
    }
    out = OUT / "baseline.json"
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(report["summary"], indent=2, ensure_ascii=False))
    print("written: %s" % out)


if __name__ == "__main__":
    main()
