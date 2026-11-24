"""Minimal OpenAI-compatible speech server for testing NL Video Suite locally.

Uses the suite's own runtime (faster-whisper already installed) and the local
models/asr/faster-whisper-large-v3; nothing is downloaded. It is a test fixture, not part of the app:
  runtime\python\python.exe documentazione\prove\p10-asr-provider\asr_test_server.py
Then configure nl.asrProvider=openai_compatible and nl.asrUrl=http://127.0.0.1:8000/v1.
"""
import os
import sys
import tempfile

from flask import Flask, jsonify, request
from faster_whisper import WhisperModel

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
MODEL_DIR = os.path.join(ROOT, "models", "asr", "faster-whisper-large-v3")
DEVICE = os.environ.get("NL_ASR_TEST_DEVICE", "cpu")
COMPUTE = os.environ.get("NL_ASR_TEST_COMPUTE", "int8")

app = Flask(__name__)
model = None


def get_model():
    global model
    if model is None:
        print(f"loading {MODEL_DIR} on {DEVICE}/{COMPUTE}", flush=True)
        model = WhisperModel(MODEL_DIR, device=DEVICE, compute_type=COMPUTE)
        print("model ready", flush=True)
    return model


@app.get("/v1/models")
def models():
    return jsonify({"object": "list", "data": [{"id": "faster-whisper-large-v3", "object": "model"}]})


@app.post("/v1/audio/transcriptions")
def transcriptions():
    upload = request.files.get("file")
    if upload is None:
        return jsonify({"error": {"message": "missing file"}}), 400
    language = request.form.get("language")
    if language in ("", "auto"):
        language = None
    suffix = os.path.splitext(upload.filename or "audio.wav")[1] or ".wav"
    fd, path = tempfile.mkstemp(suffix=suffix)
    os.close(fd)
    try:
        upload.save(path)
        segments, info = get_model().transcribe(path, language=language, vad_filter=True)
        rows = [{"start": s.start, "end": s.end, "text": s.text.strip()} for s in segments]
        text = " ".join(row["text"] for row in rows).strip()
        return jsonify({"text": text, "language": info.language, "language_probability": info.language_probability, "segments": rows})
    finally:
        try:
            os.remove(path)
        except OSError:
            pass


if __name__ == "__main__":
    print(f"model: {MODEL_DIR}", flush=True)
    print("listening on http://127.0.0.1:8000/v1", flush=True)
    app.run(host="127.0.0.1", port=8000, threaded=False)
