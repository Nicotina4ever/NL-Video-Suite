"""Reproducible baseline against isolated fixtures; does not run AI inference.

Logs and an incrementally saved report remain resumable after interruption.
"""
import argparse
import hashlib
from datetime import datetime, timezone
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]

# Only new, successful synthetic run data, never existing fixtures/inferences.
# Full editable cases are opt-in; reports and QA screenshots stay by default.
COMPACT_GROUPS = ('album-large', 'album-script', 'asr-guided', 'asr-store',
    'asr-versions', 'audio-mixer', 'creative-cache', 'creative-montage',
    'edit-versions', 'longform-performance', 'montage-planner',
    'narrative-assistant', 'narrative-audio-first', 'narrative-source',
    'narrative-versions', 'narrative-partial', 'perceptual-vertical', 'production-package',
    'reference-audio', 'preproduction', 'workspace-flow', 'workspace-prerequisite', 'reframe-export', 'shorts-studio', 'subtitle-formats', 'typography')


def run_directories():
    root = ROOT / 'sviluppo/tests/artifacts'
    return {p.resolve() for group in COMPACT_GROUPS for p in (root / group).glob('*') if p.is_dir()}


def compact_new_runs(before):
    removed = []
    root = (ROOT / 'sviluppo/tests/artifacts').resolve()
    for run in run_directories() - before:
        receipt = run / 'report.json'
        if not receipt.is_file():
            continue
        report = json.loads(receipt.read_text(encoding='utf-8-sig'))
        if report.get('passed') is not True or any(report.get(k) is True for k in ('real_ai_inference', 'new_ai_inference', 'new_inference', 'human_acceptance', 'human_artistic_acceptance', 'artistic_acceptance', 'human_listening')):
            continue
        # Old harnesses copy the EXE per run. The certified candidate and its SHA
        # remain in target/release; retaining another binary here serves no QA.
        candidates = [run / 'data'] + [p for p in run.iterdir() if p.suffix.lower() in ('.mp4', '.wav', '.m4a', '.zip', '.exe')]
        # Extracted dossier copies are recreatable from their retained JSON/HTML;
        # keep their original source fixture, never treat arbitrary dirs as scratch.
        for path in candidates:
            if not path.exists() or path.is_symlink() or path.resolve().parent != run or root not in path.resolve().parents:
                continue
            if path.is_dir():
                if path.name != 'data':
                    continue
                shutil.rmtree(path)
            else:
                path.unlink()
            removed.append(str(path.relative_to(root)))
    return removed


def emit(message):
    try:
        print(message, flush=True)
    except OSError:
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--rust-only', action='store_true')
    parser.add_argument('--skip-rust', action='store_true')
    parser.add_argument('--only', nargs='+', help='Run only named checks after a targeted fix')
    parser.add_argument('--candidate', action='store_true', help='Build/test the separate planner candidate while the old development preview is open')
    parser.add_argument('--executable', type=Path, help='Separate development EXE under target/release; keeps an open preview/certified candidate intact')
    parser.add_argument('--preserve-w3-renders', action='store_true', help='Explicitly omit audio_master renders; retain historical W3 evidence separately, never label this a full regression')
    parser.add_argument('--keep-test-data', action='store_true', help='Retain full successful synthetic cases/media for manual QA; default keeps reports/screenshots only. Real inference and failed runs are always retained.')
    args = parser.parse_args()
    out = ROOT / 'sviluppo/tests/artifacts/v1-baseline' / datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    out.mkdir(parents=True)
    report = {'created_at': datetime.now(timezone.utc).isoformat(), 'real_ai_inference': False,
              'services': 'isolated fake providers; real FFmpeg/Edge', 'results': []}
    python = str(ROOT / 'runtime/python/python.exe')
    exe = str(ROOT / ('sviluppo/target/release/nl-video-suite-planner.exe' if args.candidate else 'sviluppo/target/release/nl-video-suite.exe'))
    if args.executable:
        chosen = args.executable.resolve()
        if chosen.parent != (ROOT / 'sviluppo/target/release').resolve() or chosen.suffix.lower() != '.exe':
            parser.error('--executable must be a development .exe directly under sviluppo/target/release')
        exe = str(chosen)
    report['tested_executable'] = exe
    tasks = []
    if not args.skip_rust:
        tasks += [('fmt', ['cargo', 'fmt', '--check']),
                  ('unit', ['cargo', 'test', '--release', '--offline']),
                  ('clippy', ['cargo', 'clippy', '--release', '--offline', '--all-targets', '--', '-D', 'warnings']),
                  ('release', ['cargo', 'rustc', '--release', '--offline', '--bin', 'nl-video-suite', '--', '-o', 'target/release/nl-video-suite-planner.exe'] if args.candidate else ['cargo', 'build', '--release', '--offline'])]
        if args.executable:
            tasks[-1] = ('release', ['cargo', 'rustc', '--release', '--offline', '--bin', 'nl-video-suite', '--', '-o', exe])
    if not args.rust_only:
        for name in ['smoke', 'narrative_http', 'objects_http', 'flow_http', 'playback_http', 'maintenance_http', 'narrative_replay', 'test_pipeline_errors', 'test_role_models', 'test_qwen_adapter', 'test_portable_preflight', 'test_camera_path']:
            command = [python, str(ROOT / 'sviluppo/tests' / (name + '.py'))]
            if name in ('smoke', 'narrative_http', 'objects_http', 'flow_http', 'playback_http', 'maintenance_http', 'narrative_replay'):
                command.append(exe)
            tasks.append((name, command))
        for name in ['browser', 'nle_browser', 'narrative_browser', 'narrative_library', 'flow_browser', 'provider_profiles', 'studio_revision', 'longform_browser', 'longform_chapters', 'asr_review_browser', 'shot_map_browser', 'shot_window_browser', 'shorts_studio_browser', 'subtitle_studio_browser', 'subtitle_formats', 'motion_studio_browser', 'motion_svg_thumbnail', 'typography_browser', 'social_studio_browser', 'recovery_browser', 'recovery_uncertain', 'prepare_human_review', 'final_acceptance']:
            tasks.append((name, ['node', str(ROOT / 'sviluppo/tests' / (name + '.cjs')), exe]))
        tasks.append(('edit_versions', ['node', str(ROOT / 'sviluppo/tests/edit_versions.cjs'), exe]))
        tasks.append(('audio_mixer', ['node', str(ROOT / 'sviluppo/tests/audio_mixer.cjs'), exe]))
        tasks.append(('audio_master', ['node', str(ROOT / 'sviluppo/tests/audio_master.cjs'), exe]))
        tasks.append(('asr_versions', ['node', str(ROOT / 'sviluppo/tests/asr_versions.cjs'), exe]))
        for name in ['asr_guided', 'asr_store', 'narrative_source']:
            tasks.append((name, ['node', str(ROOT / 'sviluppo/tests' / (name + '.cjs')), exe]))
        tasks.append(('album_large', ['node', str(ROOT / 'sviluppo/tests/album_large.cjs'), exe]))
        tasks.append(('album_script', ['node', str(ROOT / 'sviluppo/tests/album_script.cjs'), exe]))
        tasks.append(('album_resources', ['node', str(ROOT / 'sviluppo/tests/album_resources.cjs'), exe]))
        tasks.append(('album_writing', ['node', str(ROOT / 'sviluppo/tests/album_writing.cjs'), exe]))
        tasks.append(('workspace_flow', ['node', str(ROOT / 'sviluppo/tests/workspace_flow.cjs'), exe]))
        tasks.append(('timeline_follow_browser', ['node', str(ROOT / 'sviluppo/tests/timeline_follow_browser.cjs'), exe]))
        tasks.append(('editing_workspace_browser', ['node', str(ROOT / 'sviluppo/tests/editing_workspace_browser.cjs'), exe]))
        tasks.append(('montage_sequence_browser', ['node', str(ROOT / 'sviluppo/tests/montage_sequence_browser.cjs'), exe]))
        tasks.append(('project_delete_browser', ['node', str(ROOT / 'sviluppo/tests/project_delete_browser.cjs'), exe]))
        tasks.append(('motion_idea_browser', ['node', str(ROOT / 'sviluppo/tests/motion_idea_browser.cjs'), exe]))
        tasks.append(('workspace_prerequisite', ['node', str(ROOT / 'sviluppo/tests/workspace_prerequisite.cjs'), exe]))
        tasks.append(('asr_device_policy', [python, str(ROOT / 'sviluppo/tests/test_asr_device_policy.py')]))
        tasks.append(('preproduction', ['node', str(ROOT / 'sviluppo/tests/preproduction.cjs'), exe]))
        tasks.append(('narrative_partial', ['node', str(ROOT / 'sviluppo/tests/narrative_partial.cjs'), exe]))
        tasks.append(('narrative_versions', ['node', str(ROOT / 'sviluppo/tests/narrative_versions.cjs'), exe]))
        tasks.append(('reference_audio', ['node', str(ROOT / 'sviluppo/tests/reference_audio.cjs'), exe]))
        for name in ['narrative_audio_first', 'production_package']:
            tasks.append((name, ['node', str(ROOT / 'sviluppo/tests' / (name + '.cjs')), exe]))
        tasks.append(('ux_audit', ['node', str(ROOT / 'sviluppo/tests/ux_audit.cjs'), exe]))
        for name in ['creative_montage', 'narrative_assistant', 'creative_cache', 'montage_planner', 'reframe_export', 'perceptual_vertical']:
            tasks.append((name, ['node', str(ROOT / 'sviluppo/tests' / (name + '.cjs')), exe]))
        tasks.append(('montage_semantic_replay', ['node', str(ROOT / 'sviluppo/tests/montage_planner.cjs'), exe,
                      '--replay-real', str(ROOT / 'sviluppo/tests/artifacts/montage-planner-real/2026-10-28T10-33-44-549Z/raw-provider-response.json')]))
        tasks.append(('montage_semantic_revision', ['node', str(ROOT / 'sviluppo/tests/montage_planner.cjs'), exe,
                      '--replay-real', str(ROOT / 'sviluppo/tests/artifacts/montage-planner-real/2026-10-28T13-52-12-349Z/raw-provider-response.json')]))
        tasks.append(('portable_core',['node',str(ROOT/'sviluppo/tests/motion_svg_thumbnail.cjs'),exe,'--portable-core']))
        tasks.append(('longform_performance',['node',str(ROOT/'sviluppo/tests/longform_performance.cjs'),exe]))
    env = {**os.environ, 'PYTHONIOENCODING': 'utf-8', 'PYTHONDONTWRITEBYTECODE': '1'}
    if args.only:
        report['scope'] = 'targeted production checks, not a complete regression'
        report['full_regression'] = False
        report['selected_checks'] = args.only
        unknown = set(args.only) - {name for name, _ in tasks}
        if unknown:
            parser.error('Unknown checks: ' + ', '.join(sorted(unknown)))
        tasks = [(name, command) for name, command in tasks if name in args.only]
    if args.preserve_w3_renders:
        tasks = [(name, command) for name, command in tasks if name != 'audio_master']
        report['scope'] = 'targeted production checks; historical W3 renders preserved' if args.only else 'general regression excluding W3 renders by explicit preservation policy'
        report['full_regression'] = False
        report['historical_w3_evidence_not_retested'] = str(ROOT / 'sviluppo/tests/artifacts/audio-master/2026-10-29T00-06-18-469Z/report.json')
    for name, command in tasks:
        entry = {'test': name, 'command': command, 'status': 'running'}
        report['results'].append(entry)
        (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        emit('RUN ' + name)
        started = time.monotonic()
        before = run_directories() if not args.keep_test_data else set()
        try:
            with (out / (name + '.log')).open('wb') as log:
                result = subprocess.run(command, cwd=ROOT / 'sviluppo' if name in ('fmt', 'unit', 'clippy', 'release') else ROOT,
                                        env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300)
            entry.update(status='passed' if result.returncode == 0 else 'failed', exit_code=result.returncode)
        except (OSError, subprocess.TimeoutExpired) as error:
            entry.update(status='failed', error=str(error))
        entry['elapsed_seconds'] = round(time.monotonic() - started, 2)
        if entry['status'] == 'passed' and not args.keep_test_data:
            try:
                entry['compacted_new_synthetic_data'] = compact_new_runs(before)
            except (OSError, ValueError) as error:
                entry['compaction_error_data_retained'] = str(error)
        entry['log'] = str(out / (name + '.log'))
        (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        emit(entry['status'].upper() + ' ' + name + ' ' + str(entry['elapsed_seconds']) + 's')
        if name == 'release' and entry['status'] != 'passed':
            report['stopped_after_build_failure'] = True
            break
    report['passed'] = all(v['status'] == 'passed' for v in report['results'])
    if Path(exe).is_file():
        digest = hashlib.sha256()
        with Path(exe).open('rb') as binary:
            for chunk in iter(lambda: binary.read(1024 * 1024), b''):
                digest.update(chunk)
        report['tested_executable_sha256'] = digest.hexdigest()
    (out / 'report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    emit('REPORT ' + str(out / 'report.json'))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
