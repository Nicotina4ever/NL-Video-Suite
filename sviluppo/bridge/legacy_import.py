"""Read-only legacy project extraction. No application servers are started."""
import json
from pathlib import Path
import sqlite3
import sys


def extract(request):
    system = request["system"]
    source = Path(request["path"]).resolve(strict=True)
    bundle = {"story": {}, "clips": [], "references": [], "warnings": [], "raw": {}}
    if system == "regista":
        root = source if source.is_dir() else source.parent
        config_path = root / "config.json"
        config = json.loads(config_path.read_text(encoding="utf-8")) if config_path.exists() else {}
        value = json.loads((root / "timeline_engine.json").read_text(encoding="utf-8"))
        bundle["raw"] = {"config": config, "timeline": value}
        bundle["story"] = {"premise": config.get("topic") or config.get("description") or "", "scenes": (config.get("storyboard") or {}).get("scenes", [])}
        tracks = {t["id"]: t for t in value.get("tracks", [])}
        for clip in value.get("clips", []):
            track = tracks.get(clip["track_id"], {})
            if not track.get("visible", True):
                continue
            filename = clip.get("asset_path")
            if not filename:
                bundle["warnings"].append("Clip senza asset: " + str(clip.get("id")))
                continue
            path = Path(filename)
            if not path.is_absolute():
                candidates = [root / filename, root / "input" / filename]
                path = next((p for p in candidates if p.is_file()), candidates[0])
            params = clip.get("params") or {}
            if params.get("overlay_scale_pct", 100) != 100 or any(params.get(k) for k in ("overlay_x_pct", "overlay_y_pct", "fade_in_sec", "fade_out_sec")) or clip.get("speed", 1) != 1:
                raise ValueError("Clip con trasformazioni non ancora migrabili: " + str(clip.get("id")) + ". Esporta prima il media da Regista per conservarne l'aspetto.")
            bundle["clips"].append({"path": str(path), "start": clip.get("start", 0),
                "trim": clip.get("trim_in") or 0, "duration": clip.get("duration") or clip["end"] - clip.get("start", 0),
                "track": track.get("type", "video"), "volume": clip.get("volume") if clip.get("volume") is not None else 1,
                "opacity": clip.get("opacity") if clip.get("opacity") is not None else 1, "label": clip.get("label") or clip.get("prompt", "")})
        return bundle
    if system == "calliope":
        # Rust canonical paths on Windows use \\?\ prefixes. SQLite's URI
        # parser treats that prefix as an authority, so normalise it here.
        sqlite_path = str(source)
        if sqlite_path.startswith("\\\\?\\UNC\\"):
            sqlite_path = "\\\\" + sqlite_path[8:]
        elif sqlite_path.startswith("\\\\?\\"):
            sqlite_path = sqlite_path[4:]
        connection = sqlite3.connect(Path(sqlite_path).as_uri() + "?mode=ro", uri=True)
        connection.row_factory = sqlite3.Row
        project_id = int(request["source_project_id"])
        with connection:
            connection.execute("BEGIN")
            def rows(table):
                return [dict(r) for r in connection.execute("SELECT * FROM " + table + " WHERE project_id=?", (project_id,))]
            project = connection.execute("SELECT * FROM projects WHERE id=?", (project_id,)).fetchone()
            if project is None:
                raise ValueError("Progetto Calliope inesistente")
            bundle["raw"] = {"project": dict(project), **{t: rows(t) for t in ("story_beats", "characters", "locations", "items", "scenes", "clips")}}
        connection.close()
        raw = bundle["raw"]
        bundle["story"] = {"premise": raw["project"].get("idea") or "", "scenes": sorted(raw["scenes"], key=lambda s:s["order_index"])}
        for kind, path_keys in (("characters", ("portrait_path", "sheet_path")), ("locations", ("reference_image_path",))):
            bundle["story"][kind] = [{"name": e["name"], "description": e.get("description") or e.get("appearance") or ""} for e in raw[kind]]
            for entity in raw[kind]:
                for key in path_keys:
                    if entity.get(key):
                        bundle["references"].append(entity[key])
        scene_order = {s["id"]: s["order_index"] for s in raw["scenes"]}
        start = 0
        for clip in sorted(raw["clips"], key=lambda c:(scene_order.get(c["scene_id"],0), c["order_index"])):
            if not clip.get("clip_path"):
                bundle["warnings"].append("Shot non ancora generato: " + str(clip["id"]))
                continue
            path = Path(clip["clip_path"])
            if not path.is_absolute():
                path = source.parent / path
            duration = clip.get("duration_sec") or 5
            bundle["clips"].append({"path": str(path), "start": start, "trim": 0, "duration": duration, "track": "video", "volume": 1, "opacity": 1, "label": clip.get("description") or "Shot"})
            start += duration
        return bundle
    if system == "zp_shorts":
        # Load the isolated canonical parser, not the legacy application's
        # src.models package (its __init__ also imports unrelated models).
        import importlib.util
        parser = Path(request["source_root"]) / "short.py"
        spec = importlib.util.spec_from_file_location("nl_suite_zp_short", parser)
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        ShortCandidate = module.ShortCandidate
        value = json.loads(source.read_text(encoding="utf-8"))
        candidates = value if isinstance(value, list) else value.get("candidates", [])
        bundle["raw"] = value
        start = 0
        for row in candidates:
            candidate = ShortCandidate.from_dict(row)
            if not candidate.approved:
                continue
            for lo, hi in candidate.derive_keep_segments():
                bundle["clips"].append({"path": request["media_path"], "start": start, "trim": lo, "duration": hi-lo, "track": "video", "volume": 1, "opacity": 1, "label": candidate.title})
                start += hi-lo
        if not bundle["clips"]:
            raise ValueError("Nessun candidato approvato nel file ZP")
        return bundle
    raise ValueError("Sistema non supportato")


if __name__ == "__main__":
    request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    Path(sys.argv[2]).write_text(json.dumps(extract(request), ensure_ascii=False), encoding="utf-8")
