"""One temporary FULL relocation, checksummed; no archive, ASR, downloads or stable update."""
import hashlib
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('package', ROOT / 'app/crea_portable.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 << 20), b''):
            h.update(block)
    return h.hexdigest()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--resume',type=Path,help='Resume the private stage from this report, never recopy verified weights')
    args=parser.parse_args()
    stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    out = ROOT / 'sviluppo/tests/artifacts/portable-full' / stamp
    out.mkdir(parents=True)
    report = dict(passed=False, real_ai_inference=False, archive_created=False,
                  stable_promoted=False, different_physical_pc=False, stages=[],
                  public_distribution_legal_acceptance=False)
    save = lambda: (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    save()
    before = digest(ROOT / 'app/nl-video-suite.exe')
    inventory, files = package.preflight()
    report['inventory'] = inventory
    report['stages'].append('source read-only inventory, clean allowlist, no personal data')
    save()
    # Exactly one temporary copy is required to test relocation of actual dependencies.
    if args.resume:
        previous=json.loads(args.resume.read_text(encoding='utf-8'))
        stage=Path(previous['stage_path']).resolve()
        assert stage.is_dir()
    else:
        stage = Path(tempfile.mkdtemp(prefix='nl-full portable con spazi-')).resolve()
    assert stage.parent == Path(tempfile.gettempdir()).resolve()
    assert stage.name.startswith('nl-full portable con spazi-')
    report['stage_path']=str(stage)
    known={}
    prior=ROOT/'sviluppo/tests/artifacts/portable-full/20261026T180048Z/verified-manifest.json'
    if prior.is_file():
        known={f['path']:f['sha256'] for f in json.loads(prior.read_text(encoding='utf-8'))}
        report['reused_reference_manifest']=str(prior.relative_to(ROOT))
    save()
    try:
        manifest = []
        total = 0
        notified = 0
        started = time.monotonic()
        for source in files:
            rel = source.relative_to(ROOT)
            if rel.as_posix() == 'app/nl-video-suite.exe':
                source = ROOT / 'sviluppo/target/release/nl-video-suite.exe'
            target = stage / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            original = source.stat()
            # Prior verified bytes remain the reference for unchanged heavy resources.
            # Any changed destination fails its SHA; app/docs/current EXE are freshly hashed.
            heavy=rel.parts[0] in ('runtime','models','engines') or rel.parts[:2]==('app','tools')
            expected=known.get(rel.as_posix()) if heavy else None
            expected=expected or digest(source)
            if not target.is_file() or digest(target)!=expected:
                shutil.copy2(source, target)
            assert digest(target) == expected, str(rel)
            assert source.stat().st_size == original.st_size and source.stat().st_mtime_ns == original.st_mtime_ns
            manifest.append(dict(path=rel.as_posix(), bytes=target.stat().st_size, sha256=expected))
            total += target.stat().st_size
            if total - notified >= 512 << 20:
                print(f'COPY+VERIFY {total / (1 << 30):.2f} GiB: {rel}', flush=True)
                report.update(verified_bytes=total, verified_files=len(manifest))
                save()
                notified = total
        (out / 'verified-manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
        report.update(verified_bytes=total, verified_files=len(manifest), copy_verify_seconds=time.monotonic()-started)
        report['stages'].append('all allowlisted files physically copied and source/destination SHA verified; candidate EXE substituted only in temporary copy')
        save()
        if not args.resume:
            assert not (stage / 'data').exists()
        env = dict(os.environ)
        for key in ('NL_SUITE_ROOT', 'PYTHONHOME', 'PYTHONPATH'):
            env.pop(key, None)
        env.update(PATH=str(stage/'app/tools')+';'+str(Path(os.environ['SystemRoot'])/'System32')+';'+os.environ['SystemRoot'],
                   HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1', HF_DATASETS_OFFLINE='1',
                   HF_HOME=str(stage/'models/.qa-cache'), TORCH_HOME=str(stage/'models/.qa-cache'), PYTHONDONTWRITEBYTECODE='1')
        py = stage / 'runtime/python/python.exe'
        probe = 'import json,sys,torch,ctranslate2,cv2,whisperx.asr,whisperx.alignment;print(json.dumps(dict(executable=sys.executable,torch=torch.__version__,ctranslate2=ctranslate2.__version__,opencv=cv2.__version__,whisperx_asr_alignment_import=True)))'
        result = subprocess.run([str(py), '-I', '-B', '-c', probe], cwd=Path(os.environ['SystemRoot'])/'System32', env=env, capture_output=True, text=True, timeout=180)
        (out/'runtime-import.log').write_text(result.stdout+'\n'+result.stderr, encoding='utf-8')
        assert result.returncode == 0, 'isolated relocated runtime import: '+result.stderr[-4000:]
        report['runtime_import'] = result.stdout.strip()
        report['stages'].append('relocated embedded Python imports actual Torch/CTranslate2/OpenCV/WhisperX ASR+alignment modules offline from System32; no model inference')
        save()
        # The browser controller uses the host Node/Edge; the suite cannot inherit their PATH.
        result = subprocess.run(['node', str(ROOT/'sviluppo/tests/motion_svg_thumbnail.cjs'), '--existing-portable', str(stage)], cwd=ROOT, capture_output=True, text=True, timeout=300)
        (out/'browser.log').write_text(result.stdout+'\n'+result.stderr, encoding='utf-8')
        assert result.returncode == 0, result.stdout+result.stderr
        report['native_browser'] = json.loads((ROOT/'sviluppo/tests/artifacts/portable-full-native/report.json').read_text())
        assert report['native_browser']['passed']
        report['stages'].append('complete candidate: UI/assets/SVG/hybrid/native CC/motion MP4/thumbnail PNG/reopen with cwdSystem32, system-only PATH, optional AI offline')
        assert digest(ROOT/'app/nl-video-suite.exe') == before
        report['passed'] = True
        print('PASS FULL temporary relocation: '+str(out), flush=True)
    except Exception as exc:
        report['error'] = repr(exc)
        raise
    finally:
        # Explicitly validated private temporary target; never the workspace or originals.
        assert stage.parent == Path(tempfile.gettempdir()).resolve() and stage.name.startswith('nl-full portable con spazi-')
        if report['passed']:
            shutil.rmtree(stage)
            report['temporary_copy_removed'] = True
        else:
            report['temporary_copy_removed']=False
            report['resume_command']='runtime/python/python.exe -B sviluppo/tests/portable_full.py --resume "'+str(out/'report.json')+'"'
            print('PRIVATE TEST STAGE RETAINED for recovery: '+str(stage),flush=True)
        save()


if __name__ == '__main__':
    main()
