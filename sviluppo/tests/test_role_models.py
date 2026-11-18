"""Reproducible, network-free tests for the suite-owned ClipsAI LLM adapter.

Run with runtime/python/python.exe sviluppo/tests/test_role_models.py.
No inference, model download or vendored engine mutation is performed.
"""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, patch


BRIDGE_PATH = Path(__file__).resolve().parents[1] / "bridge" / "clipsai_bridge.py"
SPEC = importlib.util.spec_from_file_location("suite_role_bridge", BRIDGE_PATH)
BRIDGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BRIDGE)


def fake_engine():
    engine = types.ModuleType("motore")
    engine.NUM_CTX = 8192
    engine.TEMPERATURE = 0.2
    engine.OLLAMA_MODEL = "default:latest"
    engine.OLLAMA_HOST = "http://unused.invalid"
    engine.OLLAMA_TIMEOUT = 900
    engine._import_whisperx = Mock(side_effect=AssertionError("No model loading allowed"))
    engine.log = Mock()
    return engine


class RoleModelBridgeTests(unittest.TestCase):
    def test_canonical_transcript_is_reused_without_whisper_or_downloads(self):
        engine = fake_engine()
        whisper_import = engine._import_whisperx
        engine.transcribe = Mock(side_effect=AssertionError("New ASR forbidden"))
        clipper = types.ModuleType("taglia_clip_per_editing")
        clipper.propose_clips = lambda mode, video, base, work, lang, params, progress: engine.transcribe(video, work, lang)
        raw = {"id": "stable-transcript", "language": "it", "segments": [{"start_ms": 135, "end_ms": 2077, "text": "Caffè sì."}],
               "words": [{"start_ms": 135, "end_ms": 700, "text": "Caffè"}, {"start_ms": 1700, "end_ms": 2077, "text": "sì."}]}
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / "input.mp4"
            source.touch()
            saved_path = sys.path[:]
            try:
                with patch.dict(sys.modules, {"motore": engine, "taglia_clip_per_editing": clipper}):
                    result = BRIDGE.execute({"source_root": temp, "input_path": str(source), "work_dir": temp,
                                             "operation": "clips", "language": "it", "canonical_transcript": raw})
            finally:
                sys.path[:] = saved_path
            self.assertEqual(result["_canonical_transcript_id"], "stable-transcript")
            self.assertEqual(result["words"][0], {"start": .135, "end": .7, "word": "Caffè"})
            self.assertEqual(result["segments"][0]["words"], result["words"])
            whisper_import.assert_not_called()
            self.assertTrue((Path(temp) / "input_trascrizione.json").is_file())

    def setUp(self):
        BRIDGE.USED_MODELS.clear()

    def test_cpu_model_schema_temperature_and_release_are_explicit(self):
        payload = BRIDGE.ollama_payload(
            fake_engine(), {"model": "qwen3.5:9b", "ollama_execution": "cpu"},
            "Ciao", {"type": "object"}, "Scrivi in italiano", temperature=0.0,
        )
        self.assertEqual(payload["model"], "qwen3.5:9b")
        self.assertEqual(payload["options"], {"num_ctx": 8192, "num_predict": 4096, "temperature": 0.0, "num_gpu": 0})
        self.assertEqual(payload["format"], {"type": "object"})
        self.assertEqual(payload["keep_alive"], 0)
        self.assertFalse(payload["stream"])
        self.assertEqual([message["role"] for message in payload["messages"]], ["system", "user"])

    def test_auto_does_not_override_gpu_and_explicit_engine_model_takes_priority(self):
        payload = BRIDGE.ollama_payload(
            fake_engine(), {"model": "configured:latest", "ollama_execution": "auto"},
            "Ciao", model="explicit:latest",
        )
        self.assertEqual(payload["model"], "explicit:latest")
        self.assertNotIn("num_gpu", payload["options"])
        self.assertNotIn("format", payload)

    def test_old_requests_default_to_cpu_and_invalid_policies_fail(self):
        payload = BRIDGE.ollama_payload(fake_engine(), {}, "Ciao")
        self.assertEqual(payload["options"]["num_gpu"], 0)
        self.assertEqual(payload["model"], "default:latest")
        with self.assertRaises(ValueError):
            BRIDGE.ollama_payload(fake_engine(), {"ollama_execution": "unlimited"}, "Ciao")

    def test_execute_routes_real_adapter_and_preserves_structured_response(self):
        engine = fake_engine()
        original_whisper_import = engine._import_whisperx
        schema = {"type": "object"}
        engine.transcribe = lambda *_: engine.ollama_chat("Analizza", schema=schema, temperature=0.0)
        response = Mock()
        response.status_code = 200
        response.iter_content.return_value = [json.dumps({"message": {"content": '{"accepted":true}'}}).encode()]
        network = types.ModuleType("requests")
        network.post = Mock(return_value=response)
        session = types.SimpleNamespace(post=network.post, close=Mock(), trust_env=True)
        network.Session = Mock(return_value=session)
        saved_path = sys.path[:]
        try:
            with tempfile.TemporaryDirectory(prefix="nl-role-models-") as temporary:
                root = Path(temporary)
                media = root / "synthetic.wav"
                media.touch()
                request = {
                    "source_root": str(root), "input_path": str(media), "work_dir": str(root / "work"),
                    "operation": "transcribe", "model": "clipping:latest",
                    "ollama_url": "http://127.0.0.1:54321", "ollama_execution": "cpu",
                }
                with patch.dict(sys.modules, {"motore": engine, "requests": network}):
                    result = BRIDGE.execute(request)
        finally:
            sys.path[:] = saved_path
        self.assertEqual(result, {"accepted": True})
        network.post.assert_called_once()
        args, kwargs = network.post.call_args
        self.assertEqual(args[0], "http://127.0.0.1:54321/api/chat")
        self.assertEqual(kwargs["json"]["model"], "clipping:latest")
        self.assertEqual(kwargs["json"]["options"]["num_gpu"], 0)
        self.assertEqual(kwargs["json"]["format"], schema)
        self.assertEqual(BRIDGE.USED_MODELS, {"clipping:latest"})
        original_whisper_import.assert_not_called()
        self.assertFalse(session.trust_env)

    def test_compatible_payload_and_response_use_only_compatible_protocol(self):
        url, body = BRIDGE.provider_payload(fake_engine(), {
            "llm_provider": "openai_compatible", "openai_base_url": "http://127.0.0.1:1234/v1/",
            "model": "Local Model Q4.gguf", "ollama_execution": "cpu",
        }, "Scrivi JSON", schema={"type": "object"})
        self.assertEqual(url, "http://127.0.0.1:1234/v1/chat/completions")
        self.assertEqual(body["model"], "Local Model Q4.gguf")
        self.assertEqual(body["response_format"]["json_schema"]["schema"], {"type": "object"})
        self.assertEqual(body["max_tokens"], 4096)
        for field in ("options", "think", "keep_alive", "format", "num_gpu", "num_ctx"):
            self.assertNotIn(field, body)
        response = Mock(status_code=200)
        response.iter_content.return_value = [json.dumps({"choices": [{"message": {"content": '{"ok":true}'}, "finish_reason": "stop"}]}).encode()]
        self.assertEqual(BRIDGE.read_provider_response(response, "openai_compatible"), '{"ok":true}')

    def test_authentication_truncation_and_oversize_are_explicit(self):
        response = Mock(status_code=401)
        with self.assertRaisesRegex(BRIDGE.ProviderError, "autenticazione"):
            BRIDGE.read_provider_response(response, "openai_compatible")
        response = Mock(status_code=200)
        response.iter_content.return_value = [json.dumps({"choices": [{"message": {"content": "partial"}, "finish_reason": "length"}]}).encode()]
        with self.assertRaisesRegex(BRIDGE.ProviderError, "troncata"):
            BRIDGE.read_provider_response(response, "openai_compatible")
        response.iter_content.return_value = [b"x" * (BRIDGE.MAX_RESPONSE_BYTES + 1)]
        with self.assertRaisesRegex(BRIDGE.ProviderError, "2 MiB"):
            BRIDGE.read_provider_response(response, "openai_compatible")

    def test_execute_compatible_does_not_track_or_unload_ollama_models(self):
        engine = fake_engine()
        engine.transcribe = lambda *_: engine.ollama_chat("JSON", schema={"type": "object"})
        response = Mock(status_code=200)
        response.iter_content.return_value = [json.dumps({"choices": [{"message": {"content": '{"compatible":true}'}}]}).encode()]
        network = types.ModuleType("requests")
        network.post = Mock(return_value=response)
        network.Session = Mock(return_value=types.SimpleNamespace(post=network.post, close=Mock()))
        saved_path = sys.path[:]
        try:
            with tempfile.TemporaryDirectory(prefix="nl-role-compatible-") as temporary:
                root = Path(temporary)
                media = root / "synthetic.wav"
                media.touch()
                request = {"source_root": str(root), "input_path": str(media), "work_dir": str(root / "work"),
                           "operation": "transcribe", "model": "Local.gguf", "llm_provider": "openai_compatible",
                           "openai_base_url": "http://127.0.0.1:54321/v1"}
                with patch.dict(sys.modules, {"motore": engine, "requests": network}):
                    result = BRIDGE.execute(request)
        finally:
            sys.path[:] = saved_path
        self.assertEqual(result, {"compatible": True})
        self.assertEqual(network.post.call_args.args[0], "http://127.0.0.1:54321/v1/chat/completions")
        self.assertEqual(BRIDGE.USED_MODELS, set())


if __name__ == "__main__":
    unittest.main(verbosity=2)
