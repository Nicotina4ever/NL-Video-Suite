"""Invoke the suite-owned ClipsAI engine without its old Flask server.

The request and all writable output belong to NL Video Suite. The original
engine snapshot and shared interpreter live inside NL Video Suite. Model
weights can remain in a separately configured, shared local cache.
"""
import json
import os
from pathlib import Path
import sys
import traceback

sys.stdout.reconfigure(encoding="utf-8", errors="replace")
sys.stderr.reconfigure(encoding="utf-8", errors="replace")
ALIGNMENT_STATE = {"loaded": False, "completed": False, "model_path": os.environ.get("NL_ALIGNMENT_MODEL_PATH")}
USED_MODELS = set()
PROVIDER_FAILURES = []
MAX_RESPONSE_BYTES = 2 * 1024 * 1024


class ProviderError(RuntimeError):
    """Configuration, protocol or permanent server error: do not retry silently."""


def ollama_payload(motore, request, prompt, schema=None, system=None,
                   model=None, temperature=None):
    """Suite request policy, leaving the vendored engine and service untouched."""
    execution = request.get("ollama_execution", "cpu")
    if execution not in ("cpu", "auto"):
        raise ValueError("Unsupported Ollama execution policy: " + str(execution))
    options = {"num_ctx": motore.NUM_CTX, "num_predict": 4096,
               "temperature": motore.TEMPERATURE if temperature is None else temperature}
    if execution == "cpu":
        options["num_gpu"] = 0
    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})
    payload = {"model": model or request.get("model") or motore.OLLAMA_MODEL,
               "messages": messages, "stream": False, "think": False,
               "keep_alive": 0, "options": options}
    if schema is not None:
        payload["format"] = schema
    return payload


def provider_payload(motore, request, prompt, schema=None, system=None,
                     model=None, temperature=None):
    provider = request.get("llm_provider", "ollama")
    if provider == "ollama":
        return motore.OLLAMA_HOST.rstrip("/") + "/api/chat", ollama_payload(
            motore, request, prompt, schema, system, model, temperature)
    if provider != "openai_compatible":
        raise ProviderError("Provider LLM non supportato: " + str(provider))
    base = request.get("openai_base_url", "http://127.0.0.1:8080/v1").rstrip("/")
    if not base.endswith("/v1"):
        base += "/v1"
    messages = []
    if system:
        messages.append({"role": "system", "content": system})
    messages.append({"role": "user", "content": prompt})
    payload = {"model": model or request.get("model") or motore.OLLAMA_MODEL,
               "messages": messages, "stream": False, "max_tokens": 4096,
               "temperature": motore.TEMPERATURE if temperature is None else temperature}
    if schema is not None:
        payload["response_format"] = {"type": "json_schema", "json_schema": {
            "name": "nl_suite_response", "strict": True, "schema": schema}}
    return base + "/chat/completions", payload


def read_provider_response(response, provider):
    if response.status_code in (401, 403):
        raise ProviderError("Il server LLM richiede autenticazione o rifiuta l'accesso. Supportati server locali/LAN senza API key; nessuna credenziale inviata.")
    data = bytearray()
    for chunk in response.iter_content(chunk_size=16384):
        if len(data) + len(chunk) > MAX_RESPONSE_BYTES:
            raise ProviderError("Risposta LLM oltre il limite di 2 MiB")
        data.extend(chunk)
    try:
        raw = json.loads(data)
    except (ValueError, UnicodeDecodeError) as error:
        raise ProviderError(f"Risposta LLM HTTP {response.status_code} non JSON") from error
    if not isinstance(raw, dict):
        raise ProviderError("Risposta LLM non valida: atteso un oggetto JSON")
    if response.status_code >= 400 or "error" in raw:
        detail = raw.get("error", "richiesta rifiutata")
        raise ProviderError(f"Server LLM HTTP {response.status_code}: {str(detail)[:600]}. Verifica modello, chat template e supporto JSON/schema.")
    if provider == "openai_compatible":
        choices = raw.get("choices") or []
        first = choices[0] if choices else {}
        content = (first.get("message") or {}).get("content")
        stop = first.get("finish_reason")
    else:
        content = (raw.get("message") or {}).get("content")
        stop = raw.get("done_reason")
    if stop == "length":
        raise ProviderError("Risposta LLM troncata dal limite di token: riduci la richiesta o usa un modello/server con contesto sufficiente")
    if not isinstance(content, str) or not content.strip():
        raise ProviderError("Il server LLM non ha restituito contenuto testuale; verifica supporto chat e JSON del modello")
    return content


def canonical_engine_transcript(transcript):
    """Lossless ms->seconds adapter. No new ASR or fabricated word alignment."""
    def item(value, word=False):
        start, end = value["start_ms"], value["end_ms"]
        if not isinstance(start, int) or not isinstance(end, int) or start < 0 or end <= start:
            raise ValueError("Intervallo trascrizione canonica non valido")
        return {"start": start / 1000, "end": end / 1000,
                "word" if word else "text": value["text"]}
    words = [item(w, True) for w in transcript.get("words", [])]
    segments = []
    for index, raw in enumerate(transcript.get("segments", [])):
        segment = dict(item(raw), id=index + 1)
        segment["words"] = [w for w in words if segment["start"] <= w["start"] < segment["end"]]
        segments.append(segment)
    return {"language": transcript["language"], "segments": segments, "words": words,
            "duration": max([v["end"] for v in segments + words], default=0),
            "events": [], "_canonical_transcript_id": transcript["id"]}


def execute(request):
    source = Path(request["source_root"]).resolve(strict=True)
    sys.path.insert(0, str(source))
    import motore

    original_log = motore.log
    def live_log(message):
        original_log(message)
        if request.get('progress_path'):
            target = Path(request['progress_path'])
            temporary = target.with_suffix('.tmp')
            temporary.write_text(json.dumps({'stage': str(message), 'device': request.get('device')}, ensure_ascii=False), encoding='utf-8')
            os.replace(str(temporary), str(target))
    motore.log = live_log
    operation = request["operation"]
    if operation == "diagnose":
        import importlib.util
        return {"python": sys.version, "source": str(source),
                "whisperx": importlib.util.find_spec("whisperx") is not None,
                "asr_model": os.path.isdir(motore.WHISPER_MODEL_PATH)}
    video = str(Path(request["input_path"]).resolve(strict=True))
    work = Path(request["work_dir"]).resolve()
    work.mkdir(parents=True, exist_ok=True)
    language = request.get("language", "it")
    params = request.get("params") or {}
    canonical = request.get("canonical_transcript")
    if canonical and operation != "transcribe":
        shared = canonical_engine_transcript(canonical)
        if language not in ("auto", canonical["language"]):
            raise ValueError("Lingua diversa dalla trascrizione esistente: scegli la lingua originale o ritrascrivi esplicitamente")
        def reuse_transcript(video_path, work_dir, language="it", force=False):
            if force:
                raise ValueError("La pipeline non sovrascrive l'ASR condivisa: ritrascrivi dal pannello Longform")
            if Path(video_path).resolve() != Path(video).resolve():
                raise ValueError("Trascrizione condivisa richiesta per un media differente")
            cache = Path(work_dir) / (Path(video_path).stem + "_trascrizione.json")
            cache.parent.mkdir(parents=True, exist_ok=True)
            cache.write_text(json.dumps(shared, ensure_ascii=False), encoding="utf-8")
            return shared
        motore.transcribe = reuse_transcript
    motore.OLLAMA_HOST = request.get("ollama_url", motore.OLLAMA_HOST)
    if request.get("model"):
        motore.OLLAMA_MODEL = request["model"]
        params["OLLAMA_MODEL"] = request["model"]
    # Never stop external processes or unload another app's active models.
    motore.WHISPER_DEVICE = request.get("device", "cpu")
    motore.WHISPER_ALLOW_CPU_FALLBACK = False
    motore.WHISPER_BATCH_SIZE = 4 if motore.WHISPER_DEVICE == "cuda" else 8
    # The portable Italian aligner is explicit and local, rather than the
    # implicit non-commercial VoxPopuli bundle selected by WhisperX defaults.
    # Keep lazy loading so a transcript cache hit does not import all of torch.
    original_whisper_import = motore._import_whisperx
    def portable_whisper_import():
        whisperx = original_whisper_import()
        original_align = whisperx.load_align_model
        def portable_align(*args, **kwargs):
            language_code = kwargs.get("language_code") or (args[0] if args else "it")
            if language_code != "it":
                raise RuntimeError("Allineatore portable configurato solo per italiano: nessun download automatico")
            alignment = Path(os.environ["NL_ALIGNMENT_MODEL_PATH"])
            if not (alignment / "pytorch_model.bin").is_file():
                raise RuntimeError("Allineatore italiano locale assente: esegui Verifica_PC.cmd")
            kwargs["model_name"] = str(alignment)
            try:
                result = original_align(*args, **kwargs)
                ALIGNMENT_STATE["loaded"] = True
                return result
            except Exception as error:
                ALIGNMENT_STATE["error"] = str(error)
                raise
        whisperx.load_align_model = portable_align
        original_alignment = whisperx.align
        def tracked_alignment(*args, **kwargs):
            result = original_alignment(*args, **kwargs)
            ALIGNMENT_STATE["completed"] = True
            return result
        whisperx.align = tracked_alignment
        return whisperx
    motore._import_whisperx = portable_whisper_import
    def tracked_chat(prompt, schema=None, system=None, model=None, retries=3,
                     temperature=None):
        # The original API has no options argument. Adapt that API here rather
        # than editing engines or monkey-patching global requests functions.
        import requests
        import time
        provider = request.get("llm_provider", "ollama")
        url, payload = provider_payload(motore, request, prompt, schema, system, model, temperature)
        if len(json.dumps(payload).encode("utf-8")) > 1024 * 1024:
            raise ProviderError("Richiesta LLM troppo grande (massimo 1 MiB)")
        if provider == "ollama":
            USED_MODELS.add(payload["model"])
        last_error = None
        for attempt in range(1, retries + 1):
            try:
                session = requests.Session()
                # No implicit .netrc credentials or environment HTTP proxy for
                # this unauthenticated local/LAN provider mode.
                session.trust_env = False
                try:
                    response = session.post(url, json=payload, timeout=(10, min(motore.OLLAMA_TIMEOUT, 900)), stream=True)
                    try:
                        content = read_provider_response(response, provider)
                    finally:
                        response.close()
                finally:
                    session.close()
                return json.loads(content) if schema is not None else content
            except ProviderError as error:
                PROVIDER_FAILURES.append(str(error))
                raise
            except Exception as error:
                last_error = error
                motore.log(f"Errore AI al tentativo {attempt}: {error}")
                if attempt < retries:
                    time.sleep(3 * attempt)
        detail = f"LLM ha fallito dopo {retries} tentativi: {last_error}"
        PROVIDER_FAILURES.append(detail)
        raise RuntimeError(detail)
    motore.ollama_chat = tracked_chat

    def progress(phase, pct, message=""):
        print(json.dumps({"phase": phase, "pct": pct, "message": message}, ensure_ascii=False), flush=True)

    if operation == "transcribe":
        return motore.transcribe(video, str(work), language)
    base = Path(video).stem
    if operation == "cleanup":
        import sgrossatore_raw as sg
        params["LANGUAGE"] = language
        return sg.analyze_video(video, base, str(work), params, progress)
    if operation in ("clips", "reels", "render_clips", "render_reels"):
        import taglia_clip_per_editing as taglia
        if operation == "clips":
            return taglia.propose_clips(str(request.get("mode", "2")), video, base, str(work), language, params, progress)
        if operation == "reels":
            return taglia.propose_reels(video, base, str(work), language, params, progress)
        if operation == "render_clips":
            return {"files": taglia.render_clip_files(video, request["items"], str(work), language, request.get("vertical", True), params, progress)}
        return {"files": taglia.render_reel_files(video, request["items"], str(work), language, request.get("vertical", True), params, progress)}
    if operation in ("shorts", "render_shorts"):
        import shorts
        if operation == "shorts":
            return shorts.propose_shorts(video, base, str(work), language, params, progress)
        return {"files": shorts.render_short_files(video, request["items"], str(work), language, params, progress, request.get("vertical", True), request.get("burn_subs", False))}
    if operation == "render_cleanup":
        import sgrossatore_raw as sg
        return {"files": sg.cut_selected(video, request["items"], str(work), progress)}
    raise ValueError("Unsupported operation: " + operation)


def validate_provider_outcome(result, operation):
    """Vendored engines may catch chat failures and return empty proposals.

    Retain genuinely partial results with a visible warning, but never present
    a completely failed provider analysis as a successful empty editorial list.
    Deterministic cleanup ranges remain available for explicit review.
    """
    if not PROVIDER_FAILURES or not isinstance(result, dict):
        return result
    warnings = list(dict.fromkeys(PROVIDER_FAILURES))[:12]
    if operation in ("clips", "reels", "shorts") and not result.get(operation):
        raise ProviderError("Analisi non completata: il provider LLM ha fallito e non ci sono proposte. "
                            + " | ".join(warnings)[:1800])
    result["_nl_suite_provider_warnings"] = warnings
    return result


if __name__ == "__main__":
    request_path, response_path = map(Path, sys.argv[1:3])
    USED_MODELS = set()
    PROVIDER_FAILURES = []
    request = json.loads(request_path.read_text(encoding="utf-8"))
    try:
        result = validate_provider_outcome(execute(request), request["operation"])
        if isinstance(result, dict):
            result["_nl_suite_alignment"] = ALIGNMENT_STATE
            result["_nl_suite_execution"] = result.get("_execution") or request.get("asr_execution")
            if request.get("canonical_transcript") and request["operation"] != "transcribe":
                result["_nl_suite_canonical_metadata"] = request.get("canonical_metadata") or {"engine": "unknown", "alignment_completed": False}
        if request["operation"] not in ("transcribe", "diagnose") and not request["operation"].startswith("render_"):
            cache = Path(request["work_dir"]) / (Path(request["input_path"]).stem + "_trascrizione.json")
            if cache.is_file():
                result["_transcript"] = json.loads(cache.read_text(encoding="utf-8"))
        response_path.write_text(json.dumps(result, ensure_ascii=False), encoding="utf-8")
    except Exception:
        traceback.print_exc()
        sys.exit(1)
    # Every Ollama chat already has keep_alive=0 and its own CPU/GPU policy.
    # Do not issue a later global unload: another application may have started
    # using the same model between our request and the end of this process.
