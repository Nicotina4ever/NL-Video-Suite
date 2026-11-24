"""Drive the visible NL Video Suite window on port 55901; keeps the app open."""

import json
import socket
import time
from pathlib import Path

GUI = Path(r"C:\Users\Nicola\AppData\Local\Temp\nl-gui-live")
OUT = Path(r"K:\AI\NL_Video_Suite\documentazione\prove\p27-gui")
OUT.mkdir(parents=True, exist_ok=True)
TOKEN = (GUI / "token.txt").read_text().strip()


class C:
    def __init__(self):
        self.sock = socket.create_connection(("127.0.0.1", 55901), timeout=10)
        self.r = self.sock.makefile("r", encoding="utf-8")
        self.w = self.sock.makefile("w", encoding="utf-8")
        self.i = 0

    def call(self, method, **params):
        self.i += 1
        req = {"id": self.i, "token": TOKEN, "method": method}
        if params:
            req["params"] = params
        self.w.write(json.dumps(req) + "\n")
        self.w.flush()
        return json.loads(self.r.readline())

    def ok(self, method, **params):
        value = self.call(method, **params)
        if not value.get("ok"):
            raise RuntimeError(f"{method}: {value.get('error')}")
        return value.get("result")


c = C()
c.ok("ui.set", workspace="Editing")
time.sleep(0.4)
c.ok("ui.screenshot", path=str(OUT / "editing.png"))
for panel, name in [("NL Agent", "agent"), ("NL Generate", "generate"), ("NL Bridge", "bridge"), ("NL Transcript", "transcript")]:
    c.ok("ui.panel.show", panel=panel)
    time.sleep(0.5)
    c.ok("ui.screenshot", path=str(OUT / f"{panel.lower().replace(' ', '-')}.png"), panel=panel)
elements = c.ok("ui.elements", prefix="nl.")
report = {"workspace": c.ok("ui.inspect").get("ui", {}).get("workspace"), "elements": [e.get("id") for e in elements]}
(OUT / "gui.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
print(json.dumps({"workspace": report["workspace"], "elements": len(report["elements"])}, indent=2))
