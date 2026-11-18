# -*- coding: utf-8 -*-
"""
SHORTS v1 — "l'idea dell'amico"
================================
Prende un video lungo e tira fuori N shorts verticali pronti da postare:
  1. Trascrizione WhisperX (cache di motore.py)
  2. AI: trova i momenti forti, poi li RAGGRUPPA PER ARGOMENTO
     (ogni argomento = 1 short, minimo N_SHORTS_MIN shorts)
  3. Render 16:9 del montaggio + SRT hormozi
  4. Vertical 9:16 con SFONDO ZOOM LENTO ANIMATO + sottotitoli bruciati dentro

OUTPUT: video_output/[video]_SHORTS/
    SHORT_01_TITOLO.mp4            (16:9 master)
    SHORT_01_TITOLO_FINAL_V.mp4    (9:16 verticale con sub bruciati — pronto da postare)
    SHORT_01_TITOLO_normal.srt / _hormozi.srt
"""

import os
import subprocess

import motore
import taglia_clip_per_editing as taglia

# --- CONFIGURAZIONE ---
VIDEO_INPUT_DIR  = r"K:\AI\ClipsAI\video_da_elaborare"
VIDEO_OUTPUT_DIR = r"K:\AI\ClipsAI\video_output"
LANGUAGE_DEFAULT = "it"

N_SHORTS_MIN = 3          # numero minimo di shorts da tirare fuori
SHORT_MIN_DUR = 20.0      # durata minima di uno short (secondi)
SHORT_MAX_DUR = 60.0      # durata massima di uno short (secondi)
MAX_MOMENTS = 20          # momenti candidati (top-score) su cui ragiona l'AI
SUB_FONT_SIZE = 60        # dimensione sottotitoli bruciati (video 1080x1920)
SUB_MARGIN_V = 320        # altezza sottotitoli dal fondo


class run_with_params:
    """Override parametri GUI + ripristino (come negli altri moduli)."""
    def __init__(self, params: dict | None):
        self.params = dict(params or {})
        self.saved = {}

    def __enter__(self):
        import motore as _m
        g = globals()
        for k in ("N_SHORTS_MIN", "SHORT_MIN_DUR", "SHORT_MAX_DUR",
                  "MAX_MOMENTS", "SUB_FONT_SIZE", "SUB_MARGIN_V"):
            if k in self.params:
                self.saved[k] = g[k]
                g[k] = self.params[k]
        if "OLLAMA_MODEL" in self.params:
            self.saved["_OLLAMA_MODEL"] = _m.OLLAMA_MODEL
            _m.OLLAMA_MODEL = self.params["OLLAMA_MODEL"]
        if "ENABLE_AUDIO_EVENTS" in self.params:
            self.saved["_ENABLE_AUDIO_EVENTS"] = _m.ENABLE_AUDIO_EVENTS
            _m.ENABLE_AUDIO_EVENTS = self.params["ENABLE_AUDIO_EVENTS"]
        return self

    def __exit__(self, *exc):
        import motore as _m
        g = globals()
        for k, v in self.saved.items():
            if k == "_OLLAMA_MODEL":
                _m.OLLAMA_MODEL = v
            elif k == "_ENABLE_AUDIO_EVENTS":
                _m.ENABLE_AUDIO_EVENTS = v
            else:
                g[k] = v
        return False


def _emit(on_progress, phase: str, pct, msg: str = ""):
    if on_progress:
        try:
            on_progress(phase, pct, msg)
        except Exception:
            pass


SHORTS_SCHEMA = {
    "type": "object",
    "properties": {
        "shorts": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "title": {"type": "string",
                              "description": "Titolo accattivante in italiano, max 8 parole"},
                    "topic": {"type": "string",
                              "description": "Argomento dello short in poche parole"},
                    "hook": {"type": "string",
                             "description": "Frase-gancio di apertura"},
                    "moment_numbers": {"type": "array", "items": {"type": "integer"},
                                       "description": "Numeri [M#] in ordine di montaggio"},
                },
                "required": ["title", "topic", "moment_numbers"],
            },
        },
    },
    "required": ["shorts"],
}


def ai_group_shorts(moments: list, n_min: int) -> list:
    """Raggruppa i momenti [M#] per argomento: ogni argomento = 1 short."""
    lines = []
    for i, m in enumerate(moments, 1):
        lines.append(
            f"[M{i}] TITOLO: {m['title']} | ARGOMENTO: {m.get('topic', '')} | "
            f"DURATA: {m['end']-m['start']:.0f}s | SCORE: {m['score']}/10")
    prompt = f"""Sei un montatore di shorts virali per Nicola Lanci (doppiatore).
Hai questa lista di momenti (numeri [M#]):

{chr(10).join(lines)}

Raggruppali per ARGOMENTO: ogni argomento diventa UNO SHORT.
VINCOLI:
- Almeno {n_min} shorts (se i momenti non bastano, fanne comunque il massimo possibile).
- Durata di ogni short: {SHORT_MIN_DUR:.0f}-{SHORT_MAX_DUR:.0f}s (somma le durate).
- Momenti dello stesso short = stesso argomento (o contrasto voluto).
- Ordine: gancio forte all'inizio, punchline alla fine.
- Non riusare lo stesso momento in shorts diversi.
"""
    try:
        result = motore.ollama_chat(prompt, schema=SHORTS_SCHEMA)
    except Exception as e:
        motore.log(f"   ❌ Raggruppamento shorts fallito: {e}")
        return []

    shorts = []
    for s in result.get("shorts", []):
        idxs = []
        for i in s.get("moment_numbers", []):
            try:
                i = int(i)
            except (TypeError, ValueError):
                continue
            if 1 <= i <= len(moments) and i not in idxs:
                idxs.append(i)
        if not idxs:
            continue
        total = sum(moments[i - 1]["end"] - moments[i - 1]["start"] for i in idxs)
        if not (SHORT_MIN_DUR <= total <= SHORT_MAX_DUR):
            motore.log(f"      ⚠️  Short '{s.get('title', '')}' scartato: {total:.0f}s fuori range")
            continue
        shorts.append({
            "title": s.get("title", f"Short {len(shorts) + 1}"),
            "topic": s.get("topic", ""),
            "hook": s.get("hook", ""),
            "indices": idxs,
            "total_dur": total,
        })
    return shorts


def propose_shorts(input_path: str, base_name: str, work_dir: str,
                   lingua: str = LANGUAGE_DEFAULT,
                   params: dict | None = None, on_progress=None) -> dict:
    """FASE 1 (GUI): trascrizione + momenti + raggruppamento, SENZA render."""
    with run_with_params(params):
        os.makedirs(work_dir, exist_ok=True)
        backup_dir = motore.backup_output_media(work_dir)
        if backup_dir:
            motore.log(f"   🛡️ Output precedenti salvati in: {os.path.basename(backup_dir)}")

        _emit(on_progress, "transcribe", 5, "Trascrizione WhisperX...")
        data = motore.transcribe(input_path, work_dir, lingua)
        words = data["words"]
        _emit(on_progress, "transcribe", 25, f"Trascritte {len(words)} parole")

        segments = motore.group_into_segments(words)
        ast_ann = motore.events_to_annotations(data.get("events") or [], segments)
        motore.log(f"\n🧠 FASE 1: ricerca momenti...")
        moments = []
        n_chunks = (len(segments) - 1) // taglia.AI_CHUNK_SEGS + 1
        for ci in range(0, len(segments), taglia.AI_CHUNK_SEGS):
            chunk = segments[ci:ci + taglia.AI_CHUNK_SEGS]
            num = ci // taglia.AI_CHUNK_SEGS + 1
            motore.log(f"   > Blocco {num}/{n_chunks}...")
            _emit(on_progress, "moments", 25 + 40 * num / max(n_chunks, 1),
                  f"Momenti blocco {num}/{n_chunks}...")
            moments.extend(taglia.ai_find_moments(chunk, ast_ann))

        moments = taglia.dedupe_by_overlap(moments)
        moments = sorted(moments, key=lambda m: -m["score"])[:MAX_MOMENTS]
        motore.log(f"   ✅ {len(moments)} momenti candidati")

        _emit(on_progress, "group", 70, "Raggruppamento per argomento...")
        motore.log(f"\n🗂️  FASE 2: raggruppamento per argomento (min {N_SHORTS_MIN} shorts)...")
        shorts = ai_group_shorts(moments, N_SHORTS_MIN)
        motore.log(f"   ✅ {len(shorts)} shorts pianificati")
        _emit(on_progress, "done", 80, f"{len(shorts)} shorts pianificati")

        out = []
        for s in shorts:
            out.append({
                "title": s["title"], "topic": s.get("topic", ""),
                "hook": s.get("hook", ""),
                "total_dur": round(s["total_dur"], 1),
                "segments": [[round(moments[i - 1]["start"], 3),
                              round(moments[i - 1]["end"], 3)] for i in s["indices"]],
            })
        return {"shorts": out,
                "moments": [{"title": m["title"], "topic": m.get("topic", ""),
                             "start": round(m["start"], 3), "end": round(m["end"], 3),
                             "score": m["score"]} for m in moments],
                "duration": round(data["duration"], 3),
                "words_count": len(words),
                "input_path": input_path, "work_dir": work_dir, "lingua": lingua}


def _zoom_bg_filter() -> str:
    """Filtro 9:16: sfondo blur con ZOOM LENTO animato (eval=frame, ~1px/s)."""
    w, h = taglia.VERT_W, taglia.VERT_H
    return (
        f"split=2[bg][fg];"
        f"[bg]scale={w*2}:{h*2}:force_original_aspect_ratio=increase,"
        f"crop={w*2}:{h*2},boxblur=25:3,"
        f"scale=w='{w}+t*1.5':h=-2:eval=frame,crop={w}:{h}[bg2];"
        f"[fg]scale={w}:{h}:force_original_aspect_ratio=decrease[fg2];"
        f"[bg2][fg2]overlay=(W-w)/2:(H-h)/2"
    )


def _ass_time(sec: float) -> str:
    sec = max(0.0, sec)
    h = int(sec // 3600)
    m = int((sec % 3600) // 60)
    s = sec % 60
    return f"{h}:{m:02d}:{s:05.2f}"


def write_ass(cues: list, path: str, width: int = 1080, height: int = 1920):
    """Scrive .ass con PlayRes esplicito (l'SRT non ce l'ha e libass
    sbaglia scala/posizione su video verticali)."""
    with open(path, "w", encoding="utf-8") as f:
        f.write("[Script Info]\nScriptType: v4.00+\n"
                f"PlayResX: {width}\nPlayResY: {height}\n"
                "ScaledBorderAndShadow: yes\n\n"
                "[V4+ Styles]\n"
                "Format: Name, Fontname, Fontsize, PrimaryColour, OutlineColour, "
                "BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
                f"Style: Default,Arial Black,{SUB_FONT_SIZE},&H0000FFFF,&H00000000,"
                f"1,3,0,2,30,30,{SUB_MARGIN_V},1\n\n"
                "[Events]\n"
                "Format: Layer, Start, End, Style, Text\n")
        for c in cues:
            txt = str(c.get("text", "")).replace("\n", " ")
            f.write(f"Dialogue: 0,{_ass_time(c['start'])},{_ass_time(c['end'])},"
                    f"Default,{txt}\n")


def render_vertical_only(master_path: str, out_path: str) -> bool:
    """Solo conversione 9:16 con zoom-bg, SENZA sottotitoli
    (per chi reframa/controlla in Premiere)."""
    for vf in (_zoom_bg_filter(), taglia.VF_916):
        cmd = [motore.FFMPEG_EXE, "-y", "-i", master_path,
               "-vf", vf,
               "-c:v", "libx264", "-preset", "superfast", "-crf", "20",
               "-c:a", "aac", "-b:a", "192k", out_path]
        res = subprocess.run(cmd, capture_output=True)
        if res.returncode == 0 and os.path.isfile(out_path) and os.path.getsize(out_path) > 0:
            return True
    return False


def render_short_final(master_path: str, ass_path: str, out_path: str) -> bool:
    """Passata finale: zoom-bg animato + sottotitoli bruciati via .ass.
    Fallback a blur statico."""
    ass_esc = ass_path.replace("\\", "/").replace(":", "\\:")
    for vf in (_zoom_bg_filter() + f",ass='{ass_esc}'",
               taglia.VF_916 + f",ass='{ass_esc}'"):
        cmd = [motore.FFMPEG_EXE, "-y", "-i", master_path,
               "-vf", vf,
               "-c:v", "libx264", "-preset", "superfast", "-crf", "20",
               "-c:a", "aac", "-b:a", "192k", out_path]
        res = subprocess.run(cmd, capture_output=True)
        if res.returncode == 0 and os.path.isfile(out_path) and os.path.getsize(out_path) > 0:
            return True
        motore.log(f"      ⚠️  Filtro vertical fallito, provo fallback...")
    return False


def render_short_files(input_path: str, shorts: list, work_dir: str,
                       lingua: str = LANGUAGE_DEFAULT,
                       params: dict | None = None, on_progress=None,
                       vertical: bool = True, burn_subs: bool = False) -> list:
    """FASE 2 (GUI): monta SOLO gli shorts selezionati. Ritorna i file creati.

    vertical=False  -> solo master (proporzione nativa) + sidecar, niente 9:16.
    burn_subs=False -> sottotitoli SOLO come file (.srt + .ass) da editare
                       in Premiere; True li brucia anche nel verticale.
    """
    with run_with_params(params):
        words = motore.transcribe(input_path, work_dir, lingua)["words"]
        created = []
        total = len(shorts)
        for idx, s in enumerate(shorts, 1):
            _emit(on_progress, "render", 100 * (idx - 1) / max(total, 1),
                  f"Short {idx}/{total}...")
            base = f"SHORT_{idx:02d}_{taglia.safe_filename(s.get('title', 'short'))}"
            master = os.path.join(work_dir, f"{base}.mp4")
            final = os.path.join(work_dir, f"{base}_FINAL_V.mp4")
            clip_times = [(float(a), float(b) - float(a)) for a, b in s["segments"]]
            motore.log(f"\n   🎬 Short {idx}: '{s.get('title', '')}' "
                       f"({s.get('topic', '')} — {len(clip_times)} segmenti)")
            if taglia.render_reel(input_path, clip_times, master, None) is None:
                motore.log("      ❌ Montaggio master fallito.")
                continue
            created.append(master)
            n_cues, n_h = taglia.save_reel_srt(words, clip_times, work_dir, base)
            motore.log(f"      📝 SRT: {n_cues} cue normal + {n_h} cue hormozi")
            # cue hormozi relativi per il sidecar/burn .ass (stessa matematica di save_reel_srt)
            cues_h = []
            off = 0.0
            for st, dur in clip_times:
                for c in motore.words_to_cues(words, st, st + dur, max_words=2):
                    cues_h.append({"start": c["start"] - st + off,
                                   "end": c["end"] - st + off,
                                   "text": str(c["text"]).upper()})
                off += dur
            ass_path = os.path.join(work_dir, f"{base}.ass")
            write_ass(cues_h, ass_path)
            motore.log(f"      📝 ASS style per Premiere: {os.path.basename(ass_path)}")
            if not vertical:
                continue
            if not burn_subs:
                if render_vertical_only(master, final):
                    motore.log(f"      📱 Verticale zoom-bg (no sub bruciati): {os.path.basename(final)}")
                    created.append(final)
                else:
                    motore.log("      ❌ Vertical finale fallito.")
                continue
            if render_short_final(master, ass_path, final):
                motore.log(f"      📱 Verticale zoom-bg + sub bruciati: {os.path.basename(final)}")
                created.append(final)
            else:
                motore.log("      ❌ Vertical finale fallito.")
        _emit(on_progress, "done", 100, f"{len(created)} file creati")
        return created
