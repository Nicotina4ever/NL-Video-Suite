"""Regression for the previous hidden GPU -> CPU fallback; no ASR inference."""
import importlib.util
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('suite_owned_motore', str(ROOT/'engines/clipsai/motore.py'))
motore = importlib.util.module_from_spec(spec)
spec.loader.exec_module(motore)

class DevicePolicy(unittest.TestCase):
    def test_requested_gpu_failure_never_runs_cpu(self):
        calls=[]
        def load(model, device, **kwargs):
            calls.append(device)
            raise RuntimeError('SIMULATED CUDA out of memory')
        with tempfile.TemporaryDirectory(prefix='nl-device-test-') as work:
            source=Path(work)/'speech.wav'
            source.write_bytes(b'fixture')
            with patch.object(motore,'_import_whisperx',return_value=types.SimpleNamespace(load_model=load)), patch.object(motore,'get_video_duration',return_value=8), patch.object(motore,'_whisper_model_source',return_value='fixture'), patch.object(motore,'WHISPER_DEVICE','cuda'), patch.object(motore,'WHISPER_ALLOW_CPU_FALLBACK',False,create=True):
                with self.assertRaisesRegex(RuntimeError,'Nessun ripiego in CPU'):
                    motore.transcribe(str(source),work,'it')
        self.assertEqual(calls,['cuda'])

if __name__=='__main__':
    unittest.main()
