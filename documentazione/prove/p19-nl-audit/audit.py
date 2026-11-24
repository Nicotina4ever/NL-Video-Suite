"""P19 NL audit: prove every NL panel is active, bound to the open sequence and to real commands.

Starts the desktop app with the control channel, opens a COPY of the GTA6 project, selects the
"NLVideosuite" workspace and, for every panel:
  - shows it and reads `ui.elements`,
  - requires the shared `nl.context` line (project + sequence + playhead) and no
    `panel.placeholder`,
  - saves a screenshot,
  - runs one safe command that belongs to the panel and records ok / expected refusal.
It also checks the workspace menu, the docking layout and NL Home buttons.

Run:
  python documentazione/prove/p19-nl-audit/audit.py --exe "K:\\AI\\NL_Video_Suite\\NL Video Suite.exe"

The app is started with a temporary data directory and a project copy; the source project is never
modified. No AI service is started.
"""

import argparse
import json
import os
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_EXE = ROOT / "NL Video Suite.exe"
DEFAULT_PROJECT = ROOT / "documentazione" / "prove" / "f4-shorts" / "gta6-shorts.fcproj"

# The 29 panels, their title (as `ui.panel.show` accepts) and a safe command for the audit.
PANELS = [
    ("NL Home", "NlHome", "project.inspect", {}),
    ("NL Agent", "NlAgent", "nl.agent.plan", {}),
    ("NL Generate", "NlGenerate", "nl.models.workflows", {}),
    ("NL Music", "NlMusic", "nl.models.workflows", {}),
    ("NL ASR", "NlAsr", "nl.asr.models", {}),
    ("NL Clips AI", "NlClipping", "nl.pipeline.result", {"operation": "clips"}),
    ("NL Sgrossa", "NlSgrossa", "nl.pipeline.result", {"operation": "cleanup"}),
    ("NL Analyze", "NlAnalyze", "nl.longform.get", {}),
    ("NL Transcript", "NlTranscript", "nl.longform.get", {}),
    ("NL Longform", "NlLongform", "nl.longform.get", {}),
    ("NL Search", "NlSearch", "nl.search.index", {}),
    ("NL Shot Map", "NlShots", "sequence.inspect", {}),
    ("NL Social", "NlSocial", "nl.delivery.social", {"platform": "youtube"}),
    ("NL Studio", "NlStudio", "project.inspect", {}),
    ("NL Subtitles", "NlSubtitles", "captions.list", {}),
    ("NL Titles", "NlTitles", "graphics.template.list", {}),
    ("NL Graphics", "NlGraphics", "graphics.template.list", {}),
    ("NL Audio", "NlAudio", "mixer.inspect", {}),
    ("NL Delivery", "NlDelivery", "nl.delivery.qa", {}),
    ("NL Album", "NlAlbum", "nl.album.list", {}),
    ("NL Versions", "NlVersions", "nl.versions.list", {}),
    ("NL Montage", "NlMontage", "nl.pipeline.result", {"operation": "montage"}),
    ("NL Bridge", "NlBridge", "nl.pipeline.result", {"operation": "bridge"}),
    ("NL Story", "NlStory", "nl.story.get", {}),
    ("NL Queue", "NlQueue", "nl.queue.list", {}),
    ("NL Settings", "NlSettings", "nl.asr.models", {}),
    ("NL Cover", "NlCover", "nl.storage.status", {}),
    ("NL Workflows", "NlWorkflows", "nl.workflows.list", {}),
    ("NL Services", "NlServices", "nl.services.status", {}),
    ("NL Speech", "NlSpeech", "nl.asr.models", {}),
]

EXPECTED_ERRORS = (
    "no clips result yet",
    "no cleanup result yet",
    "no montage result yet",
    "no bridge result yet",
    "select a video",
    "select a clip",
    "no active sequence",
    "no transcript",
    "no candidates",
    "run the rough cut analysis first",
    "non disponibile",
)


class Client:
    def __init__(self, port, token):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=10)
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

    def ok(self, method, **params):
        reply = self.call(method, **params)
        if not reply.get("ok"):
            raise RuntimeError("%s %s failed: %s" % (method, params, reply.get("error")))
        return reply.get("result")

    def command(self, command, params=None):
        return self.call("engine.execute", command=command, params=params or {})


def free_port():
    sock = socket.socket()
    try:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]
    finally:
        sock.close()


def wait_for_server(port, timeout=120.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return
        except OSError:
            time.sleep(0.5)
    raise RuntimeError("the app did not open the control port in %s s" % timeout)


def panels_in_layout(node):
    found = set()
    if isinstance(node, dict):
        for value in node.values():
            found |= panels_in_layout(value)
    elif isinstance(node, list):
        for value in node:
            found |= panels_in_layout(value)
    elif isinstance(node, str) and node.startswith("Nl"):
        found.add(node)
    return found


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--project", default=str(DEFAULT_PROJECT))
    parser.add_argument("--out", default=str(ROOT / "documentazione" / "prove" / "p19-nl-audit"))
    parser.add_argument("--no-screenshots", action="store_true")
    args = parser.parse_args()

    exe = Path(args.exe)
    project = Path(args.project)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    shots = out / "panels"
    shots.mkdir(exist_ok=True)
    if not exe.is_file():
        raise SystemExit("exe not found: %s" % exe)
    if not project.is_file():
        raise SystemExit("project not found: %s" % project)

    work = Path(tempfile.mkdtemp(prefix="nl-p19-audit-"))
    data = work / "data"
    copy = work / "project.fcproj"
    data.mkdir()
    shutil.copy2(project, copy)
    port = free_port()
    token = secrets.token_hex(32)
    env = dict(os.environ)
    env["FILMCRAFT_CONTROL_TOKEN"] = token
    env["NL_SUITE_ROOT"] = str(exe.parent)
    process = subprocess.Popen(
        [str(exe), "--control", str(port), "--control-input-only", "--data-dir", str(data), str(copy)],
        env=env,
        cwd=str(exe.parent),
    )
    report = {
        "date": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "exe": str(exe),
        "exe_mtime": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(exe.stat().st_mtime)),
        "project": str(project),
        "panels": {},
        "summary": {},
    }
    try:
        wait_for_server(port)
        client = Client(port, token)
        inspect = client.ok("ui.inspect")
        report["inspect_before"] = inspect
        menu = client.ok("ui.menu.list")
        workspaces = [item["label"] for item in menu if item.get("path") == ["Window", "Workspaces"]]
        report["workspace_menu"] = workspaces
        client.ok("engine.execute", command="window.workspace.nlvideosuite", params={})
        time.sleep(0.5)
        inspect = client.ok("ui.inspect")
        report["workspace_after"] = inspect.get("ui", {}).get("workspace")
        docking = client.ok("ui.docking")
        layout_panels = panels_in_layout(docking.get("layout", {}))
        report["layout_panels"] = sorted(layout_panels)
        missing_layout = [name for name in ("NlHome", "NlAgent") if name not in layout_panels]
        report["missing_layout"] = missing_layout

        # First pass: screenshots and elements only. Commands run in a second pass, so a refusal
        # from one panel cannot leak into the next panel's status line in the screenshots.
        for title, panel_id, command, params in PANELS:
            entry = {"title": title, "panel": panel_id, "command": command}
            try:
                client.ok("ui.panel.show", panel=title)
                time.sleep(0.3)
                elements = client.ok("ui.elements", prefix="nl.")
                rows = elements if isinstance(elements, list) else []
                entry["elements"] = [row.get("id") for row in rows if isinstance(row, dict)]
                entry["context"] = next((row.get("label") for row in rows if isinstance(row, dict) and row.get("id") == "nl.context"), None)
                entry["placeholder"] = any(row.get("id") == "panel.placeholder" for row in rows if isinstance(row, dict))
                if not args.no_screenshots:
                    shot = shots / (panel_id + ".png")
                    entry["screenshot"] = str(shot)
                    entry["screenshot_ok"] = bool(client.call("ui.screenshot", path=str(shot), panel=title).get("ok"))
            except Exception as error:  # one panel never aborts the others
                entry["error"] = str(error)
            report["panels"][title] = entry

        # Second pass: one safe command per panel, recorded as ok or as an expected refusal.
        for title, _panel_id, command, params in PANELS:
            entry = report["panels"][title]
            try:
                reply = client.command(command, params)
                entry["command_ok"] = bool(reply.get("ok"))
                entry["command_error"] = reply.get("error")
                if not reply.get("ok"):
                    text = (reply.get("error") or "").lower()
                    entry["command_expected"] = any(marker in text for marker in EXPECTED_ERRORS)
            except Exception as error:
                entry["command_ok"] = False
                entry["command_error"] = str(error)

        # Home buttons: one per NL panel except Home.
        client.ok("ui.panel.show", panel="NL Home")
        time.sleep(0.3)
        home = client.ok("ui.elements", prefix="nl.nlhome.")
        report["home_buttons"] = sorted(row.get("id") for row in home if isinstance(row, dict))
        client.call("app.quit")
    finally:
        try:
            process.wait(timeout=20)
        except Exception:
            process.terminate()
            try:
                process.wait(timeout=10)
            except Exception:
                process.kill()

    panels = report["panels"]
    report["summary"] = {
        "panels": len(panels),
        "workspace_listed": "NL Video Suite" in report.get("workspace_menu", []),
        "workspace_after": report.get("workspace_after"),
        "missing_layout": report.get("missing_layout", []),
        "missing_context": sorted(t for t, p in panels.items() if not p.get("context")),
        "placeholders": sorted(t for t, p in panels.items() if p.get("placeholder")),
        "panels_without_elements": sorted(t for t, p in panels.items() if not p.get("elements")),
        "commands_failed": sorted(t for t, p in panels.items() if p.get("command_ok") is False and not p.get("command_expected")),
        "commands_expected_refusal": sorted(t for t, p in panels.items() if p.get("command_ok") is False and p.get("command_expected")),
        "home_buttons": len(report.get("home_buttons", [])),
        "screenshots": sum(1 for p in panels.values() if p.get("screenshot_ok")),
    }
    report_path = out / "audit.json"
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(report["summary"], indent=2, ensure_ascii=False))
    print("written: %s" % report_path)
    failures = (
        not report["summary"]["workspace_listed"]
        or report["summary"]["missing_layout"]
        or report["summary"]["missing_context"]
        or report["summary"]["placeholders"]
        or report["summary"]["commands_failed"]
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
