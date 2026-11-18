"""One retained authentic 6s source, read-only; no AI or expensive jobs."""
import importlib.util
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / 'sviluppo/tests/artifacts/v1-authentic-neon-fc6a58d3/data/projects/962b8dbe-3ce5-403a-9a73-b558bf83ce5d/assets/first-six-seconds.mp4'
OUT = ROOT / 'sviluppo/tests/artifacts/vertical-perceptual-evidence'


def run():
    OUT.mkdir(exist_ok=True)
    spec = importlib.util.spec_from_file_location('camera', ROOT / 'sviluppo/bridge/camera_path.py')
    camera = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(camera)
    cache = OUT / 'authentic-legacy-samples.json'
    started = time.perf_counter()
    samples = json.loads(cache.read_text()) if cache.exists() else camera.detect(SOURCE, 0, 6000)
    if not cache.exists():
        cache.write_text(json.dumps(samples, indent=2))
    crop = (9 / 16) / (1920 / 1080)
    path = camera.trajectory(samples, bounds=[[crop / 2, 1 - crop / 2], [.5, .5]])
    (OUT / 'authentic-legacy-camera.json').write_text(json.dumps(path, indent=2))
    ffmpeg, ffprobe = ROOT / 'app/tools/ffmpeg.exe', ROOT / 'app/tools/ffprobe.exe'
    frames = json.loads(subprocess.check_output([str(ffprobe), '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'frame=best_effort_timestamp_time', '-of', 'json', str(SOURCE)]))['frames']
    times = [float(f['best_effort_timestamp_time']) for f in frames]
    md5 = subprocess.check_output([str(ffmpeg), '-v', 'error', '-i', str(SOURCE), '-map', '0:v:0', '-f', 'framemd5', '-'], text=True)
    hashes = [line.split(',')[-1].strip() for line in md5.splitlines() if not line.startswith('#') and ',' in line]
    for at in (0.3, 2.8, 5.6):
        target = OUT / f'original-{at}.png'
        if not target.exists():
            subprocess.run([str(ffmpeg), '-v', 'error', '-ss', str(at), '-i', str(SOURCE), '-frames:v', '1', str(target)], check=True)
    report = dict(source=str(SOURCE), inference=False, elapsed_seconds=time.perf_counter()-started,
                  decoded_frames=len(times), pts_monotonic=all(b>a for a,b in zip(times,times[1:])),
                  pts_step_min=min(b-a for a,b in zip(times,times[1:])), pts_step_max=max(b-a for a,b in zip(times,times[1:])),
                  identical_adjacent_source_frames=sum(a==b for a,b in zip(hashes,hashes[1:])),
                  detections=sum(bool(s['center']) for s in samples), samples=len(samples),
                  old_camera_points=len(path['points']), old_camera_range=max(p['x'] for p in path['points'])-min(p['x'] for p in path['points']),
                  human_visual_acceptance=False)
    (OUT / 'diagnosis.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    run()
