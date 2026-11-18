# -*- coding: utf-8 -*-
"""
SGROSSATORE v4 — Taglio "sgrosso" professionale
================================================
Rimuove da un video parlato:
  1. Silenzi lunghi          (gap tra parole > MAX_SILENCE_SEC, conservando SILENCE_KEEP_PAD attorno alla voce)
  2. Fiati / respiri         (solo se ISOLATI tra due pause, con guardia anti-abuso)
  3. Filler words            (ehm, uhm, mh, ... clampati tra le parole vicine)
  4. False partenze / bad takes (AI locale a 2 passaggi: proposta + revisione critica)

Nessun taglio entra mai dentro una parola: il piano tagli è verificato con
`speech_overlap_seconds()` e finisce in `tagli_sgrossatura.txt/.json`.
Ogni spezzone reso è misurato con ffprobe (scostamenti >0.30s segnalati) e viene
creata anche `00_anteprima_sgrossata.mp4` (concat dei clip per rivedere il ritmo).

Motore di trascrizione (motore.py):
  WhisperX = faster-whisper large-v3 (GPU) + VAD + forced alignment wav2vec2 italiano
  -> timestamp parola precisi: i tagli non mozzano più le parole.

L'AI (Ollama qwen2.5:14b) lavora su frasi NUMERATE [S1]..[Sn] e risponde con
indici interi (structured outputs) -> zero matching fuzzy, zero JSON corrotti.

OUTPUT: cartella video_output/[video]_SGROSSO/ con 001_clip.mp4, 002_clip.mp4, ...
        (gli spezzoni "buoni" in ordine, pronti per il montaggio in Premiere)
"""

import os
import re
import json
import unicodedata
import subprocess

import motore

# --- CONFIGURAZIONE ---
VIDEO_INPUT_DIR  = r"K:\AI\ClipsAI\video_da_sgrossare"
VIDEO_OUTPUT_DIR = r"K:\AI\ClipsAI\video_output"
LANGUAGE         = "it"

# --- IMPOSTAZIONI SGROSSO ---
MAX_SILENCE_SEC     = 1.2                     # gap > 1.2s tra parole = silenzio da tagliare
FILLER_WORDS        = ["ehm", "uhm", "uh", "mhm", "hmm", "mh", "eh", "uhh", "uhmm"]
FILLER_MAX_SEC      = 0.7                     # un "filler" > 0.7s è probabilmente una parola vera
ENABLE_AI_BAD_TAKES = True
AI_CHUNK_SEGS       = 90                      # frasi per chiamata AI (~10-18 min di parlato):
                                              # più contesto per le false partenze, metà chiamate
MAX_CUT_SEC         = 60.0                    # un bad take non può superare 60s

# --- IMPOSTAZIONI FIATI ---
ENABLE_BREATH_DETECTION = True
BREATH_MIN_SEC          = 0.08                # durata minima di un fiato
BREATH_MAX_SEC          = 0.7                 # durata massima di un fiato
BREATH_DB_THRESHOLD     = -40                 # soglia dB del silencedetect
BREATH_GAP_MIN_SEC      = 0.08                # gap minimo tra parole in cui cercare il fiato

# --- TAGLIO (v4: i tagli rispettano SEMPRE i confini delle parole) ---
MIN_CLIP_SEC          = 0.8    # spezzoni più corti vengono scartati
MIN_CUT_SEC           = 0.15   # sotto questa soglia un taglio è inutile (crea solo un click)
CUT_EDGE_SILENCE      = True   # taglia anche il silenzio iniziale/finale
SILENCE_KEEP_PAD      = 0.20   # secondi di pausa CONSERVATI attorno alla voce
WORD_SAFE_PAD         = 0.02   # margine di sicurezza: mai entrare nelle parole
BREATH_ISOLATION_SEC  = 0.06   # un fiato è tagliabile solo se ha silenzio ai lati
BREATH_MAX_WORD_RATIO = 0.10   # oltre questa quota di fiati il rilevamento è inaffidabile
AUDIO_FADE_SEC        = 0.015  # micro-fade audio ai bordi (anti-click)
MAKE_PREVIEW          = True   # crea 00_anteprima_sgrossata.mp4 (concat dei clip)
AUTO_REPORT           = True   # scrive tagli_sgrossatura.txt/.json anche dai job GUI
AI_MIN_CONFIDENCE     = 55     # proposte AI sotto questa soglia: scartate prima della revisione
AI_DEBUG_DUMP         = True   # scrive ai_badtakes_debug.json (proposte + decisioni AI)
MERGE_GOOD_GAP_SEC    = 0.05   # ricompone solo briciole tra due spezzoni

# --- PARAMETRI GUI (override via run_with_params, default = costanti sopra) ---
# Chiavi accettate: MAX_SILENCE_SEC, FILLER_WORDS, FILLER_MAX_SEC,
# ENABLE_AI_BAD_TAKES, AI_CHUNK_SEGS, MAX_CUT_SEC, ENABLE_BREATH_DETECTION,
# BREATH_MIN_SEC, BREATH_MAX_SEC, BREATH_DB_THRESHOLD, BREATH_GAP_MIN_SEC,
# MIN_CLIP_SEC, MERGE_GOOD_GAP_SEC, LANGUAGE, OLLAMA_MODEL,
# ENABLE_AUDIO_EVENTS (inoltrato a motore.py)
_PARAM_KEYS = (
    "MAX_SILENCE_SEC", "FILLER_WORDS", "FILLER_MAX_SEC",
    "ENABLE_AI_BAD_TAKES", "AI_CHUNK_SEGS", "MAX_CUT_SEC",
    "ENABLE_BREATH_DETECTION", "BREATH_MIN_SEC", "BREATH_MAX_SEC",
    "BREATH_DB_THRESHOLD", "BREATH_GAP_MIN_SEC",
    "MIN_CLIP_SEC", "MERGE_GOOD_GAP_SEC", "LANGUAGE",
    "MIN_CUT_SEC", "CUT_EDGE_SILENCE", "SILENCE_KEEP_PAD", "WORD_SAFE_PAD",
    "BREATH_ISOLATION_SEC", "BREATH_MAX_WORD_RATIO",
    "AUDIO_FADE_SEC", "MAKE_PREVIEW", "AUTO_REPORT",
    "AI_MIN_CONFIDENCE", "AI_DEBUG_DUMP",
)


class run_with_params:
    """Context manager: applica override parametri (GUI) e ripristina tutto.

    Imposta anche motore.OLLAMA_MODEL se passato come OLLAMA_MODEL.
    Sicuro perché il server esegue un solo job alla volta (lock globale).
    """
    def __init__(self, params: dict | None):
        self.params = dict(params or {})
        self.saved = {}

    def __enter__(self):
        import motore as _m
        g = globals()
        for k in _PARAM_KEYS:
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


# ═══════════════════════════════════════════════════════════════
# UTILITÀ
# ═══════════════════════════════════════════════════════════════

def normalize_text(text: str) -> str:
    if not text:
        return ""
    nfkd = unicodedata.normalize('NFKD', text)
    t = "".join([c for c in nfkd if not unicodedata.combining(c)])
    t = re.sub(r'[^a-zA-Z0-9\s]', '', t.lower())
    return " ".join(t.split())


def fillers_in_word(word: str) -> bool:
    return normalize_text(word) in FILLER_WORDS


# ═══════════════════════════════════════════════════════════════
# STEP A: SILENZI (gap tra parole allineate)
# ═══════════════════════════════════════════════════════════════

def find_silence_intervals(words: list) -> list:
    """Gap > MAX_SILENCE_SEC tra una parola e la successiva."""
    intervals = []
    for i in range(1, len(words)):
        gap = words[i]['start'] - words[i-1]['end']
        if gap > MAX_SILENCE_SEC:
            intervals.append((words[i-1]['end'], words[i]['start']))
    return intervals


# ═══════════════════════════════════════════════════════════════
# STEP B: FIATI (micro-suoni tra le parole, FFmpeg silencedetect)
# ═══════════════════════════════════════════════════════════════

def detect_breaths_ffmpeg(input_path: str, words: list) -> list:
    """
    Un fiato = suono breve (0.08-0.7s) a volume medio-basso tra due parole.
    Usiamo silencedetect: le parti NON-silenziose dentro un gap tra parole
    sono i candidati fiato.
    """
    if not words:
        return []

    motore.log("   🫁 Rilevamento Fiati (FFmpeg silencedetect)...")
    try:
        res = subprocess.run(
            [motore.FFMPEG_EXE, "-hide_banner", "-nostats", "-i", input_path,
             "-af", f"silencedetect=n={BREATH_DB_THRESHOLD}dB:d=0.05",
             "-vn", "-f", "null", "-"],
            capture_output=True, text=True, encoding='utf-8', errors='ignore',
            timeout=1800)
    except subprocess.TimeoutExpired:
        motore.log("      ⚠️  silencedetect in timeout — skip fiati.")
        return []
    if res.returncode != 0:
        motore.log("      ⚠️  silencedetect fallito — skip fiati.")
        return []

    silences = []
    silence_start = None
    for line in (res.stderr or "").split('\n'):
        if 'silence_start:' in line:
            m = re.search(r'silence_start:\s*([\d.]+)', line)
            if m:
                silence_start = float(m.group(1))
        elif 'silence_end:' in line and silence_start is not None:
            m = re.search(r'silence_end:\s*([\d.]+)', line)
            if m:
                silences.append((silence_start, float(m.group(1))))
                silence_start = None

    if not silences:
        motore.log("      ⚠️  Nessun dato da silencedetect — skip fiati.")
        return []

    # Per ogni gap tra parole: le parti non-silenziose = potenziali fiati
    breaths = []
    for i in range(1, len(words)):
        gap_start = words[i-1]['end']
        gap_end   = words[i]['start']
        if gap_end - gap_start < BREATH_GAP_MIN_SEC:
            continue

        gap_silences = [(max(s, gap_start), min(e, gap_end))
                        for s, e in silences if e > gap_start and s < gap_end]
        gap_silences.sort()

        sounds = []
        curr = gap_start
        for s, e in gap_silences:
            if s > curr:
                sounds.append((curr, s))
            curr = max(curr, e)
        if curr < gap_end:
            sounds.append((curr, gap_end))

        for bs, be in sounds:
            if not (BREATH_MIN_SEC <= (be - bs) <= BREATH_MAX_SEC):
                continue
            # Solo fiati ISOLATI: deve esserci silenzio su ENTRAMBI i lati.
            # Altrimenti è coda di parola o rumore continuo: tagliarlo mozzerebbe la voce.
            if bs < gap_start + BREATH_ISOLATION_SEC:
                continue
            if be > gap_end - BREATH_ISOLATION_SEC:
                continue
            breaths.append((bs, be))

    motore.log(f"      🫁 Trovati {len(breaths)} fiati potenziali.")
    return breaths


# ═══════════════════════════════════════════════════════════════
# STEP C: FILLER WORDS
# ═══════════════════════════════════════════════════════════════

def find_filler_intervals(words: list) -> list:
    intervals = []
    for w in words:
        if fillers_in_word(w['word']) and (w['end'] - w['start']) <= FILLER_MAX_SEC:
            intervals.append((max(0.0, w['start'] - 0.05), w['end'] + 0.05))
    return intervals


# ═══════════════════════════════════════════════════════════════
# PIANO TAGLI v4 (padding attorno alla voce + clamp sulle parole)
# ═══════════════════════════════════════════════════════════════

def _clamp(value, lo, hi):
    return max(lo, min(hi, value))


def find_silence_cuts(words: list, total_duration: float) -> list:
    """Tagli di silenzio CONSERVANDO SILENCE_KEEP_PAD secondi attorno alla voce.

    La v3 tagliava l'intero gap: le frasi si incollavano senza respiro e ogni
    imprecisione dell'allineamento diventava un taglio secco e innaturale.
    """
    cuts = []
    if not words:
        return cuts
    pad = SILENCE_KEEP_PAD

    if CUT_EDGE_SILENCE:
        head = words[0]['start']
        if head > MAX_SILENCE_SEC and head - pad >= MIN_CUT_SEC:
            cuts.append((0.0, head - pad, f"Silenzio iniziale {head:.1f}s"))
        tail = total_duration - words[-1]['end']
        if tail > MAX_SILENCE_SEC and tail - pad >= MIN_CUT_SEC:
            cuts.append((words[-1]['end'] + pad, total_duration, f"Silenzio finale {tail:.1f}s"))

    for i in range(1, len(words)):
        gap = words[i]['start'] - words[i-1]['end']
        if gap <= MAX_SILENCE_SEC:
            continue
        st = words[i-1]['end'] + pad
        et = words[i]['start'] - pad
        if et - st >= MIN_CUT_SEC:
            cuts.append((st, et, f"Silenzio {gap:.1f}s"))
    return cuts


def find_breath_cuts(breaths: list) -> list:
    """I fiati arrivano già isolati e interni al gap: qui solo la soglia minima."""
    cuts = []
    for bs, be in breaths:
        if be - bs >= MIN_CUT_SEC:
            cuts.append((bs, be, "Fiato"))
    return cuts


def find_filler_cuts(words: list) -> list:
    """Taglia la parola filler senza mai invadere le parole vicine."""
    cuts = []
    for i, w in enumerate(words):
        if not fillers_in_word(w['word']) or (w['end'] - w['start']) > FILLER_MAX_SEC:
            continue
        prev_end = words[i-1]['end'] if i > 0 else 0.0
        next_start = words[i+1]['start'] if i + 1 < len(words) else w['end'] + WORD_SAFE_PAD
        st = _clamp(w['start'] - WORD_SAFE_PAD, prev_end, w['end'])
        et = _clamp(w['end'] + WORD_SAFE_PAD, w['start'], next_start)
        if et - st >= MIN_CUT_SEC:
            cuts.append((st, et, f"Filler '{w['word']}'"))
    return cuts


def merge_cuts_with_reasons(cuts: list) -> list:
    """Unisce tagli sovrapposti/adiacenti in un intervallo unico con motivo leggibile."""
    merged = []
    for st, et, reason in sorted(cuts, key=lambda c: (c[0], c[1])):
        if merged and st <= merged[-1][1] + 1e-6:
            merged[-1][1] = max(merged[-1][1], et)
            if reason not in merged[-1][2]:
                merged[-1][2].append(reason)
        else:
            merged.append([st, et, [reason]])
    return [(st, et, " + ".join(parts)[:160]) for st, et, parts in merged]


def speech_overlap_seconds(cuts: list, words: list) -> float:
    """Secondi di parlato che finiscono dentro un taglio (esclusi filler e AI, voluti)."""
    total = 0.0
    for st, et, reason in cuts:
        if reason.startswith("Filler") or reason.startswith("AI"):
            continue
        for w in words:
            if w['end'] <= st:
                continue
            if w['start'] >= et:
                break
            total += max(0.0, min(et, w['end']) - max(st, w['start']))
    return total


def build_cut_plan(input_path: str, words: list, total_duration: float,
                   on_progress=None) -> tuple:
    """Piano tagli v4: silenzi con padding, fiati isolati, filler clampati.

    Ritorna (cuts, breaths): cuts = [(start, end, reason), ...].
    """
    cuts = list(find_silence_cuts(words, total_duration))

    breaths = []
    if ENABLE_BREATH_DETECTION:
        _emit(on_progress, "breaths", 50, "Rilevamento fiati...")
        breaths = detect_breaths_ffmpeg(input_path, words)
        limit = max(10, int(BREATH_MAX_WORD_RATIO * len(words)))
        if len(breaths) > limit:
            motore.log(f"      ⚠️  Fiati inaffidabili ({len(breaths)} su {len(words)} "
                       f"parole, limite {limit}) → fiati ignorati")
            breaths = []
        cuts.extend(find_breath_cuts(breaths))

    _emit(on_progress, "fillers", 55, "Analisi filler...")
    cuts.extend(find_filler_cuts(words))
    return cuts, breaths


def _plan_from_workdir(work_dir: str) -> tuple:
    """Rilegge il piano tagli persistito (`sgrossa_analyze.json`) per il report.

    Serve alla GUI: il job di analisi salva il piano, il job di taglio lo riusa
    senza doverlo ricalcolare.
    """
    try:
        with open(os.path.join(work_dir, "sgrossa_analyze.json"), "r", encoding="utf-8") as f:
            plan = json.load(f)
        cuts = [(float(c["start"]), float(c["end"]), c.get("reason", ""))
                for c in plan.get("cuts", [])]
        good = [(float(s), float(e)) for s, e in plan.get("good", [])]
        return cuts, good, int(plan.get("words_count", 0)), float(plan.get("duration", 0.0))
    except Exception:
        return [], [], 0, 0.0


def save_cut_report(output_dir: str, base_name: str, cuts: list, good: list,
                    words_count: int, total_duration: float, rendered: list) -> str:
    """Report umano+JSON: ogni taglio pianificato e ogni spezzone realmente reso."""
    txt_path = os.path.join(output_dir, "tagli_sgrossatura.txt")
    json_path = os.path.join(output_dir, "tagli_sgrossatura.json")
    removed = sum(e - s for s, e, _ in cuts)

    lines = [
        f"# PIANO TAGLI — {base_name}",
        f"# Durata originale: {total_duration:.1f}s | parole allineate: {words_count}",
        f"# Tagli: {len(cuts)} ({removed:.1f}s rimossi, {100*removed/max(total_duration,1e-6):.1f}% del video)",
        f"# Spezzoni buoni: {len(good)} | resi: {len(rendered)}",
        "#",
        "# TAGLI (n  start -> end  durata  motivo)",
    ]
    for i, (st, et, reason) in enumerate(cuts, 1):
        lines.append(f"{i:04d}  {st:9.3f} -> {et:9.3f}  ({et-st:6.3f}s)  {reason}")
    lines.append("")
    lines.append("# SPEZZONI RESI (file  sorgente start -> end  durata resa)")
    for path, st, et, got in rendered:
        lines.append(f"{os.path.basename(path)}  {st:9.3f} -> {et:9.3f}  ({got:6.3f}s)")
    with open(txt_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    payload = {
        "base_name": base_name,
        "duration": round(total_duration, 3),
        "words": words_count,
        "removed_sec": round(removed, 3),
        "cuts": [{"start": round(s, 3), "end": round(e, 3),
                  "dur": round(e - s, 3), "reason": r} for s, e, r in cuts],
        "clips": [{"file": os.path.basename(p), "start": round(s, 3),
                   "end": round(e, 3), "rendered_dur": round(g, 3)}
                  for p, s, e, g in rendered],
    }
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, ensure_ascii=False, indent=1)
    return txt_path


def make_preview(output_dir: str, clip_paths: list):
    """Concatena gli spezzoni in un unico mp4 per la revisione rapida del ritmo."""
    if len(clip_paths) < 2:
        return None
    out_path = os.path.join(output_dir, "00_anteprima_sgrossata.mp4")
    lst = os.path.join(output_dir, "_preview_concat.txt")
    with open(lst, "w", encoding="utf-8") as f:
        for p in clip_paths:
            f.write(f"file '{p.replace(os.sep, '/')}'\n")
    res = subprocess.run(
        [motore.FFMPEG_EXE, "-y", "-f", "concat", "-safe", "0",
         "-i", lst, "-c", "copy", out_path],
        capture_output=True)
    try:
        os.remove(lst)
    except OSError:
        pass
    if res.returncode == 0 and os.path.isfile(out_path) and os.path.getsize(out_path) > 0:
        return out_path
    motore.log("   ⚠️  Anteprima non creata")
    return None


# ═══════════════════════════════════════════════════════════════
# STEP D: AI — BAD TAKES a 2 passaggi (proposta + revisione)
# ═══════════════════════════════════════════════════════════════

def build_annotations(segments: list, words: list, events: list | None = None) -> dict:
    """
    Per ogni frase numerata, annota eventi rilevanti:
      (PAUSA Xs)     -> silenzio > 1.2s subito PRIMA della frase
      (FILLER: x)    -> filler word dentro la frase
      (FIATO)        -> fiato rilevato nel gap prima della frase
      (FIATO)/(RISATA)/(MUSICA)... -> evento audio AST sovrapposto alla frase
    """
    ann = {}
    breath_set = []  # riutilizziamo i fiati passati via globale sotto
    prev_end = 0.0
    for seg in segments:
        marks = []
        gap_before = seg['start'] - prev_end
        if gap_before > MAX_SILENCE_SEC:
            marks.append(f"(PAUSA {gap_before:.1f}s)")
        fillers = [w['word'] for w in seg.get('_words', []) if fillers_in_word(w['word'])]
        if fillers:
            marks.append(f"(FILLER: {', '.join(fillers)})")
        if marks:
            ann[seg['idx']] = " " + " ".join(marks)
        prev_end = seg['end']
    # Eventi AST (fiati/risate/musica...): si aggiungono in coda ai marcatori
    for idx, extra in motore.events_to_annotations(events or [], segments).items():
        ann[idx] = ann.get(idx, "") + extra
    return ann


def _attach_words(segments: list, words: list) -> list:
    """Attacca le parole grezze a ogni frase numerata (per le annotazioni)."""
    segs = []
    wi = 0
    for seg in segments:
        s_words = []
        while wi < len(words) and words[wi]['end'] <= seg['end'] + 0.01:
            if words[wi]['start'] >= seg['start'] - 0.01:
                s_words.append(words[wi])
            wi += 1
        seg2 = dict(seg)
        seg2['_words'] = s_words
        segs.append(seg2)
    return segs


# Diagnostica AI (proposte/decisioni per chunk) — serve a capire perché un taglio non è stato fatto
_AI_DIAGNOSTICS: list = []


def _locate_quote(chunk: list, start_idx: int, end_idx: int, quote: str):
    """Trova la sequenza CONTIGUA di parole corrispondente a `quote`.

    In questo modo si tagliano SOLO le parole sbagliate (timestamp allineati),
    lasciando intatta la versione corretta che segue.
    Ritorna (start_sec, end_sec) oppure None.
    """
    want = normalize_text(quote or "").split()
    if not want:
        return None
    span = []
    for seg in chunk:
        if start_idx <= seg.get("idx", -1) <= end_idx:
            span.extend(seg.get("_words", []))
    if len(span) < len(want):
        return None
    norm = [normalize_text(w.get("word", "")) for w in span]
    n = len(want)
    best = None
    for wlen in (n, n + 1, n - 1):
        if wlen < 1 or wlen > len(norm):
            continue
        for i in range(0, len(norm) - wlen + 1):
            hits = sum(1 for k in range(min(wlen, n)) if norm[i + k] == want[k])
            score = (hits - abs(wlen - n)) / n
            if best is None or score > best[0]:
                best = (score, i, wlen)
    if not best or best[0] < 0.85:
        return None
    _score, i, wlen = best
    return span[i]["start"], span[i + wlen - 1]["end"]


def dump_ai_diagnostics(output_dir: str):
    """Scrive ai_badtakes_debug.json (proposte + decisioni AI) per diagnosi/regolazione."""
    if not AI_DEBUG_DUMP or not _AI_DIAGNOSTICS:
        return None
    path = os.path.join(output_dir, "ai_badtakes_debug.json")
    try:
        with open(path, "w", encoding="utf-8") as f:
            json.dump(_AI_DIAGNOSTICS, f, ensure_ascii=False, indent=1)
        return path
    except Exception:
        return None


def extract_bad_takes_ai(annotated_text: str, label: str) -> list:
    """
    v5 — 2 passaggi, ANCORATI AL TESTO:
      FASE 1 (bozza):    l'AI propone i tagli citando le PAROLE ESATTE da rimuovere
      FASE 2 (revisione): un supervisore VALIDA o SCARTA ogni proposta
    Il taglio finale usa i timestamp delle parole citate (non l'intera frase):
    così la versione corretta che segue resta intatta.
    Ritorna [(start_sec, end_sec, reason), ...].
    """
    # ── FASE 1 ──
    prompt_draft = f"""Sei un editor di doppiaggio italiano. Nella trascrizione ANNOTATA di Nicola Lanci (doppiatore) trova SOLO gli errori di registrazione: false partenze, ripetizioni fatte per correggersi, parole inciampate.

LEGENDA: [S#] = numero frase; (PAUSA Xs) = silenzio prima della frase; (FILLER: x) = esitazione; (FIATO)/(RISATA)/(MUSICA)... = evento audio (indizio, non verità).

COME SI RICONOSCE UN ERRORE (esempi):
1) Riattacco: "allora il colore — no aspetta — il colore della parete è blu"
   → rimuovi "allora il colore no aspetta"; tieni "il colore della parete è blu".
2) Ripetizione ravvicinata: "questo è un test, questo è un test importante"
   → rimuovi la prima "questo è un test"; tieni la seconda.
3) Correzione nel mezzo: "dobbiamo aspettare tre, anzi quattro giorni"
   → rimuovi "tre, anzi"; tieni "quattro giorni".
4) Parola ripetuta: "la macchina — la macchina rossa"
   → rimuovi la prima "la macchina".

COSA NON È UN ERRORE (non segnalare):
- Battute, imitazioni, enfasi, citazioni, ironia: Nicola è doppiatore, le variazioni sono volute.
- Pause, respiri, riformulazioni volute con parole diverse.

REGOLE RISPOSTA:
- `remove_quote`: copia LETTERALMENTE (stesse parole, in ordine) SOLO la parte da rimuovere: sequenza CONTIGUA presente nel testo, max 20 parole. NON includere la parte corretta.
- `start_seg`/`end_seg`: frasi [S#] che contengono quella parte (estremi inclusi).
- `keep_quote`: copia la versione corretta che resta (facoltativo).
- `category`: false_partenza | ripetizione | correzione | inciampo. `confidence`: 0-100.
- Se non trovi errori certi, restituisci "proposed_cuts": [].

TRASCRIZIONE ANNOTATA ({label}):
{annotated_text}
"""

    try:
        result = motore.ollama_chat(prompt_draft, schema=motore.CUT_SCHEMA, temperature=0.0)
    except Exception as e:
        motore.log(f"      ⚠️  Fase 1 (bozza) fallita: {e}")
        return []

    proposed = []
    for c in result.get("proposed_cuts", []):
        try:
            conf = int(c.get("confidence", 100))
        except (TypeError, ValueError):
            conf = 100
        quote = (c.get("remove_quote") or "").strip()
        if conf < AI_MIN_CONFIDENCE:
            motore.log(f"      ⤵️  Scartata (conf {conf}%): \"{quote[:50]}\"")
            continue
        c["_confidence"] = conf
        proposed.append(c)

    if not proposed:
        motore.log("      📋 Fase 1: nessuna proposta valida.")
        return []
    motore.log(f"      📋 Fase 1: {len(proposed)} proposte (soglia conf {AI_MIN_CONFIDENCE}%). Validazione...")
    for c in proposed:
        motore.log(f"         • [{c.get('start_seg')}-{c.get('end_seg')}] {c.get('category','?')} "
                   f"conf {c['_confidence']}%: \"{(c.get('remove_quote') or '')[:70]}\"")
    _AI_DIAGNOSTICS.append({"label": label, "phase1": proposed})

    cuts_str = "\n".join(
        f"- [{c.get('start_seg','?')}-{c.get('end_seg','?')}] categoria={c.get('category','?')} "
        f"conf={c['_confidence']}% rimuovi=\"{c.get('remove_quote','')}\" "
        f"tieni=\"{c.get('keep_quote','')}\" motivo={c.get('reason','')}"
        for c in proposed)

    # ── FASE 2 ──
    prompt_verify = f"""Sei il supervisore del montaggio di Nicola Lanci (doppiatore). L'assistente propone di TOGLIERE alcune parole. Per OGNI proposta devi scegliere UNA di queste due etichette:

- RIMUOVI  = confermo: quelle parole sono un errore di registrazione e vanno tolte dal video.
- LASCIA   = non toccare: non è un errore, oppure non sono sicuro.

Scegli RIMUOVI quando:
1. La parte citata è un vero errore di registrazione: parola riattaccata, frase interrotta e ripetuta subito dopo, ripetizione della stessa parola o espressione.
2. La versione corretta esiste subito dopo nel testo, oppure la parte citata è una ripetizione inutile.
3. Togliere la parte citata NON rompe il senso della frase.

Scegli LASCIA quando:
1. È una battuta, imitazione, enfasi, citazione o ironia (Nicola è doppiatore: sono volute).
2. È una riformulazione voluta con parole diverse, non una correzione.
3. La citazione non esiste nel testo così com'è, o includerebbe anche la versione corretta.
4. Nel dubbio, LASCIA.

Se la citazione è imprecisa ma l'errore è chiaro, correggi `final_remove_quote` copiando le parole esatte dal testo.

Rispondi con una decisione per OGNI proposta, con gli stessi numeri [S#].

TRASCRIZIONE ANNOTATA ({label}):
{annotated_text}

PROPOSTE:
{cuts_str}
"""
    try:
        result = motore.ollama_chat(prompt_verify, schema=motore.VERIFY_SCHEMA, temperature=0.0)
    except Exception as e:
        motore.log(f"      ⚠️  Fase 2 (revisione) fallita: {e}")
        return []

    decisions = result.get("decisions", []) or []
    _AI_DIAGNOSTICS.append({"label": label, "phase2": decisions})

    by_range = {}
    for c in proposed:
        try:
            key = tuple(sorted((int(c.get("start_seg")), int(c.get("end_seg")))))
        except (TypeError, ValueError):
            continue
        by_range.setdefault(key, c)

    seg_index = {s["idx"]: s for s in _CURRENT_SEGMENTS}
    intervals = []
    for d in decisions:
        try:
            si, ei = int(d.get("start_seg")), int(d.get("end_seg"))
        except (TypeError, ValueError):
            continue
        prop = by_range.get(tuple(sorted((si, ei))))
        if prop is None:
            continue
        verdict = str(d.get("verdict", "")).strip().upper()
        reason = (d.get("reason") or "").strip()
        if verdict not in ("RIMUOVI", "KEEP"):
            motore.log(f"      ⏭️  LASCIA [{si}-{ei}]: {reason[:80]}")
            continue

        quote = (d.get("final_remove_quote") or prop.get("remove_quote") or "").strip()
        category = prop.get("category", "errore")

        # 1) taglio di PRECISIONE: solo le parole citate (timestamp allineati)
        span = _locate_quote(_CURRENT_SEGMENTS, si, ei, quote)
        if span:
            st, et = span
            label_txt = f"AI {category}: \"{quote[:60]}\""
        else:
            # 2) fallback: confini di frase (comportamento storico)
            lo, hi = sorted((si, ei))
            if lo not in seg_index or hi not in seg_index:
                continue
            st, et = seg_index[lo]["start"], seg_index[hi]["end"]
            label_txt = f"AI {category} (frase intera): {reason[:60]}"

        dur = et - st
        if dur < MIN_CUT_SEC or dur > MAX_CUT_SEC:
            motore.log(f"      ⏭️  DROP [{si}-{ei}]: durata {dur:.2f}s fuori limiti")
            continue
        motore.log(f"      ✂️  RIMUOVI [{si}-{ei}] {category}: {st:.2f}s → {et:.2f}s ({dur:.2f}s)")
        intervals.append((st, et, label_txt))

    motore.log(f"      ✅ Fase 2: applicati {len(intervals)} tagli su {len(proposed)} proposti.")
    return intervals


# Frasi numerate correnti (usate da extract_bad_takes_ai per la mappatura)
_CURRENT_SEGMENTS = []


def run_bad_takes_ai(segments_annotated: str, label: str, segments: list) -> list:
    global _CURRENT_SEGMENTS
    _CURRENT_SEGMENTS = segments
    return extract_bad_takes_ai(segments_annotated, label)


# ═══════════════════════════════════════════════════════════════
# PIPELINE
# ═══════════════════════════════════════════════════════════════

def merge_intervals(intervals: list) -> list:
    if not intervals:
        return []
    iv = sorted((max(0.0, s), e) for s, e in _as_pairs(intervals))
    merged = [list(iv[0])]
    for s, e in iv[1:]:
        if s <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], e)
        else:
            merged.append([s, e])
    return merged


def _as_pairs(intervals: list) -> list:
    out = []
    for it in intervals:
        out.append((it[0], it[1]))
    return out


def invert_intervals(bad: list, total_duration: float) -> list:
    """Dai intervalli da TAGLIARE, ricava gli spezzoni BUONI."""
    if not bad:
        return [(0.0, total_duration)]
    merged = merge_intervals(bad)
    good = []
    curr = 0.0
    for st, et in merged:
        if st - curr >= MIN_CLIP_SEC:
            good.append((curr, st))
        curr = max(curr, et)
    if total_duration - curr >= MIN_CLIP_SEC:
        good.append((curr, total_duration))
    return good


def merge_good_intervals(good: list) -> list:
    """Ricompone spezzoni separati solo da micro-tagli, senza cambiare l'ordine."""
    if not good:
        return []
    merged = [list(good[0])]
    for start, end in good[1:]:
        gap = start - merged[-1][1]
        if gap <= MERGE_GOOD_GAP_SEC:
            merged[-1][1] = end
        else:
            merged.append([start, end])
    return [tuple(interval) for interval in merged
            if interval[1] - interval[0] >= MIN_CLIP_SEC]


def save_annotated_transcript(path: str, segments: list, words: list,
                             breaths: list, total_duration: float, base_name: str):
    """TXT leggibile per debug/verifica umana."""
    lines = [
        f"# Trascrizione Annotata — {base_name}",
        f"# Durata: {total_duration:.1f}s | Frasi: {len(segments)} | Parole: {len(words)} | Fiati: {len(breaths)}",
        "# Legenda: (PAUSA Xs) silenzio | (FILLER: x) esitazione | [S#] numero frase",
        "#" + "-" * 60,
        "",
    ]
    for seg in segments:
        lines.append(f"[S{seg['idx']}] ({seg['start']:.1f}s-{seg['end']:.1f}s) {seg['text']}")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


def analyze_video(input_path: str, base_name: str, output_dir: str,
                  params: dict | None = None, on_progress=None) -> dict:
    """FASE 1 (GUI): trascrizione + intervalli, SENZA tagliare.

    Ritorna dict JSON-serializzabile:
      {good: [[st,end]...], bad: [[st,end,reason]...],
       segments: [{idx,start,end,text}...], duration, words_count,
       annotated_path, input_path, work_dir}
    """
    with run_with_params(params):
        motore.log(f"\n{'='*55}")
        motore.log(f"   SGROSSATURA v4: {base_name}")
        motore.log(f"{'='*55}")

        os.makedirs(output_dir, exist_ok=True)
        backup_dir = motore.backup_output_media(output_dir)
        if backup_dir:
            motore.log(f"   🛡️ Output precedenti salvati in: {os.path.basename(backup_dir)}")

        # ── 1. TRASCRIZIONE (motore.py, con cache) ──
        motore.log("🎙️  Trascrizione (WhisperX + allineamento parola)...")
        _emit(on_progress, "transcribe", 5, "Trascrizione WhisperX...")
        data = motore.transcribe(input_path, output_dir, LANGUAGE)
        words = data["words"]
        total_duration = data["duration"]
        motore.log(f"   ⏱️  {total_duration:.1f}s | {len(words)} parole allineate")
        _emit(on_progress, "transcribe", 40, f"Trascritte {len(words)} parole")

        # ── 2. FRASI NUMERATE + ANNOTAZIONI ──
        segments = motore.group_into_segments(words)
        segments = _attach_words(segments, words)
        annotations = build_annotations(segments, words, data.get("events"))

        ann_path = os.path.join(output_dir, "trascrizione_annotata.txt")
        save_annotated_transcript(ann_path, segments, words, [], total_duration, base_name)
        motore.log(f"   📝 Annotazione salvata: {os.path.basename(ann_path)}")

        # ── 3. INTERVALLI DA TAGLIARE (v4: padding + clamp sulle parole) ──
        _emit(on_progress, "silences", 45, "Analisi silenzi...")
        bad_intervals, breaths = build_cut_plan(input_path, words, total_duration, on_progress)
        motore.log(f"   🔇 Silenzi > {MAX_SILENCE_SEC}s: conservo {SILENCE_KEEP_PAD:.2f}s attorno alla voce")
        motore.log(f"   🫁 Fiati tagliati: {len(breaths)}")
        motore.log(f"   🗣️  Filler inclusi nel piano")

        # 3d. AI bad takes (a chunk, per stare dentro il contesto)
        ai_debug_path = None
        if ENABLE_AI_BAD_TAKES:
            motore.log("   🤖 Analisi AI Bad Takes (2 fasi, ancorata al testo)...")
            _AI_DIAGNOSTICS.clear()
            n_chunks = (len(segments) - 1) // AI_CHUNK_SEGS + 1
            for n, ci in enumerate(range(0, len(segments), AI_CHUNK_SEGS), 1):
                chunk = segments[ci:ci + AI_CHUNK_SEGS]
                local_ann = motore.render_numbered(chunk, {s["idx"]: annotations.get(s["idx"], "") for s in chunk})
                label = f"frasi {chunk[0]['idx']}-{chunk[-1]['idx']}"
                _emit(on_progress, "ai", 55 + 35 * n / max(n_chunks, 1),
                      f"AI chunk {n}/{n_chunks}...")
                try:
                    cuts = run_bad_takes_ai(local_ann, label, chunk)
                except Exception as e:
                    motore.log(f"      ⚠️  Chunk {label}: {e}")
                    cuts = []
                for st, et, reason in cuts:
                    motore.log(f"      ✂️  AI Bad Take: {reason}  ({st:.1f}s → {et:.1f}s)")
                    bad_intervals.append((st, et, f"AI: {reason}"))
            ai_debug_path = dump_ai_diagnostics(output_dir)
            if ai_debug_path:
                motore.log(f"   🧪 Diagnostica AI: {os.path.basename(ai_debug_path)}")

        # ── 4. SPEZZONI BUONI ──
        cuts = merge_cuts_with_reasons(bad_intervals)
        good = merge_good_intervals(invert_intervals([(s, e) for s, e, _ in cuts], total_duration))
        removed = sum(e - s for s, e, _ in cuts)
        overlap = speech_overlap_seconds(cuts, words)
        motore.log(f"\n   🗑️  {len(cuts)} tagli ({removed:.1f}s rimossi, "
                   f"{100*removed/max(total_duration,1e-6):.1f}% del video)")
        motore.log(f"   ✅ {len(good)} spezzoni buoni estratti")
        if overlap > 0.5:
            motore.log(f"   ⚠️  Controllo sicurezza: {overlap:.2f}s di parlato dentro i tagli")
        else:
            motore.log(f"   🛡️ Nessun parlato dentro i tagli (ok)")
        _emit(on_progress, "done", 95, f"{len(good)} spezzoni buoni trovati")

        return {
            "good": [[round(s, 3), round(e, 3)] for s, e in good],
            "bad": [[round(s, 3), round(e, 3), r] for s, e, r in cuts],
            "cuts": [{"start": round(s, 3), "end": round(e, 3),
                      "dur": round(e - s, 3), "reason": r} for s, e, r in cuts],
            "removed_sec": round(removed, 3),
            "overlap_sec": round(overlap, 3),
            "segments": [{"idx": s["idx"], "start": round(s["start"], 3),
                          "end": round(s["end"], 3), "text": s.get("text", "")}
                         for s in segments],
            "duration": round(total_duration, 3),
            "words_count": len(words),
            "annotated_path": ann_path,
            "ai_debug_path": ai_debug_path,
            "input_path": input_path,
            "work_dir": output_dir,
        }


def cut_selected(input_path: str, good_intervals: list, output_dir: str,
                 on_progress=None) -> list:
    """FASE 2 (GUI): taglia SOLO gli intervalli selezionati. Ritorna i file creati."""
    items = list(good_intervals or [])
    if len(items) == 2 and all(isinstance(x, (int, float)) for x in items):
        items = [items]  # singolo intervallo piatto [s, e]
    pairs = []
    for it in items:
        s, e = float(it[0]), float(it[1])
        if e > s:
            pairs.append((s, e))
    total = len(pairs)
    created = []
    rendered = []
    base_name = os.path.splitext(os.path.basename(input_path))[0]
    _has_audio = motore.has_audio_stream(input_path)

    orig_log = motore.log
    def _cap(msg):
        orig_log(msg)
        if isinstance(msg, str) and msg.strip().startswith("->"):
            _emit(on_progress, "cut", None, msg.strip())
    motore.log = _cap
    try:
        for i, (st, et) in enumerate(pairs, 1):
            _emit(on_progress, "cut", 100 * (i - 1) / max(total, 1),
                  f"Taglio spezzone {i}/{total}...")
            dur = et - st
            out_clip = os.path.join(output_dir, f"{i:03d}_clip.mp4")
            cmd = [
                motore.FFMPEG_EXE, "-y",
                "-ss", str(round(st, 3)),
                "-t",  str(round(dur, 3)),
                "-i",  input_path,
                "-map", "0",
                "-c:v", "libx264", "-preset", "superfast", "-crf", "20",
                "-c:a", "aac", "-b:a", "192k",
            ]
            # Micro-fade ai bordi: evita click/secchi all'inizio e alla fine dello spezzone
            if AUDIO_FADE_SEC > 0 and dur > AUDIO_FADE_SEC * 3 and _has_audio:
                cmd += ["-af", (f"afade=t=in:st=0:d={AUDIO_FADE_SEC},"
                                f"afade=t=out:st={dur - AUDIO_FADE_SEC:.3f}:d={AUDIO_FADE_SEC}")]
            cmd.append(out_clip)
            res = subprocess.run(cmd, capture_output=True)
            ok = (res.returncode == 0 and os.path.isfile(out_clip)
                  and os.path.getsize(out_clip) > 0)
            if ok:
                got = motore.get_video_duration(out_clip)
                warn = "" if abs(got - dur) <= 0.30 else f"  ⚠️ resa {got:.2f}s (attesi {dur:.2f}s)"
                created.append(out_clip)
                rendered.append((out_clip, st, et, got))
                motore.log(f"   -> {i:03d}_clip.mp4 ({dur:.1f}s){warn}")
            else:
                err = (res.stderr or b"").decode(errors="ignore")[-160:]
                motore.log(f"   ❌ Errore clip {i}: {err}")
    finally:
        motore.log = orig_log

    # Report + anteprima: valgono sia per la GUI sia per la CLI
    if AUTO_REPORT and created:
        cuts, good, words_count, duration = _plan_from_workdir(output_dir)
        report = save_cut_report(output_dir, base_name, cuts, good,
                                 words_count, duration, rendered)
        motore.log(f"   📄 Report tagli: {os.path.basename(report)}")
    if MAKE_PREVIEW and len(created) >= 2:
        preview = make_preview(output_dir, created)
        if preview:
            motore.log(f"   🎞️  Anteprima unica: {os.path.basename(preview)}")

    _emit(on_progress, "done", 100, f"{len(created)} clip create")
    return created


def cut_clips(input_path: str, good_intervals: list, output_dir: str, cuts: list | None = None):
    """Compatibilità: delega al renderer unico (cut_selected)."""
    return cut_selected(input_path, good_intervals, output_dir)


def process_video(input_path: str, base_name: str, output_dir: str):
    """CLI classico: analisi completa + taglio di tutti gli spezzoni buoni.

    Report (`tagli_sgrossatura.txt/.json`) e anteprima vengono creati da
    `cut_selected`: stesso percorso usato dalla GUI.
    """
    result = analyze_video(input_path, base_name, output_dir)
    cut_selected(input_path, result["good"], output_dir)
    motore.log(f"\n🎉 Sgrossatura completata per {base_name}!")
    motore.log(f"📁 {output_dir}")


def main():
    if not os.path.exists(VIDEO_INPUT_DIR):
        os.makedirs(VIDEO_INPUT_DIR)

    videos = [f for f in os.listdir(VIDEO_INPUT_DIR)
              if f.lower().endswith(('.mp4', '.mkv', '.mov'))]
    if not videos:
        motore.log(f"❌ Nessun video in {VIDEO_INPUT_DIR}")
        return

    motore.log(f"📼 Trovati {len(videos)} video da sgrossare:")
    for v in videos:
        motore.log(f"   - {v}")
    motore.log("")

    for video in videos:
        input_path = os.path.join(VIDEO_INPUT_DIR, video)
        base_name  = os.path.splitext(video)[0]
        work_dir   = os.path.join(VIDEO_OUTPUT_DIR, base_name.replace(" ", "_") + "_SGROSSO")
        try:
            process_video(input_path, base_name, work_dir)
        except Exception as e:
            motore.log(f"❌ Errore su {video}: {e}")
            import traceback
            traceback.print_exc()


if __name__ == "__main__":
    main()
