"""Real selective forced alignment using the user's existing Qwen GGUF runner.

Only the edited cue audio is decoded. Raw ASR is never rewritten. An aligner
matches the supplied text; it does NOT prove that a rewritten word was spoken.
"""
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys
import time

VERSION = "selective-qwen-v1"


def identity(path, directory):
    path = Path(path)
    stat = path.stat()
    record = directory / (hashlib.sha256(str(path).encode()).hexdigest() + ".json")
    stamp = {"path": str(path), "bytes": stat.st_size, "mtime_ns": stat.st_mtime_ns}
    if record.is_file():
        old = json.loads(record.read_text(encoding="utf-8"))
        if all(old.get(k) == v for k, v in stamp.items()):
            return old
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b""):
            digest.update(block)
    stamp["sha256"] = digest.hexdigest()
    record.write_text(json.dumps(stamp), encoding="utf-8")
    return stamp


def run(request):
    config, unit = request["config"], request["unit"]
    work = Path(request["work_dir"])
    model = Path(config["aligner"]).resolve(strict=True)
    runner = Path(config["runner"]).resolve(strict=True)
    cache = Path(request["cache_dir"]); cache.mkdir(parents=True, exist_ok=True)
    identities = cache / "identities"; identities.mkdir(exist_ok=True)
    models = {"model": identity(model, identities), "runner": identity(runner, identities)}
    source_stat = Path(request["source"]).stat()
    from_ms = max(0, unit["source"]["start_ms"] - 250)
    to_ms = min(request["duration_ms"], unit["source"]["end_ms"] + 250)
    reference = " ".join(unit["text"].split())
    if not reference or len(reference) > 4000 or not 0 < to_ms - from_ms <= 60500:
        raise ValueError("Cue non valido per l'allineamento selettivo")
    key = hashlib.sha256(json.dumps({"version": VERSION, "asset": unit["asset_id"],
        "text": reference, "from": from_ms, "to": to_ms, "language": unit["language"],
        "models": models,"threads":config["threads"],"source_bytes":source_stat.st_size,"source_mtime":source_stat.st_mtime_ns}, sort_keys=True).encode()).hexdigest()
    ready = cache / (key + ".json")
    if ready.is_file():
        result = json.loads(ready.read_text(encoding="utf-8")); result["cache_hit"] = True
        return result
    audio, raw = work / "cue.wav", work / "aligned.json"
    hidden = {"creationflags": 0x08000000} if sys.platform == "win32" else {}
    subprocess.run([request["ffmpeg"], "-nostdin", "-v", "error", "-n", "-ss", str(from_ms / 1000),
                    "-i", request["source"], "-t", str((to_ms - from_ms) / 1000), "-vn", "-ac", "1", "-ar", "16000", str(audio)],
                   check=True, capture_output=True, timeout=90, **hidden)
    started = time.monotonic()
    command = [str(runner), "-m", str(model), "-f", str(audio), "--align", "--text", reference,
               "-l", {"it": "Italian", "en": "English", "es": "Spanish"}.get(unit["language"], unit["language"]),
               "-t", str(config["threads"]), "-o", str(raw)]
    with (work / "runner.log").open("wb") as log:
        subprocess.run(command, cwd=runner.parent, stdout=log, stderr=subprocess.STDOUT, timeout=240, check=True, **hidden)
    payload = json.loads(raw.read_text(encoding="utf-8"))
    words = []
    for word in payload.get("words", []):
        a, b, text = word.get("start"), word.get("end"), str(word.get("word", "")).strip()
        if not isinstance(a, (float, int)) or not isinstance(b, (float, int)) or not math.isfinite(a) or not math.isfinite(b) or not text or a < 0 or b <= a or b * 1000 > to_ms - from_ms + 100:
            raise ValueError("Il runner ha prodotto timing non validi: nessuna modifica applicata")
        start, end = from_ms + round(a * 1000), min(to_ms, from_ms + round(b * 1000))
        if words and start < words[-1]["start_ms"]:
            raise ValueError("Il runner ha prodotto parole non ordinate")
        words.append({"text": text, "start_ms": start, "end_ms": end})
    normalized = lambda text: "".join(c.lower() for c in text if c.isalnum())
    if not words or normalized(" ".join(w["text"] for w in words)) != normalized(reference):
        raise ValueError("Il testo allineato non coincide con il riferimento richiesto")
    result = {"words": words, "cache_hit": False, "metadata": {"engine": "qwen_forced_aligner_cpp",
              "version": VERSION, "models":models,
              "language": unit["language"], "device": "cpu", "elapsed_seconds": round(time.monotonic() - started, 3),
              "source_range": {"start_ms": from_ms, "end_ms": to_ms}, "only_this_cue": True,
              "semantic_match_proven": False, "warning": "L'allineamento forza il testo fornito: verifica che le parole siano effettivamente pronunciate."}}
    ready.write_text(json.dumps(result, ensure_ascii=False), encoding="utf-8")
    audio.unlink()
    return result


if __name__ == "__main__":
    request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    result = run(request)
    Path(sys.argv[2]).write_text(json.dumps(result, ensure_ascii=False), encoding="utf-8")
