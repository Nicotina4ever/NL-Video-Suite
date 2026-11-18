# -*- coding: utf-8 -*-
"""
TAGLIA CLIP v3 — Clipping Social professionale
==============================================
Modalità:
  1) IG STORIES      — clip singola <= 25s
  2) SOCIAL / REELS  — clip singola 30-60s
  3) REEL NARRATIVO  — montaggio multi-segmento 40-75s (2 passaggi AI)

Miglioramenti rispetto alla v2:
  * Trascrizione WhisperX (large-v3 GPU + allineamento parola <100ms) con cache
  * L'AI risponde con INDICI DI FRASE [S#] + structured outputs JSON (Ollama)
    -> zero matching fuzzy testo->timestamp, zero clip duplicate, zero JSON corrotti
  * Ogni clip ha un punteggio 1-10: si tengono le migliori, senza duplicati
  * Sottotitoli SRT (normale + hormozi) generati dai timestamp parola allineati
  * Export opzionale 9:16 verticale (sfondo sfocato) pronto per Stories/Reels

OUTPUT: video_output/[video]/
    NN_TITOLO.mp4            (16:9 master)
    NN_TITOLO_V.mp4          (9:16 verticale, se richiesto)
    NN_TITOLO_normal.srt
    NN_TITOLO_hormozi.srt
"""

import os
import re
import json
import subprocess
import time

import motore

# --- CONFIGURAZIONE ---
VIDEO_INPUT_DIR  = r"K:\AI\ClipsAI\video_da_elaborare"
VIDEO_OUTPUT_DIR = r"K:\AI\ClipsAI\video_output"
LANGUAGE_DEFAULT = "it"

# Vincoli piattaforma
MODE_RULES = {
    "1": {"name": "IG STORIES",      "max_dur": 25.0, "min_dur": 4.0,  "max_clips": 8},
    "2": {"name": "SOCIAL / REELS",  "max_dur": 75.0, "min_dur": 20.0, "max_clips": 6},
    "3": {"name": "REEL NARRATIVO", "max_dur": 75.0, "min_dur": 40.0, "max_clips": 4},
}

AI_CHUNK_SEGS = 45        # frasi numerate per chiamata AI
DEDUPE_OVERLAP = 0.50     # scarta clip che si sovrappongono >50% con una già tenuta

# --- PARAMETRI GUI (override via run_with_params) ---
# Chiavi: AI_CHUNK_SEGS, DEDUPE_OVERLAP, OLLAMA_MODEL,
#         max_clips, min_dur, max_dur (applicati alla modalità in uso),
#         ENABLE_AUDIO_EVENTS (inoltrato a motore.py)


class run_with_params:
    """Context manager: override parametri GUI + ripristino.

    Se è passato `mode` e chiavi max_clips/min_dur/max_dur, patcha solo
    la regola MODE_RULES di quella modalità. Imposta anche
    motore.OLLAMA_MODEL se passato. Sicuro: il server esegue un job alla volta.
    """
    def __init__(self, params: dict | None, mode: str | None = None):
        self.params = dict(params or {})
        self.mode = mode
        self.saved = {}

    def __enter__(self):
        import motore as _m
        g = globals()
        for k in ("AI_CHUNK_SEGS", "DEDUPE_OVERLAP"):
            if k in self.params:
                self.saved[k] = g[k]
                g[k] = self.params[k]
        if self.mode and self.mode in MODE_RULES:
            for k in ("max_clips", "min_dur", "max_dur"):
                if k in self.params:
                    self.saved[f"RULE:{k}"] = MODE_RULES[self.mode][k]
                    MODE_RULES[self.mode][k] = self.params[k]
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
            elif k.startswith("RULE:") and self.mode in MODE_RULES:
                MODE_RULES[self.mode][k[5:]] = v
            else:
                g[k] = v
        return False


def _emit(on_progress, phase: str, pct, msg: str = ""):
    if on_progress:
        try:
            on_progress(phase, pct, msg)
        except Exception:
            pass

# 9:16
VERT_W, VERT_H = 1080, 1920
VF_916 = (
    f"split=2[bg][fg];"
    f"[bg]scale={VERT_W}:{VERT_H}:force_original_aspect_ratio=increase,crop={VERT_W}:{VERT_H},boxblur=25:3[bg2];"
    f"[fg]scale={VERT_W}:{VERT_H}:force_original_aspect_ratio=decrease[fg2];"
    f"[bg2][fg2]overlay=(W-w)/2:(H-h)/2"
)


# ═══════════════════════════════════════════════════════════════
# UTILITY
# ═══════════════════════════════════════════════════════════════

def safe_filename(name: str, maxlen: int = 35) -> str:
    name = re.sub(r'\W+', '_', name).strip('_')
    return name[:maxlen] or "clip"


def map_chunk_indices(chunk: list, local_idx: int):
    """Indice locale (1-based dentro il chunk) -> segmento globale."""
    if 1 <= local_idx <= len(chunk):
        return chunk[local_idx - 1]
    return None


def clip_bounds(chunk: list, start_local: int, end_local: int):
    s = map_chunk_indices(chunk, start_local)
    e = map_chunk_indices(chunk, end_local)
    if s is None or e is None:
        return None
    if e["end"] <= s["start"]:
        s, e = e, s
    return s["start"], e["end"]


def dedupe_by_overlap(clips: list) -> list:
    """Ordina per score decrescente e scarta le clip troppo sovrapposte."""
    clips = sorted(clips, key=lambda c: -c.get("score", 0))
    kept = []
    for c in clips:
        s, e = c["start"], c["end"]
        if e <= s:
            continue
        clash = False
        for k in kept:
            inter = max(0.0, min(e, k["end"]) - max(s, k["start"]))
            overlap = inter / max(0.001, min(e - s, k["end"] - k["start"]))
            if overlap > DEDUPE_OVERLAP:
                clash = True
                break
        if not clash:
            kept.append(c)
    kept.sort(key=lambda c: c["start"])
    return kept


# ═══════════════════════════════════════════════════════════════
# AI — CLIP SINGOLE (modalità 1 e 2)
# ═══════════════════════════════════════════════════════════════

def ai_find_clips(chunk: list, mode: str, chunk_ann: dict | None = None) -> list:
    """
    Chiede all'AI di trovare 1-3 clip nel chunk.
    I numeri [S#] sono LOCALI al chunk (da 1 a len(chunk)).
    Ritorna lista di dict con start/end già in secondi GLOBALI.
    """
    rule = MODE_RULES[mode]
    local_text = motore.render_numbered(chunk, chunk_ann)
    max_w = int(rule["max_dur"])

    prompt = f"""Sei un video editor professionista specializzato in contenuti social virali per Nicola Lanci (doppiatore).
Dal blocco di trascrizione numerato qui sotto, scegli 1-3 momenti da tagliare come clip AUTONOMA per {rule['name']}.

MARCATORI (se presenti dopo [S#]): (FIATO)=fiato/respiro, (RISATA), (MUSICA), (SILENZIO), (APPLAUSI), (EFFETTI) — eventi audio rilevati automaticamente (indizio, non verità).

VINCOLI:
- Durata clip: da {int(rule['min_dur'])} a {max_w} secondi (calcola con i numeri [S#]: la clip va da start_seg a end_seg INCLUSI).
- INIZIO: deve aprire con un gancio forte (domanda, battuta, concetto sorprendente) — niente "ciao, oggi parliamo di...".
- FINE: deve chiudere il senso (punchline, conclusione, domanda aperta), senza tagliare a metà frase.
- Qualità: battute, rivelazioni, storytelling, concetti forti. Evita parti piatte.
- Punteggio score (1-10): quanto è virale/autorevole questo momento. Sii critico: 10 = oro puro.

NUMERI DA USARE nella risposta: start_seg ed end_seg DEVONO essere numeri [S#] presenti nel blocco (1..{len(chunk)}).

TRASCRIIZIONE BLOCCO ({len(chunk)} frasi):
{local_text}
"""
    try:
        result = motore.ollama_chat(prompt, schema=motore.CLIP_SCHEMA)
    except Exception as e:
        motore.log(f"      ⚠️  {e}")
        return []

    clips = []
    for c in result.get("clips", []):
        try:
            sl, el = int(c["start_seg"]), int(c["end_seg"])
        except (KeyError, TypeError, ValueError):
            continue
        bounds = clip_bounds(chunk, sl, el)
        if bounds is None:
            continue
        st, et = bounds
        if not (rule["min_dur"] <= et - st <= rule["max_dur"]):
            continue
        clips.append({
            "title": c.get("title", "Clip"),
            "start": st, "end": et,
            "score": max(1, min(10, int(c.get("score", 5)))),
            "reason": c.get("reason", ""),
        })
    return clips


# ═══════════════════════════════════════════════════════════════
# AI — MOMENTI (modalità 3, fase 1)
# ═══════════════════════════════════════════════════════════════

def ai_find_moments(chunk: list, chunk_ann: dict | None = None) -> list:
    local_text = motore.render_numbered(chunk, chunk_ann)
    prompt = f"""Sei un video editor professionista. Dal blocco di trascrizione numerato, individua 1-3 MOMENTI ad alto valore (battute, rivelazioni, storytelling, concetti forti) adatti a un Reel narrativo.

MARCATORI (se presenti dopo [S#]): (FIATO)=fiato/respiro, (RISATA), (MUSICA), (SILENZIO), (APPLAUSI), (EFFETTI) — eventi audio rilevati automaticamente (indizio, non verità).

VINCOLI:
- Ogni momento: 8-40 secondi (usa i numeri [S#] per calcolarlo: start_seg..end_seg INCLUSI).
- Ogni momento deve avere senso compiuto e una chiusura forte.
- Punteggio score 1-10: quanto è forte l'impatto. Sii critico.

NUMERI DA USARE: start_seg ed end_seg DEVONO essere numeri [S#] presenti nel blocco (1..{len(chunk)}).

TRASCRIIZIONE BLOCCO ({len(chunk)} frasi):
{local_text}
"""
    try:
        result = motore.ollama_chat(prompt, schema=motore.MOMENTS_SCHEMA)
    except Exception as e:
        motore.log(f"      ⚠️  {e}")
        return []

    moments = []
    for m in result.get("moments", []):
        try:
            sl, el = int(m["start_seg"]), int(m["end_seg"])
        except (KeyError, TypeError, ValueError):
            continue
        bounds = clip_bounds(chunk, sl, el)
        if bounds is None:
            continue
        st, et = bounds
        if not (8.0 <= (et - st) <= 40.0):
            continue
        moments.append({
            "title": m.get("title", "Momento"),
            "topic": m.get("topic", ""),
            "start": st, "end": et,
            "score": max(1, min(10, int(m.get("score", 5)))),
        })
    return moments


# ═══════════════════════════════════════════════════════════════
# AI — COMPOSIZIONE REEL (modalità 3, fase 2)
# ═══════════════════════════════════════════════════════════════

def ai_compose_reels(moments: list) -> list:
    """Componi i momenti (già numerati [M#]) in 2-4 reel narrativi 40-75s."""
    lines = []
    for i, m in enumerate(moments, 1):
        lines.append(
            f"[M{i}] TITOLO: {m['title']} | ARGOMENTO: {m.get('topic','')} | "
            f"DURATA: {m['end']-m['start']:.0f}s | SCORE: {m['score']}/10")
    moments_text = "\n".join(lines)

    prompt = f"""Sei un montatore professionista di reel virali. Hai questa lista di momenti disponibili (numeri [M#]):

{moments_text}

Componi 2-4 REEL NARRATIVI.

VINCOLI DI MONTAGGIO:
- Durata totale di ogni reel: 40-75 secondi (somma le durate dei momenti scelti).
- Coerenza: i momenti dello stesso reel devono stare in tema (stesso argomento) o creare un contrasto efficace.
- Ordine: momento con gancio forte all'inizio, chiusura con punchline.
- Non riusare lo stesso momento in reel diversi.
- moment_numbers: elenco dei numeri [M#] in ordine di montaggio.

Se i momenti disponibili non bastano per 2 reel validi, ne componi uno solo di qualità.
"""
    try:
        result = motore.ollama_chat(prompt, schema=motore.REELS_SCHEMA)
    except Exception as e:
        motore.log(f"   ❌ Composizione reel fallita: {e}")
        return []

    reels = []
    for r in result.get("reels", []):
        idxs = []
        for i in r.get("moment_numbers", []):
            try:
                i = int(i)
            except (TypeError, ValueError):
                continue
            if 1 <= i <= len(moments) and i not in idxs:
                idxs.append(i)
        if not idxs:
            continue
        total = sum(moments[i-1]["end"] - moments[i-1]["start"] for i in idxs)
        if not (40.0 <= total <= 75.0):
            motore.log(f"      ⚠️  Reel '{r.get('title','')}' scartato: durata {total:.0f}s fuori range 40-75s")
            continue
        reels.append({
            "title": r.get("title", f"Reel {len(reels)+1}"),
            "hook": r.get("hook", ""),
            "indices": idxs,
            "total_dur": total,
        })
    return reels


# ═══════════════════════════════════════════════════════════════
# RENDERING
# ═══════════════════════════════════════════════════════════════

def render_clip(input_path: str, start: float, end: float,
                out_169: str, out_916: str | None, verbose: bool = True):
    """Taglia un clip 16:9 (e opzionalmente 9:16 con sfondo sfocato)."""
    dur = round(end - start, 3)

    def run(cmd, tag):
        res = subprocess.run(cmd, capture_output=True)
        if res.returncode != 0:
            err = (res.stderr or b"").decode(errors="ignore")[-200:]
            motore.log(f"   ❌ FFmpeg {tag}: {err}")
            return False
        if verbose:
            pass
        return True

    cmd16 = [
        motore.FFMPEG_EXE, "-y",
        "-ss", str(round(start, 3)), "-t", str(dur), "-i", input_path,
        "-map", "0",
        "-c:v", "libx264", "-preset", "superfast", "-crf", "19",
        "-c:a", "aac", "-b:a", "192k",
        out_169,
    ]
    if not run(cmd16, "16:9"):
        return False

    if out_916:
        cmd9 = [
            motore.FFMPEG_EXE, "-y",
            "-ss", str(round(start, 3)), "-t", str(dur), "-i", input_path,
            "-vf", VF_916,
            "-map", "0:v", "-map", "0:a?",
            "-c:v", "libx264", "-preset", "superfast", "-crf", "19",
            "-c:a", "aac", "-b:a", "192k",
            out_916,
        ]
        if not run(cmd9, "9:16"):
            try:
                os.remove(out_916)
            except OSError:
                pass
            return False

    return True


def render_reel(input_path: str, clip_times: list,
                out_169: str, out_916: str | None):
    """Taglia + concatena i segmenti di un reel."""
    work_tmp = os.path.dirname(out_169)
    temp_files = []
    total = 0.0
    ok = True

    for i, (start, dur) in enumerate(clip_times):
        tf = os.path.join(work_tmp, f"_tmp_seg_{i:02d}.mp4")
        cmd = [
            motore.FFMPEG_EXE, "-y",
            "-ss", str(round(start, 3)), "-t", str(round(dur, 3)), "-i", input_path,
            "-map", "0",
            "-c:v", "libx264", "-preset", "superfast", "-crf", "19",
            "-c:a", "aac", "-b:a", "192k",
            tf,
        ]
        res = subprocess.run(cmd, capture_output=True)
        if res.returncode != 0:
            motore.log(f"   ❌ Segmento {i+1} fallito")
            ok = False
            continue
        temp_files.append(tf)
        total += dur

    if not ok or not temp_files:
        for tf in temp_files:
            try: os.remove(tf)
            except OSError: pass
        return None

    def concat(out_path):
        lst = os.path.join(work_tmp, "_concat.txt")
        with open(lst, "w", encoding="utf-8") as f:
            for tf in temp_files:
                f.write(f"file '{tf.replace(os.sep, '/')}'\n")
        res = subprocess.run(
            [motore.FFMPEG_EXE, "-y", "-f", "concat", "-safe", "0", "-i", lst, "-c", "copy", out_path],
            capture_output=True)
        try: os.remove(lst)
        except OSError: pass
        return res.returncode == 0

    if not concat(out_169):
        motore.log("   ❌ Concatenazione 16:9 fallita")
        for tf in temp_files:
            try: os.remove(tf)
            except OSError: pass
        return None

    if out_916:
        # Rielabora i segmenti in 9:16 e concatena
        temp_v = []
        for i, (start, dur) in enumerate(clip_times):
            tfv = os.path.join(work_tmp, f"_tmp_segv_{i:02d}.mp4")
            cmd = [
                motore.FFMPEG_EXE, "-y",
                "-ss", str(round(start, 3)), "-t", str(round(dur, 3)), "-i", input_path,
                "-vf", VF_916,
                "-map", "0:v", "-map", "0:a?",
                "-c:v", "libx264", "-preset", "superfast", "-crf", "19",
                "-c:a", "aac", "-b:a", "192k",
                tfv,
            ]
            res = subprocess.run(cmd, capture_output=True)
            if res.returncode == 0:
                temp_v.append(tfv)
        vertical_ok = temp_v and len(temp_v) == len(clip_times)
        if vertical_ok:
            lst = os.path.join(work_tmp, "_concatv.txt")
            with open(lst, "w", encoding="utf-8") as f:
                for tf in temp_v:
                    f.write(f"file '{tf.replace(os.sep, '/')}'\n")
            res = subprocess.run(
                [motore.FFMPEG_EXE, "-y", "-f", "concat", "-safe", "0", "-i", lst, "-c", "copy", out_916],
                capture_output=True)
            try: os.remove(lst)
            except OSError: pass
            vertical_ok = res.returncode == 0 and os.path.isfile(out_916) and os.path.getsize(out_916) > 0
        if not vertical_ok:
            motore.log("   ❌ Render/concatenazione 9:16 fallita")
            try: os.remove(out_916)
            except OSError: pass
            for tf in temp_v:
                try: os.remove(tf)
                except OSError: pass
            for tf in temp_files:
                try: os.remove(tf)
                except OSError: pass
            return None
        for tf in temp_v:
            try: os.remove(tf)
            except OSError: pass

    for tf in temp_files:
        try: os.remove(tf)
        except OSError: pass

    return total


def save_clip_srt(words: list, start: float, end: float, work_dir: str, base: str):
    """SRT normale + hormozi per una clip (tempi relativi al clip)."""
    cues_n = motore.words_to_cues(words, start, end)
    p_n = os.path.join(work_dir, f"{base}_normal.srt")
    motore.save_srt(cues_n, p_n, offset=start)

    cues_h = motore.words_to_cues(words, start, end, max_words=2)
    p_h = os.path.join(work_dir, f"{base}_hormozi.srt")
    motore.save_srt(cues_h, p_h, offset=start, upper=True)
    return len(cues_n), len(cues_h)


def save_reel_srt(words: list, clip_times: list, work_dir: str, base: str):
    """SRT per reel multi-segmento con offset cumulativo."""
    cues = []
    offset = 0.0
    for start, dur in clip_times:
        end = start + dur
        cues.extend(motore.words_to_cues(words, start, end))
        offset += dur
    # I tempi dei cue sono assoluti; sottrai l'offset progressivo
    # (più semplice: rigenera per segmento con offset relativo)
    all_cues = []
    off = 0.0
    for start, dur in clip_times:
        end = start + dur
        seg_cues = motore.words_to_cues(words, start, end)
        for c in seg_cues:
            all_cues.append({
                "start": c["start"] - start + off,
                "end":   c["end"]   - start + off,
                "text":  c["text"],
            })
        off += dur
    p_n = os.path.join(work_dir, f"{base}_normal.srt")
    motore.save_srt(all_cues, p_n)

    all_h = []
    off = 0.0
    for start, dur in clip_times:
        end = start + dur
        seg_cues = motore.words_to_cues(words, start, end, max_words=2)
        for c in seg_cues:
            all_h.append({
                "start": c["start"] - start + off,
                "end":   c["end"]   - start + off,
                "text":  c["text"],
            })
        off += dur
    p_h = os.path.join(work_dir, f"{base}_hormozi.srt")
    motore.save_srt(all_h, p_h, upper=True)
    return len(all_cues), len(all_h)


# ═══════════════════════════════════════════════════════════════
# FLUSSO PRINCIPALE
# ═══════════════════════════════════════════════════════════════

def ask(prompt: str, default: str) -> str:
    v = input(prompt).strip()
    return v or default


def propose_clips(mode: str, input_path: str, base_name: str,
                  work_dir: str, lingua: str = LANGUAGE_DEFAULT,
                  params: dict | None = None, on_progress=None) -> dict:
    """FASE 1 (GUI): trascrizione + AI + dedupe, SENZA renderizzare.

    Ritorna {clips: [{title,start,end,score,reason}...], duration,
             words_count, input_path, work_dir, mode, lingua}.
    """
    with run_with_params(params, mode):
        rule = MODE_RULES[mode]
        os.makedirs(work_dir, exist_ok=True)
        backup_dir = motore.backup_output_media(work_dir)
        if backup_dir:
            motore.log(f"   🛡️ Output precedenti salvati in: {os.path.basename(backup_dir)}")

        _emit(on_progress, "transcribe", 5, "Trascrizione WhisperX...")
        motore.log(f"\n🎙️  Trascrizione (WhisperX + allineamento parola)...")
        data = motore.transcribe(input_path, work_dir, lingua)
        words = data["words"]
        motore.log(f"   ⏱️  {data['duration']:.0f}s | {len(words)} parole allineate")
        _emit(on_progress, "transcribe", 30, f"Trascritte {len(words)} parole")

        segments = motore.group_into_segments(words)
        ast_ann = motore.events_to_annotations(data.get("events") or [], segments)
        motore.log(f"\n🧠 Analisi AI in blocchi di {AI_CHUNK_SEGS} frasi ({rule['name']})...")

        candidates = []
        n_chunks = (len(segments) - 1) // AI_CHUNK_SEGS + 1
        for ci in range(0, len(segments), AI_CHUNK_SEGS):
            chunk = segments[ci:ci + AI_CHUNK_SEGS]
            num = ci // AI_CHUNK_SEGS + 1
            motore.log(f"   > Blocco {num}/{n_chunks} (frasi {chunk[0]['idx']}-{chunk[-1]['idx']})...")
            _emit(on_progress, "ai", 30 + 55 * num / max(n_chunks, 1),
                  f"AI blocco {num}/{n_chunks}...")
            clips = ai_find_clips(chunk, mode, ast_ann)
            for c in clips:
                motore.log(f"      💡 '{c['title']}' — {c['end']-c['start']:.0f}s — score {c['score']}/10 — {c.get('reason','')}")
            candidates.extend(clips)

        motore.log(f"\n✂️  {len(candidates)} clip candidate -> selezione...")
        final = dedupe_by_overlap(candidates)[:rule["max_clips"]]
        motore.log(f"   ✅ {len(final)} clip definitive (top-score, no duplicati)")
        _emit(on_progress, "done", 90, f"{len(final)} clip proposte")

        return {
            "clips": [{"title": c["title"], "start": round(c["start"], 3),
                       "end": round(c["end"], 3), "score": c["score"],
                       "reason": c.get("reason", "")} for c in final],
            "duration": round(data["duration"], 3),
            "words_count": len(words),
            "segments": [{"idx": s["idx"], "start": round(s["start"], 3),
                          "end": round(s["end"], 3), "text": s.get("text", "")}
                         for s in segments],
            "input_path": input_path, "work_dir": work_dir,
            "mode": mode, "lingua": lingua,
        }


def render_clip_files(input_path: str, clips: list, work_dir: str,
                      lingua: str = LANGUAGE_DEFAULT, vertical: bool = True,
                      params: dict | None = None, on_progress=None) -> list:
    """FASE 2 (GUI): renderizza SOLO le clip selezionate. Ritorna i file creati."""
    with run_with_params(params):
        # la cache trascrizione rende questa chiamata istantanea
        words = motore.transcribe(input_path, work_dir, lingua)["words"]
        created = []
        total = len(clips)
        for idx, c in enumerate(clips, 1):
            st, et = float(c["start"]), float(c["end"])
            dur = et - st
            _emit(on_progress, "render", 100 * (idx - 1) / max(total, 1),
                  f"Render clip {idx}/{total}...")
            base = f"{idx:02d}_{safe_filename(c.get('title', 'clip'))}"
            out16 = os.path.join(work_dir, f"{base}.mp4")
            out9 = os.path.join(work_dir, f"{base}_V.mp4") if vertical else None
            motore.log(f"\n   ✂️  Clip {idx}: '{c.get('title','')}' ({dur:.1f}s, score {c.get('score','?')}/10)")
            if render_clip(input_path, st, et, out16, out9, verbose=False):
                n_cues, n_h = save_clip_srt(words, st, et, work_dir, base)
                motore.log(f"       📝 SRT: {n_cues} cue normal + {n_h} cue hormozi")
                created.append(out16)
                if vertical and out9 and os.path.isfile(out9):
                    created.append(out9)
            motore.log(f"       🎬 {os.path.basename(out16)}")
        _emit(on_progress, "done", 100, f"{len(created)} file creati")
        return created


def process_single(mode: str, input_path: str, base_name: str,
                   work_dir: str, data: dict, vertical: bool):
    """CLI classico: propone e renderizza tutto."""
    rule = MODE_RULES[mode]
    words = data["words"]
    segments = motore.group_into_segments(words)
    ast_ann = motore.events_to_annotations(data.get("events") or [], segments)
    motore.log(f"\n🧠 Analisi AI in blocchi di {AI_CHUNK_SEGS} frasi ({rule['name']})...")

    candidates = []
    n_chunks = (len(segments) - 1) // AI_CHUNK_SEGS + 1
    for ci in range(0, len(segments), AI_CHUNK_SEGS):
        chunk = segments[ci:ci + AI_CHUNK_SEGS]
        num = ci // AI_CHUNK_SEGS + 1
        motore.log(f"   > Blocco {num}/{n_chunks} (frasi {chunk[0]['idx']}-{chunk[-1]['idx']})...")
        clips = ai_find_clips(chunk, mode, ast_ann)
        for c in clips:
            motore.log(f"      💡 '{c['title']}' — {c['end']-c['start']:.0f}s — score {c['score']}/10 — {c.get('reason','')}")
        candidates.extend(clips)

    motore.log(f"\n✂️  {len(candidates)} clip candidate -> selezione...")
    final = dedupe_by_overlap(candidates)[:rule["max_clips"]]
    motore.log(f"   ✅ {len(final)} clip definitive (top-score, no duplicati)")

    created = render_clip_files(input_path,
                                [{"title": c["title"], "start": c["start"],
                                  "end": c["end"], "score": c["score"],
                                  "reason": c.get("reason", "")} for c in final],
                                work_dir, LANGUAGE_DEFAULT, vertical)
    motore.log(f"\n✅ {len(created)} file creati in {work_dir}")


def propose_reels(input_path: str, base_name: str, work_dir: str,
                  lingua: str = LANGUAGE_DEFAULT,
                  params: dict | None = None, on_progress=None) -> dict:
    """FASE 1 (GUI): momenti + composizione reel, SENZA montare.

    Ritorna {moments: [...], reels: [{title,hook,indices,total_dur,
             segments:[[start,end]...]}...], ...}.
    """
    with run_with_params(params, "3"):
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
        motore.log(f"\n🧠 FASE 1: ricerca momenti ad alto impatto...")

        moments = []
        n_chunks = (len(segments) - 1) // AI_CHUNK_SEGS + 1
        for ci in range(0, len(segments), AI_CHUNK_SEGS):
            chunk = segments[ci:ci + AI_CHUNK_SEGS]
            num = ci // AI_CHUNK_SEGS + 1
            motore.log(f"   > Blocco {num}/{n_chunks} (frasi {chunk[0]['idx']}-{chunk[-1]['idx']})...")
            _emit(on_progress, "moments", 25 + 45 * num / max(n_chunks, 1),
                  f"Momenti blocco {num}/{n_chunks}...")
            found = ai_find_moments(chunk, ast_ann)
            for m in found:
                motore.log(f"      💡 [M] '{m['title']}' — {m['end']-m['start']:.0f}s — score {m['score']}/10")
            moments.extend(found)

        moments = dedupe_by_overlap(moments)
        moments = sorted(moments, key=lambda m: -m["score"])[:12]
        motore.log(f"\n   ✅ {len(moments)} momenti validi (top-score)")

        if len(moments) < 2:
            motore.log("   ⚠️  Pochi momenti validi. Riprova con un video più ricco.")
            return {"moments": [], "reels": [], "duration": round(data["duration"], 3),
                    "input_path": input_path, "work_dir": work_dir, "lingua": lingua}

        _emit(on_progress, "compose", 75, "Composizione reel...")
        motore.log(f"\n🎬 FASE 2: composizione reel narrativi...")
        reels = ai_compose_reels(moments)
        motore.log(f"   ✅ {len(reels)} reel da montare\n")
        _emit(on_progress, "done", 85, f"{len(reels)} reel proposti")

        out_reels = []
        for reel in reels:
            segs = []
            for mi in reel["indices"]:
                m = moments[mi - 1]
                segs.append([round(m["start"], 3), round(m["end"], 3)])
            out_reels.append({"title": reel["title"], "hook": reel.get("hook", ""),
                              "total_dur": round(reel["total_dur"], 1),
                              "segments": segs})

        return {
            "moments": [{"title": m["title"], "topic": m.get("topic", ""),
                         "start": round(m["start"], 3), "end": round(m["end"], 3),
                         "score": m["score"]} for m in moments],
            "reels": out_reels,
            "duration": round(data["duration"], 3),
            "words_count": len(words),
            "input_path": input_path, "work_dir": work_dir, "lingua": lingua,
        }


def render_reel_files(input_path: str, reels: list, work_dir: str,
                      lingua: str = LANGUAGE_DEFAULT, vertical: bool = True,
                      params: dict | None = None, on_progress=None) -> list:
    """FASE 2 (GUI): monta SOLO i reel selezionati. Ritorna i file creati."""
    with run_with_params(params, "3"):
        words = motore.transcribe(input_path, work_dir, lingua)["words"]
        created = []
        total = len(reels)
        for idx, reel in enumerate(reels, 1):
            _emit(on_progress, "render", 100 * (idx - 1) / max(total, 1),
                  f"Montaggio reel {idx}/{total}...")
            base = f"REEL_{idx:02d}_{safe_filename(reel.get('title', 'reel'))}"
            out16 = os.path.join(work_dir, f"{base}.mp4")
            out9 = os.path.join(work_dir, f"{base}_V.mp4") if vertical else None
            clip_times = [(float(s), float(e) - float(s))
                          for s, e in reel["segments"]]
            motore.log(f"   🎬 Reel {idx}: '{reel.get('title','')}' — {len(clip_times)} segmenti")
            total_dur = render_reel(input_path, clip_times, out16, out9)
            if total_dur is None:
                motore.log("      ❌ Montaggio reel fallito.")
                continue
            n_cues, n_h = save_reel_srt(words, clip_times, work_dir, base)
            motore.log(f"      📝 SRT: {n_cues} cue normal + {n_h} cue hormozi")
            created.append(out16)
            if vertical and out9 and os.path.isfile(out9):
                created.append(out9)
            motore.log(f"      ✅ {os.path.basename(out16)} — durata {total_dur:.0f}s")
        _emit(on_progress, "done", 100, f"{len(created)} file creati")
        return created


def process_reels(input_path: str, base_name: str, work_dir: str,
                 data: dict, vertical: bool):
    """CLI classico: propone e monta tutti i reel."""
    words = data["words"]
    segments = motore.group_into_segments(words)
    ast_ann = motore.events_to_annotations(data.get("events") or [], segments)
    motore.log(f"\n🧠 FASE 1: ricerca momenti ad alto impatto...")

    moments = []
    n_chunks = (len(segments) - 1) // AI_CHUNK_SEGS + 1
    for ci in range(0, len(segments), AI_CHUNK_SEGS):
        chunk = segments[ci:ci + AI_CHUNK_SEGS]
        num = ci // AI_CHUNK_SEGS + 1
        motore.log(f"   > Blocco {num}/{n_chunks} (frasi {chunk[0]['idx']}-{chunk[-1]['idx']})...")
        found = ai_find_moments(chunk, ast_ann)
        for m in found:
            motore.log(f"      💡 [M] '{m['title']}' — {m['end']-m['start']:.0f}s — score {m['score']}/10")
        moments.extend(found)

    moments = dedupe_by_overlap(moments)
    moments = sorted(moments, key=lambda m: -m["score"])[:12]
    motore.log(f"\n   ✅ {len(moments)} momenti validi (top-score)")

    if len(moments) < 2:
        motore.log("   ⚠️  Pochi momenti validi. Riprova con un video più ricco.")
        return

    motore.log(f"\n🎬 FASE 2: composizione reel narrativi...")
    reels = ai_compose_reels(moments)
    if not reels:
        motore.log("   ⚠️  Nessun reel componibile valido.")
        return

    created = render_reel_files(
        input_path,
        [{"title": r["title"],
          "segments": [[moments[i - 1]["start"], moments[i - 1]["end"]]
                       for i in r["indices"]]} for r in reels],
        work_dir, LANGUAGE_DEFAULT, vertical)
    motore.log(f"\n✅ {len(created)} file creati in {work_dir}")


def process_videos():
    motore.log("\n" + "=" * 55)
    motore.log("      WORKFLOW CLIP AI v3 — NICOLA LANCI")
    motore.log("=" * 55)
    motore.log("Modalità:")
    motore.log("  1) Instagram Stories   (clip <= 25s)")
    motore.log("  2) Social / Reels      (clip 30-60s)")
    motore.log("  3) REEL NARRATIVO      (montaggio 40-75s)")
    scelta = ask("\nScelta [1/2/3] (default 2): ", "2")
    if scelta not in ("1", "2", "3"):
        motore.log("❌ Scelta non valida.")
        return

    lingua = ask("Lingua del video [it/en] (default it): ", "it").lower()
    if lingua not in ("it", "en"):
        lingua = "it"
    vertical = ask("Export verticale 9:16 con sfondo sfocato? [s/n] (default s): ", "s").lower()
    vertical = vertical in ("s", "y", "si", "yes", "")

    if not os.path.exists(VIDEO_OUTPUT_DIR):
        os.makedirs(VIDEO_OUTPUT_DIR)

    if not os.path.exists(VIDEO_INPUT_DIR):
        os.makedirs(VIDEO_INPUT_DIR)

    videos = [f for f in os.listdir(VIDEO_INPUT_DIR)
              if f.lower().endswith(('.mp4', '.mkv', '.mov'))]
    if not videos:
        motore.log(f"❌ Nessun video in {VIDEO_INPUT_DIR}")
        return

    for video in videos:
        input_path = os.path.join(VIDEO_INPUT_DIR, video)
        base_name  = os.path.splitext(video)[0]
        work_dir   = os.path.join(VIDEO_OUTPUT_DIR, base_name.replace(" ", "_"))
        os.makedirs(work_dir, exist_ok=True)

        backup_dir = motore.backup_output_media(work_dir)
        if backup_dir:
            motore.log(f"   🛡️ Output precedenti salvati in: {os.path.basename(backup_dir)}")

        motore.log(f"\n{'='*55}")
        motore.log(f"   🎬 {base_name} — modalità {MODE_RULES[scelta]['name']}")
        motore.log(f"{'='*55}")

        motore.log(f"\n🎙️  Trascrizione (WhisperX + allineamento parola)...")
        data = motore.transcribe(input_path, work_dir, lingua)
        motore.log(f"   ⏱️  {data['duration']:.0f}s | {len(data['words'])} parole allineate")

        if scelta in ("1", "2"):
            process_single(scelta, input_path, base_name, work_dir, data, vertical)
        else:
            process_reels(input_path, base_name, work_dir, data, vertical)

    motore.log(f"\n✅ Processo completato! Output in: {VIDEO_OUTPUT_DIR}")


if __name__ == "__main__":
    process_videos()
