"""Synthetic trajectory tests. No claim of face-detection/editorial ground truth."""
import importlib.util
import json
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('camera_path', ROOT / 'sviluppo/bridge/camera_path.py')
camera = importlib.util.module_from_spec(spec)
spec.loader.exec_module(camera)


def curve_metrics(points):
    velocity, acceleration = [], []
    for a, b in zip(points, points[1:]):
        if b['cut']:
            continue
        dt = (b['source_ms'] - a['source_ms']) / 1000
        for axis in ('x', 'y'):
            c = dt * a['v' + axis]
            d = 3 * (b[axis] - a[axis]) - dt * (2 * a['v' + axis] + b['v' + axis])
            e = 2 * (a[axis] - b[axis]) + dt * (a['v' + axis] + b['v' + axis])
            for n in range(61):
                q = n / 60
                velocity.append(abs((c + 2 * d * q + 3 * e * q * q) / dt))
                acceleration.append(abs((2 * d + 6 * e * q) / dt**2))
    return dict(max_velocity=max(velocity), max_acceleration=max(acceleration), evaluated='analytic cubic derivatives,61 samples per segment per axis, cuts excluded')


def value_at(points, at):
    if at <= points[0]['source_ms']:
        return points[0]['x']
    for a, b in zip(points, points[1:]):
        if at <= b['source_ms']:
            if b['cut']:
                return a['x'] if at < b['source_ms'] else b['x']
            dt = (b['source_ms'] - a['source_ms']) / 1000
            q = (at - a['source_ms']) / (b['source_ms'] - a['source_ms'])
            c = dt * a['vx']
            d = 3 * (b['x'] - a['x']) - dt * (2 * a['vx'] + b['vx'])
            e = 2 * (a['x'] - b['x']) + dt * (a['vx'] + b['vx'])
            return a['x'] + q * (c + q * (d + q * e))
    return points[-1]['x']


def run():
    report = dict(passed=False, ai_inference=False, media='synthetic measured coordinates, not real detection', cases={})
    for name in ('static', 'rapid', 'loss', 'cut', 'outlier', 'edge'):
        samples = []
        for i in range(41):
            x = .5 + (.012 if i % 2 else -.012) if name == 'static' else .25 if i < 12 else .75
            if name == 'outlier':
                x = .9 if i == 15 else .4
            if name == 'edge':
                x = .5 if i<12 else .0 if i<26 else 1.
            center = None if name == 'loss' and 15 <= i <= 22 else [x, .4]
            samples.append(dict(source_ms=i * 250, center=center, cut=name == 'cut' and i == 12, faces=[], uncertainty='synthetic'))
        t = camera.trajectory(samples,bounds=[[.15,.85],[.5,.5]] if name=='edge' else None)
        p = t['points']
        velocities = [(b['x'] - a['x']) / ((b['source_ms']-a['source_ms'])/1000) for a, b in zip(p, p[1:]) if not b['cut']]
        continuous = curve_metrics(p)
        assert all(0 <= s['x'] <= 1 and 0 <= s['y'] <= 1 for s in p)
        assert max(map(abs, velocities)) <= 1.01
        assert continuous['max_velocity'] <= 1.0001
        assert continuous['max_acceleration'] <= 3.001, (name,continuous)
        if name == 'static':
            assert max(s['x'] for s in p) - min(s['x'] for s in p) < .01
        elif name == 'rapid':
            assert abs(value_at(p,4000) - .75) < .06, 'fast subject must settle within1s without accumulating lag'
        elif name == 'loss':
            values=[value_at(p,at) for at in range(3750,5501,50)]
            assert max(values)-min(values) < .03
        elif name == 'cut':
            assert any(s['cut'] and s['source_ms']==3000 for s in p) and value_at(p,3000)==.75 and value_at(p,2999)==.25
        elif name == 'outlier':
            assert all(abs(s['x'] - .4) < .0001 for s in p)
        elif name == 'edge':
            assert all(.15<=value_at(p,at)<=.85 for at in range(0,10001,5))
        report['cases'][name] = dict(knots=len(p),travel=max(s['x'] for s in p)-min(s['x'] for s in p), max_velocity=max(map(abs, velocities)), velocity_std=statistics.pstdev(velocities), continuous_curve=continuous)
    # Composition mode: intentional shot locking, not another per-frame ease.
    composed = {}
    for name in ('quiet', 'rapid', 'loss', 'cut', 'forced_fixed'):
        samples = []
        for i in range(41):
            x = .48 + (.012 if i % 2 else -.012) if name == 'quiet' else .3 if i < 12 else .7
            center = None if name == 'loss' and 15 <= i <= 24 else [x, .5]
            samples.append(dict(source_ms=i*250, center=center, cut=name=='cut' and i==12,
                                faces=[dict(x=x-.04,y=.4,width=.08,height=.2)] if center else [], uncertainty='single' if center else 'missing'))
        t = camera.composed_trajectory(samples, bounds=[[.1582,.8418],[.5,.5]], behavior='fixed' if name=='forced_fixed' else 'composed')
        p, m = t['points'], curve_metrics(t['points'])
        assert m['max_velocity'] < .5 and m['max_acceleration'] < 1.5, (name,m)
        if name=='quiet':
            assert max(v['x'] for v in p)-min(v['x'] for v in p)<1e-9, 'head jitter must not move the camera'
            assert t['preferred_fit']=='cover'
        elif name=='rapid':
            assert abs(value_at(p,4500)-.7)<.07, 'composed camera must respond to significant movement'
            assert t['preferred_fit']=='blur', 'large movements are not a certified fixed crop'
        elif name=='loss':
            assert t['preferred_fit']=='blur' and t['diagnostics']['maximum_gap_ms']>=2500
        elif name=='cut':
            assert value_at(p,3000)==.7 and value_at(p,2999)==.3
        elif name=='forced_fixed':
            assert max(v['x'] for v in p)-min(v['x'] for v in p)<1e-9
            assert t['preferred_fit']=='blur', 'manual fixed is not proof of all-face containment'
        composed[name]=dict(knots=len(p),preferred_fit=t['preferred_fit'],diagnostics=t['diagnostics'],continuous_curve=m)
    report['composed_cases']=composed
    report['passed'] = True
    out = ROOT / 'sviluppo/tests/artifacts/camera-path'
    out.mkdir(exist_ok=True)
    (out / 'synthetic-report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    run()
