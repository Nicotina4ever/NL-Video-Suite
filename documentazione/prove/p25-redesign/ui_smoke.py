"""P25 UI smoke: Montaggio base with NL tabs, top-level NL menu, NL Agent panel."""

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
EXE = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "filmcraft-nl" / "target" / "debug" / "filmcraft.exe"
PROJECT = ROOT / "documentazione" / "prove" / "f4-shorts" / "gta6-shorts.fcproj"
OUT = ROOT / "documentazione" / "prove" / "p25-redesign"
OUT.mkdir(parents=True, exist_ok=True)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class C:
    def __init__(self, port, token):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=10)
        self.r = self.sock.makefile("r", encoding="utf-8")
        self.w = self.sock.makefile("w", encoding="utf-8")
        self.token = token
        self.i = 0

    def call(self, method, **params):
        self.i += 1
        req = {"id": self.i, "token": self.token, "method": method}
        if params:
            req["params"] = params
        self.w.write(json.dumps(req) + "\n")
        self.w.flush()
        return json.loads(self.r.readline())

    def ok(self, method, **params):
        v = self.call(method, **params)
        if not v.get("ok"):
            raise RuntimeError(f"{method} {params}: {v.get('error')}")
        return v.get("result")


def main():
    work = Path(tempfile.mkdtemp(prefix="nl-p25-ui-"))
    data = work / "data"
    data.mkdir()
    project = work / "gta6.fcproj"
    project.write_bytes(PROJECT.read_bytes())
    port = free_port()
    token = secrets.token_hex(32)
    env = dict(os.environ)
    env["FILMCRAFT_CONTROL_TOKEN"] = token
    env["NL_SUITE_ROOT"] = str(ROOT)
    process = subprocess.Popen([str(EXE), "--control", str(port), "--control-input-only", "--data-dir", str(data), str(project)], env=env)
    try:
        deadline = time.time() + 120
        while time.time() < deadline:
            try:
                socket.create_connection(("127.0.0.1", port), timeout=1).close()
                break
            except OSError:
                time.sleep(0.5)
        c = C(port, token)
        c.ok("engine.execute", command="window.workspace.editing", params={})
        c.ok("ui.set", workspace="Editing")
        time.sleep(0.5)
        inspect = c.ok("ui.inspect")
        c.ok("ui.screenshot", path=str(OUT / "editing-nl-tabs.png"))
        c.ok("ui.panel.show", panel="NL Agent")
        time.sleep(0.4)
        c.ok("ui.screenshot", path=str(OUT / "nl-agent-panel.png"), panel="NL Agent")
        elements = c.ok("ui.elements", prefix="nl.agent.")
        # Timeline right-click: the NL submenu must appear at the pointer.
        c.ok("ui.click", x=900, y=620, button="right")
        time.sleep(0.3)
        menu_before = c.ok("ui.elements", prefix="timeline.nlMenu")
        timeline_menu = [e.get("id") for e in menu_before]
        if "timeline.nlMenu.open" in timeline_menu:
            c.ok("ui.click", id="timeline.nlMenu.open")
            time.sleep(0.3)
            c.ok("ui.screenshot", path=str(OUT / "timeline-nl-menu.png"))
            menu_after = c.ok("ui.elements", prefix="timeline.nlMenu")
            timeline_menu = [e.get("id") for e in menu_after]
        else:
            c.ok("ui.screenshot", path=str(OUT / "timeline-context.png"))
        items = c.ok("ui.menu.list")
        def top_paths():
            paths = set()
            for item in items:
                path = item.get("path") or []
                if path:
                    paths.add((path[0], path[1] if len(path) > 1 else ""))
            return sorted(paths)
        report = {
            "workspace": inspect.get("ui", {}).get("workspace"),
            "agent_elements": [e.get("id") for e in elements],
            "timeline_menu": timeline_menu,
            "menu": top_paths(),
            "dock": inspect.get("ui", {}).get("dock"),
            "screenshots": ["editing-nl-tabs.png", "nl-agent-panel.png"],
        }
        (OUT / "smoke.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
        print(json.dumps({"workspace": report["workspace"], "agent_elements": report["agent_elements"], "menu_has_nl": any(a == "NL Video Suite" for a, _ in report["menu"])}, indent=2))
        c.call("app.quit")
    finally:
        try:
            process.wait(timeout=20)
        except Exception:
            process.terminate()
            try:
                process.wait(timeout=10)
            except Exception:
                process.kill()


if __name__ == "__main__":
    main()
