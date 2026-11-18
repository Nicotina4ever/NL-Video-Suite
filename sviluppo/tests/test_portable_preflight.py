"""Read-only inventory plus isolated synthetic packaging guards; no ZIP/models/ASR."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('nl_package', ROOT / 'app/crea_portable.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackagingPreflight(unittest.TestCase):
    def fixture(self, directory):
        root = Path(directory)
        required = ['app/nl-video-suite.exe', 'app/tools/ffmpeg.exe', 'app/tools/ffprobe.exe',
                    'runtime/python/python.exe', 'models/asr/faster-whisper-large-v3/model.bin',
                    'models/alignment/italian-xlsr53/pytorch_model.bin']
        for name in package.DIRECTORIES:
            (root / name).mkdir(exist_ok=True)
        for name in required + list(package.FILES) + ['data/settings.json', 'data/projects/test/.cache/preview.bin']:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('SYNTHETIC FIXTURE', encoding='utf-8')
        return root

    def test_guard_and_clean_inventory_use_executable_version_without_creating_archive(self):
        with tempfile.TemporaryDirectory(prefix='nl-package-guard-') as directory:
            root = self.fixture(directory)
            with patch.object(package, 'ROOT', root), patch.object(package.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, '0.7.1\n', '')):
                report, files = package.preflight()
                self.assertEqual(report['version'], '0.7.1')
                self.assertEqual(sum(report['components_bytes'].values()),report['source_bytes'])
                self.assertIn('torch_cuda_libraries',report['capability_notes'])
                self.assertFalse(any(p.is_relative_to(root / 'data') for p in files))
                _, with_data = package.preflight(True)
                self.assertIn(root / 'data/settings.json', with_data)
                self.assertFalse(any('.cache' in p.parts for p in with_data))
                with self.assertRaisesRegex(RuntimeError, 'Versione attesa'):
                    package.preflight(expected_version='1.0.0')
                (root / 'app/tools/ffprobe.exe').unlink()
                with self.assertRaisesRegex(RuntimeError, 'Pacchetto incompleto'):
                    package.preflight()
                self.assertFalse((root / 'distribuzione').exists())

    def test_real_read_only_inventory_no_archive_or_model_copy(self):
        before = sorted(str(p) for p in (ROOT / 'distribuzione').glob('*'))
        result = subprocess.run([str(ROOT / 'runtime/python/python.exe'), str(ROOT / 'app/crea_portable.py'), '--verifica'],
                                capture_output=True, text=True, check=True, timeout=90,
                                env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})
        report = json.loads(result.stdout)
        self.assertFalse(report['archive_created'])
        self.assertFalse(report['models_hashed_or_copied'])
        self.assertFalse(report['includes_personal_data'])
        self.assertFalse(report['real_ai_inference'])
        self.assertGreater(report['file_count'], 100)
        self.assertEqual(before, sorted(str(p) for p in (ROOT / 'distribuzione').glob('*')))
        print('READ-ONLY PACKAGING INVENTORY ' + json.dumps(report), flush=True)


if __name__ == '__main__':
    unittest.main()
