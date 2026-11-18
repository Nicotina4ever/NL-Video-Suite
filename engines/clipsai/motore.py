# -*- coding: utf-8 -*-
"""
motore.py — Motore condiviso ClipsAI (trascrizione + AI)
=========================================================
Strato comune usato da `sgrossatore_raw.py` e `taglia_clip_per_editing.py`.

Trascrizione (engine professionale):
  - WhisperX = faster-whisper (large-v3, GPU int8) + VAD pyannote
  - Forced alignment wav2vec2 italiano (VOXPOPULI_ASR_BASE_10K_IT)
    -> timestamp parola precisi (<100 ms), ideali per sottotitoli e tagli puliti
  - Cache JSON locale: la trascrizione viene fatta UNA volta per video e riutilizzata

AI (Ollama locale):
  - /api/chat con STRUCTURED OUTPUTS (JSON schema) -> zero parse fragili
  - L'AI risponde con INDICI DI SEGMENTO (interi), non testo fuzzy:
    niente più matching approssimativo testo->timestamp

Tutto gira localmente, nessuna API cloud.
"""

import os
import sys
import json
import time
import hashlib
import platform
import subprocess
import shutil
from datetime import datetime

# ═══════════════════════════════════════════════════════════════
# CONFIGURAZIONE CENTRALE
# ═══════════════════════════════════════════════════════════════

BASE_DIR       = os.path.dirname(os.path.abspath(__file__))

OLLAMA_HOST    = "http://localhost:11434"
OLLAMA_MODEL   = "qwen2.5:14b-instruct"   # modello locale italiano (già installato)
OLLAMA_TIMEOUT = 900                      # secondi max per una risposta AI
NUM_CTX        = int(os.environ.get("CLIPSAI_NUM_CTX", "") or 8192)
# Context window per le chiamate AI. Note:
#  - qwen2.5:14b-instruct supporta al massimo 32768 token (Ollama taglia qui anche
#    se il server è configurato a 64k: il modello non arriva a 64k).
#  - con 8 GB di VRAM il modello è già parzialmente su CPU a 8192; alzare il ctx
#    sposta altra KV cache su CPU -> generazione più lenta (misurato: 32k = +4.7 GB su CPU).
#  - per dare più contesto all'AI conviene alzare le FRASI per chiamata (AI_CHUNK_SEGS
#    in sgrossatore_raw.py), non il context window.
# Override: set CLIPSAI_NUM_CTX=16384
TEMPERATURE    = 0.2                      # bassa = output deterministico e fedele

# WhisperX
WHISPER_MODEL  = "large-v3"               # miglior modello multilingua open (IT incluso)
WHISPER_DEVICE = "cuda"                   # RTX 4060 Ti; fallback CPU automatico
WHISPER_INT8   = True                     # int8 = ~3 GB VRAM, stessa qualità
# Modello ASR: copia locale CTranslate2 (nessun download).
# Override possibile senza toccare il codice:  set CLIPSAI_WHISPER_MODEL=<cartella modello>
WHISPER_MODEL_PATH = (os.environ.get("CLIPSAI_WHISPER_MODEL", "").strip()
                      or r"D:\Faster-Whisper-XXL\Faster-Whisper-XXL\_models\faster-whisper-large-v3")

# Cartella modelli condivisa (progetto Diarizzazione): contiene già AST e pyannote.
SHARED_MODELS_DIR = r"H:\Condiviso\Materiale Per Lezioni\Film e Cartoni\hazbin-hotel-s1\Diarizzazione\models"

# Eventi audio AST (fiati/risate/musica...) — OPT-IN via ENABLE_AUDIO_EVENTS.
# Modulo audio_events.py (copia dal progetto Diarizzazione, categoria "Fiato").
# Non invalida la cache: gli eventi si calcolano in lazy e si aggiungono al JSON.
ENABLE_AUDIO_EVENTS = False
AST_MODEL        = "MIT/ast-finetuned-audioset-10-10-0.4593"
AST_WINDOW_SEC   = 5.0    # finestre corte = granularità migliore per i fiati
AST_HOP_SEC      = 2.5
AST_BATCH        = 4
# Cache AST: si usa la PRIMA cartella che contiene davvero il modello.
# Ordine: copia del progetto -> cartella condivisa su H: (già presente) -> default (scarica).
# Override esplicito:  set CLIPSAI_AST_CACHE=<cartella cache>
_AST_CACHE_CANDIDATES = (
    os.path.join(BASE_DIR, "models", "hf"),
    os.path.join(SHARED_MODELS_DIR, "hf", "hub"),
    os.path.join(SHARED_MODELS_DIR, "hf"),
)
_AST_DIR_NAME = "models--MIT--ast-finetuned-audioset-10-10-0.4593"


def _ast_cache_has_model(cache_dir: str) -> bool:
    """True solo se la cache contiene davvero uno snapshot con config + pesi."""
    snapshots = os.path.join(cache_dir, _AST_DIR_NAME, "snapshots")
    if not os.path.isdir(snapshots):
        return False
    for root, _dirs, files in os.walk(snapshots):
        if "config.json" in files and any(
                f.endswith((".safetensors", ".bin")) for f in files):
            return True
    return False


AST_CACHE_DIR = (os.environ.get("CLIPSAI_AST_CACHE", "").strip()
                 or next((p for p in _AST_CACHE_CANDIDATES if _ast_cache_has_model(p)),
                         _AST_CACHE_CANDIDATES[0]))

FFMPEG_EXE     = "ffmpeg"
FFPROBE_EXE    = "ffprobe"

# Suddivisione in "frasi" numerate per i prompt AI
AI_SEG_MAX_SEC   = 12.0    # durata max di un segmento numerato (secondi)
AI_SEG_MAX_WORDS = 16      # parole max per segmento numerato

# ═══════════════════════════════════════════════════════════════
# UTILITÀ
# ═══════════════════════════════════════════════════════════════

def log(msg: str):
    try:
        print(msg, flush=True)
    except UnicodeEncodeError:
        # Console Windows cp1252: riprova con UTF-8 (chcp 65001 equivale)
        import io
        sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
        print(msg, flush=True)


def get_video_duration(filepath: str) -> float:
    try:
        res = subprocess.run(
            [FFPROBE_EXE, "-v", "error", "-show_entries", "format=duration",
             "-of", "default=noprint_wrappers=1:nokey=1", filepath],
            capture_output=True, text=True, timeout=60)
        return float(res.stdout.strip())
    except Exception:
        return 0.0


def has_audio_stream(filepath: str) -> bool:
    """True se il file ha almeno una traccia audio (per evitare filtri audio a vuoto)."""
    try:
        res = subprocess.run(
            [FFPROBE_EXE, "-v", "error", "-select_streams", "a",
             "-show_entries", "stream=index", "-of", "csv=p=0", filepath],
            capture_output=True, text=True, timeout=60)
        return bool((res.stdout or "").strip())
    except Exception:
        return False


def video_fingerprint(filepath: str) -> str:
    """Identità stabile di un video (permette di invalidare la cache al cambio file)."""
    st = os.stat(filepath)
    return f"{st.st_size}-{st.st_mtime_ns}"


def _import_whisperx():
    """Importa whisperx con path di sicurezza (evita problemi di DLL su Windows)."""
    os.environ.setdefault("KMP_DUPLICATE_LIB_OK", "TRUE")
    try:
        import whisperx
        return whisperx
    except Exception:
        # Se python_env non è quello attivo, prova il site-packages del progetto
        sp = os.path.join(BASE_DIR, "python_env", "Lib", "site-packages")
        if os.path.isdir(sp) and sp not in sys.path:
            sys.path.insert(0, sp)
        import whisperx
        return whisperx


def _whisper_model_source() -> str:
    """Restituisce il modello locale gia' presente, senza fallback a download online.

    Override: variabile d'ambiente CLIPSAI_WHISPER_MODEL (cartella modello CTranslate2).
    """
    if not os.path.isdir(WHISPER_MODEL_PATH):
        raise FileNotFoundError(
            f"Modello Whisper locale non trovato: {WHISPER_MODEL_PATH} "
            f"(puoi indicarne un altro con CLIPSAI_WHISPER_MODEL)"
        )
    model_bin = os.path.join(WHISPER_MODEL_PATH, "model.bin")
    if not os.path.isfile(model_bin):
        raise FileNotFoundError(
            f"Modello Whisper locale incompleto (manca model.bin): {WHISPER_MODEL_PATH}"
        )
    return WHISPER_MODEL_PATH


def _cache_signature(language: str) -> dict:
    return {
        "language": language,
        "model": WHISPER_MODEL,
        "model_path": WHISPER_MODEL_PATH,
        "engine": "whisperx-cache-v2",
    }


def backup_output_media(output_dir: str) -> str | None:
    """Sposta i media precedenti in un backup datato prima di un nuovo run."""
    media = [name for name in os.listdir(output_dir)
             if name.lower().endswith((".mp4", ".srt"))]
    if not media:
        return None
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
    backup_dir = os.path.join(output_dir, f"_backup_{stamp}")
    os.makedirs(backup_dir, exist_ok=False)
    for name in media:
        shutil.move(os.path.join(output_dir, name), os.path.join(backup_dir, name))
    return backup_dir
# ═══════════════════════════════════════════════════════════════
# TRASCRIZIONE PROFESSIONALE (WhisperX)
# ═══════════════════════════════════════════════════════════════

def transcribe(video_path: str, work_dir: str, language: str = "it",
               force: bool = False) -> dict:
    """
    Trascrive il video (una volta, poi usa la cache) e restituisce:
      {
        "language": "it",
        "duration": 1954.2,
        "segments": [ {"id":1,"start":..,"end":..,"text":"..","words":[{"word","start","end"}]} ],
        "words": [ ... lista piatta di parole ordinate per tempo ... ],
        "events": [ ... eventi audio AST, solo se ENABLE_AUDIO_EVENTS ... ]
      }
    """
    os.makedirs(work_dir, exist_ok=True)
    base = os.path.splitext(os.path.basename(video_path))[0]
    cache_path = os.path.join(work_dir, f"{base}_trascrizione.json")
    fp = video_fingerprint(video_path)

    if not force and os.path.exists(cache_path):
        try:
            with open(cache_path, "r", encoding="utf-8") as f:
                cached = json.load(f)
            # NOTA: la return è FUORI dal with — su Windows un file aperto
            # non si può sostituire (os.replace darebbe PermissionError).
            if (cached.get("_fingerprint") == fp
                    and cached.get("_signature") == _cache_signature(language)
                    and cached.get("words")):
                log(f"   ⚡ Cache trascrizione trovata: {os.path.basename(cache_path)}")
                cached["_cache_hit"] = True
                cached.setdefault("_execution", {"device": "unknown", "cache_hit": True})
                return _maybe_add_audio_events(video_path, cache_path, cached)
        except Exception:
            pass  # cache corrotta -> riscrive

    whisperx = _import_whisperx()
    import torch

    duration = get_video_duration(video_path)

    # ── 1. Carica modello ASR (large-v3, GPU int8) ──────────────
    model_source = _whisper_model_source()
    log(f"   🧠 Carico Whisper {WHISPER_MODEL} locale ({WHISPER_DEVICE} int8)...")
    t0 = time.time()
    asr_device = WHISPER_DEVICE
    try:
        model_asr = whisperx.load_model(
            model_source, asr_device,
            compute_type="int8" if WHISPER_INT8 else "float16",
            local_files_only=True,
        )
    except Exception as e:
        if not globals().get("WHISPER_ALLOW_CPU_FALLBACK", True):
            raise RuntimeError(f"WhisperX {asr_device} non avviato: {e}. Nessun ripiego in CPU: correggi la GPU o scegli CPU esplicitamente.") from e
        log(f"   ⚠️  GPU non disponibile ({type(e).__name__}); passo a CPU.")
        asr_device = "cpu"
        model_asr = whisperx.load_model(
            model_source, "cpu", compute_type="int8", local_files_only=True
        )
    log(f"      OK in {time.time()-t0:.0f}s")

    # ── 2. Carica audio 16kHz mono ──────────────────────────────
    t0 = time.time()
    audio = whisperx.load_audio(video_path)
    log(f"   🎧 Audio caricato: {len(audio)/16000:.0f}s ({time.time()-t0:.0f}s)")

    # ── 3. Trascrizione batch + VAD ─────────────────────────────
    t0 = time.time()
    log(f"   📝 Trascrizione WhisperX (VAD + batch)...")
    result = model_asr.transcribe(audio, batch_size=globals().get("WHISPER_BATCH_SIZE", 16), language=language)
    log(f"      {len(result['segments'])} segmenti grezzi in {time.time()-t0:.0f}s")

    # libera GPU prima dell'allineamento
    del model_asr
    if torch.cuda.is_available():
        torch.cuda.empty_cache()

    # ── 4. Forced alignment wav2vec2 (timestamp parola <100ms) ──
    align_device = asr_device
    t0 = time.time()
    log(f"   🎯 Allineamento parola (wav2vec2 {language.upper()}, {align_device})...")
    alignment_ok = False
    try:
        align_model, align_meta = whisperx.load_align_model(language_code=language, device=align_device)
        result = whisperx.align(result["segments"], align_model, align_meta, audio,
                                align_device, return_char_alignments=False)
        alignment_ok = True
    except Exception as e:
        log(f"      ⚠️  Allineamento non riuscito ({e}); uso timestamp CTranslate2.")
    if alignment_ok:
        log(f"      OK in {time.time()-t0:.0f}s")

    # ── 5. Normalizza e salva ───────────────────────────────────
    segments = []
    words = []
    for i, seg in enumerate(result.get("segments", []), 1):
        seg_words = []
        for w in seg.get("words", []):
            if w is None or "start" not in w or "end" not in w:
                continue
            if (w["end"] - w["start"]) < 0.005:      # parola non allineata: skip
                continue
            w2 = {"word": (w.get("word") or "").strip(),
                  "start": round(float(w["start"]), 3),
                  "end": round(float(w["end"]), 3)}
            if not w2["word"]:
                continue
            seg_words.append(w2)
            words.append(w2)
        if seg_words:
            segments.append({
                "id": i,
                "start": round(float(seg["start"]), 3),
                "end": round(float(seg_words[-1]["end"]), 3),
                "text": seg.get("text", "").strip(),
                "words": seg_words,
            })

    if not words:
        raise RuntimeError("Trascrizione vuota: il video non contiene voce parlata?")

    out = {
        "_fingerprint": fp,
        "_signature": _cache_signature(language),
        "_engine": f"whisperx/{WHISPER_MODEL}/int8+wav2vec2-{language}",
        "_execution": {"device": asr_device, "alignment_device": align_device, "cpu_fallback": asr_device != WHISPER_DEVICE},
        "language": result.get("language", language),
        "duration": duration or words[-1]["end"],
        "segments": segments,
        "words": words,
    }
    cache_tmp = f"{cache_path}.tmp"
    with open(cache_tmp, "w", encoding="utf-8") as f:
        json.dump(out, f, ensure_ascii=False)
    os.replace(cache_tmp, cache_path)

    log(f"   ✅ Trascrizione salvata: {os.path.basename(cache_path)} "
        f"({len(segments)} frasi, {len(words)} parole)")
    return _maybe_add_audio_events(video_path, cache_path, out, audio=audio)


# ═══════════════════════════════════════════════════════════════
# EVENTI AUDIO AST (fiati/risate/musica...) — OPT-IN
# ═══════════════════════════════════════════════════════════════

def _maybe_add_audio_events(video_path: str, cache_path: str, out: dict,
                            audio=None) -> dict:
    """
    Step AST (solo se ENABLE_AUDIO_EVENTS): calcola gli eventi audio con
    audio_events.detect_audio_events, li attacca a out["events"] e riscrive
    la cache. Fallimento = warning + trascrizione invariata (zero impatto).
    """
    if not ENABLE_AUDIO_EVENTS or out.get("events"):
        return out
    try:
        if audio is None:
            whisperx = _import_whisperx()
            audio = whisperx.load_audio(video_path)
        import torch
        device = "cuda" if (WHISPER_DEVICE == "cuda"
                            and torch.cuda.is_available()) else "cpu"
        os.makedirs(AST_CACHE_DIR, exist_ok=True)
        if BASE_DIR not in sys.path:
            sys.path.insert(0, BASE_DIR)
        import audio_events as _ae
        log("   🔊 Rilevamento eventi audio (AST: fiati/risate/musica)...")
        t0 = time.time()
        events = _ae.detect_audio_events(
            audio=audio, sample_rate=16000, model_name=AST_MODEL,
            cache_dir=AST_CACHE_DIR, device=device,
            window_seconds=AST_WINDOW_SEC, hop_seconds=AST_HOP_SEC,
            batch_size=AST_BATCH)
        log(f"      {len(events)} eventi audio in {time.time()-t0:.0f}s")
        out["events"] = events
        tmp = f"{cache_path}.tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False)
        os.replace(tmp, cache_path)
    except Exception as e:
        log(f"      ⚠️  Eventi audio saltati ({type(e).__name__}: {e})")
    return out


def events_to_annotations(events: list, segments: list) -> dict:
    """
    Eventi AST -> {idx: " [FIATO] [MUSICA]"} per motore.render_numbered().
    Stessa regola di overlap del progetto Diarizzazione (>=0.25s o >=20%).
    """
    ann = {}
    for s in segments:
        st, et = float(s["start"]), float(s["end"])
        tags = []
        for ev in (events or []):
            if ev.get("kind") == "pause":
                continue
            es, ee = float(ev.get("start", 0.0)), float(ev.get("end", 0.0))
            overlap = max(0.0, min(et, ee) - max(st, es))
            if overlap >= 0.25 or ((et > st) and overlap / (et - st) >= 0.2):
                label = f"({str(ev.get('label', '')).upper()})"
                if label != "()" and label not in tags:
                    tags.append(label)
        if tags:
            ann[s["idx"]] = " " + " ".join(tags)
    return ann


# ═══════════════════════════════════════════════════════════════
# FRASI NUMERATE PER I PROMPT AI
# ═══════════════════════════════════════════════════════════════

def group_into_segments(words: list, max_sec: float = AI_SEG_MAX_SEC,
                        max_words: int = AI_SEG_MAX_WORDS) -> list:
    """
    Raggruppa le parole in frasi numerate [S1]..[Sn] per i prompt AI.
    Criteri: durata <= max_sec oppure parole <= max_words.
    Ogni frase: {idx, start, end, text}
    """
    segs = []
    curr = []
    for w in words:
        if curr:
            last = curr[-1]
            if (w["start"] - last["end"]) > max_sec or \
               (w["end"] - curr[0]["start"]) > max_sec or \
               len(curr) >= max_words:
                segs.append(_make_seg(segs, curr))
                curr = []
        curr.append(w)
    if curr:
        segs.append(_make_seg(segs, curr))
    return segs


def _make_seg(segs, curr):
    idx = len(segs) + 1
    text = " ".join(w["word"] for w in curr)
    return {"idx": idx, "start": curr[0]["start"], "end": curr[-1]["end"], "text": text}


def render_numbered(segments: list, annotations: dict | None = None) -> str:
    """
    Testo numerato per l'AI:  [S12] il testo della frase
    `annotations` opzionale: {idx: " [PAUSA 1.5s]" } marcatori extra dopo l'indice.
    """
    annotations = annotations or {}
    lines = []
    for s in segments:
        tag = f"[S{s['idx']}]"
        extra = annotations.get(s["idx"], "")
        lines.append(f"{tag}{extra} {s['text']}")
    return "\n".join(lines)


def seg_bounds(segments: list, start_idx: int, end_idx: int):
    """Converte indici di segmento in (start_sec, end_sec), validando l'ordine."""
    if not (1 <= start_idx <= len(segments)) or not (1 <= end_idx <= len(segments)):
        return None, None
    if end_idx < start_idx:
        start_idx, end_idx = end_idx, start_idx
    s = segments[start_idx - 1]
    e = segments[end_idx - 1]
    return s["start"], e["end"]


# ═══════════════════════════════════════════════════════════════
# AI OLAMA — CHAT CON OUTPUT STRUTTURATO
# ═══════════════════════════════════════════════════════════════

def ollama_running() -> bool:
    try:
        import requests
        r = requests.get(f"{OLLAMA_HOST}/api/version", timeout=5)
        return r.status_code == 200
    except Exception:
        return False


def ollama_has_model(model: str = OLLAMA_MODEL) -> bool:
    try:
        import requests
        r = requests.get(f"{OLLAMA_HOST}/api/tags", timeout=10)
        names = [m.get("name", "") for m in r.json().get("models", [])]
        return any(n == model or n.startswith(model + ":") for n in names)
    except Exception:
        return False


def ollama_chat(prompt: str, schema: dict | None = None,
                system: str | None = None,
                model: str | None = None,
                retries: int = 3,
                temperature: float | None = None) -> dict | str:
    """
    Chiamata AI con OUTPUT STRUTTURATO (JSON schema) via /api/chat.
    - schema presente  -> ritorna dict già parsato (garantito conforme)
    - schema None      -> ritorna stringa libera
    Con retry automatici (backoff) su timeout/errore di rete.
    """
    import requests

    if model is None:
        model = OLLAMA_MODEL

    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})

    payload = {
        "model": model,
        "messages": messages,
        "stream": False,
        "options": {"num_ctx": NUM_CTX,
                    "temperature": TEMPERATURE if temperature is None else temperature},
    }
    if schema is not None:
        payload["format"] = schema
        payload["schema"] = schema      # alcuni client Ollama richiedono entrambi
        # Il campo canonico è "format"; "schema" è ignorato se non supportato.
        payload.pop("schema", None)     # mantieni solo il campo canonico

    last_err = None
    for attempt in range(1, retries + 1):
        try:
            r = requests.post(f"{OLLAMA_HOST}/api/chat", json=payload,
                              timeout=OLLAMA_TIMEOUT)
            r.raise_for_status()
            content = r.json().get("message", {}).get("content", "")
            if schema is None:
                return content
            return json.loads(content)
        except json.JSONDecodeError as e:
            last_err = e
            log(f"      ⚠️  JSON non valido al tentativo {attempt}: {e}")
        except Exception as e:
            last_err = e
            log(f"      ⚠️  Errore AI al tentativo {attempt}: {e}")
        time.sleep(3 * attempt)

    raise RuntimeError(f"Ollama ha fallito dopo {retries} tentativi: {last_err}")


# ═══════════════════════════════════════════════════════════════
# SCHEMI JSON CONDIVISI
# ═══════════════════════════════════════════════════════════════

CLIP_SCHEMA = {
    "type": "object",
    "properties": {
        "clips": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Titolo accattivante in italiano, max 8 parole"},
                    "start_seg": {"type": "integer", "description": "Indice della PRIMA frase [S#] della clip"},
                    "end_seg": {"type": "integer", "description": "Indice dell'ULTIMA frase [S#] della clip"},
                    "score": {"type": "integer", "minimum": 1, "maximum": 10,
                              "description": "Punteggio di impatto/viralità 1-10"},
                    "reason": {"type": "string", "description": "Perché questa clip funziona"}
                },
                "required": ["title", "start_seg", "end_seg", "score"]
            }
        }
    },
    "required": ["clips"]
}

CUT_SCHEMA = {
    "type": "object",
    "properties": {
        "proposed_cuts": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "category": {"type": "string",
                                 "enum": ["false_partenza", "ripetizione", "correzione", "inciampo"],
                                 "description": "Tipo di errore di registrazione rilevato"},
                    "start_seg": {"type": "integer", "description": "Prima frase [S#] che contiene la parte da rimuovere"},
                    "end_seg": {"type": "integer", "description": "Ultima frase [S#] che contiene la parte da rimuovere"},
                    "remove_quote": {"type": "string",
                                     "description": "Parole ESATTE da rimuovere, copiate letteralmente e in ordine dal testo (max 20 parole, sequenza contigua). Non includere la versione corretta."},
                    "keep_quote": {"type": "string",
                                   "description": "Parole della versione corretta che resta (facoltativo)"},
                    "confidence": {"type": "integer", "minimum": 0, "maximum": 100,
                                   "description": "Quanto sei sicuro che sia un errore di registrazione"},
                    "reason": {"type": "string", "description": "Spiegazione breve"}
                },
                "required": ["category", "start_seg", "end_seg", "remove_quote", "confidence"]
            }
        }
    },
    "required": ["proposed_cuts"]
}

VERIFY_SCHEMA = {
    "type": "object",
    "properties": {
        "decisions": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "start_seg": {"type": "integer"},
                    "end_seg": {"type": "integer"},
                    "verdict": {"type": "string", "enum": ["RIMUOVI", "LASCIA"],
                                "description": "RIMUOVI = togli dal video le parole citate (errore di registrazione); LASCIA = non toccare nulla"},
                    "final_remove_quote": {"type": "string",
                                           "description": "OBbligatoria: le parole ESATTE da rimuovere, copiate dal testo (se la proposta era gia' corretta, ripetila qui)"},
                    "reason": {"type": "string", "description": "Perche' hai deciso cosi'"}
                },
                "required": ["start_seg", "end_seg", "verdict", "final_remove_quote", "reason"]
            }
        }
    },
    "required": ["decisions"]
}

MOMENTS_SCHEMA = {
    "type": "object",
    "properties": {
        "moments": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "topic": {"type": "string"},
                    "start_seg": {"type": "integer"},
                    "end_seg": {"type": "integer"},
                    "score": {"type": "integer", "minimum": 1, "maximum": 10}
                },
                "required": ["title", "start_seg", "end_seg", "score"]
            }
        }
    },
    "required": ["moments"]
}

REELS_SCHEMA = {
    "type": "object",
    "properties": {
        "reels": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "hook": {"type": "string", "description": "Gancio iniziale di 1 frase, alto impatto"},
                    "moment_numbers": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Numeri dei momenti [M#] in ordine di montaggio"
                    }
                },
                "required": ["title", "moment_numbers"]
            }
        }
    },
    "required": ["reels"]
}


# ═══════════════════════════════════════════════════════════════
# SOTTOTITOLI (SRT) — da parole allineate
# ═══════════════════════════════════════════════════════════════

def format_srt_time(seconds: float) -> str:
    ms = int(round((seconds - int(seconds)) * 1000))
    if ms >= 1000:
        seconds += 1
        ms = 0
    s = int(seconds)
    h, m = divmod(s, 3600)
    m, sec = divmod(m, 60)
    return f"{h:02d}:{m:02d}:{sec:02d},{ms:03d}"


def words_to_cues(words: list, start: float, end: float,
                  max_words: int = None, max_sec: float = None) -> list:
    """
    Raggruppa le parole in cue di sottotitolo.
    max_words=None -> frasi naturali (pausa > 0.45s o <=14 parole)
    max_words=N    -> rigido (es. hormozi: N parole per cue)
    Ritorna [{start, end, text}]
    """
    sel = [w for w in words if w["start"] < end and w["end"] > start]
    if not sel:
        return []
    cues = []
    curr = []
    for w in sel:
        if curr:
            gap = w["start"] - curr[-1]["end"]
            dur = w["end"] - curr[0]["start"]
            split = (max_words and len(curr) >= max_words) or \
                    (max_sec and dur > max_sec) or \
                    (not max_words and gap > 0.45) or \
                    (not max_words and len(curr) >= 14)
            if split:
                cues.append(_cue(curr))
                curr = []
        curr.append(w)
    if curr:
        cues.append(_cue(curr))
    return cues


def _cue(curr):
    return {"start": curr[0]["start"], "end": curr[-1]["end"],
            "text": " ".join(w["word"] for w in curr).strip()}


def save_srt(cues: list, path: str, offset: float = 0.0, upper: bool = False):
    with open(path, "w", encoding="utf-8") as f:
        for i, c in enumerate(cues, 1):
            text = c["text"].upper().strip() if upper else c["text"].strip()
            if not text:
                continue
            f.write(f"{i}\n")
            f.write(f"{format_srt_time(max(0.0, c['start'] - offset))} --> "
                    f"{format_srt_time(max(0.01, c['end'] - offset))}\n")
            f.write(f"{text}\n\n")
    return len(cues)
