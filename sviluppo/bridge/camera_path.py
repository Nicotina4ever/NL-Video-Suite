"""Offline camera trajectory from bounded CPU detections. No neural models.

Source time, not timeline time: trimming/splitting does not restart the camera.
Raw samples are cached separately from the editable stabilization settings.
"""
import json
import math
from pathlib import Path
import sys
import statistics
import cv2

VERSION = 'camera-cpu-v2'


def coefficients(a, b, axis):
    dt = (b['source_ms'] - a['source_ms']) / 1000
    c = dt * a['v' + axis]
    d = 3 * (b[axis] - a[axis]) - dt * (2 * a['v' + axis] + b['v' + axis])
    e = 2 * (a[axis] - b[axis]) + dt * (a['v' + axis] + b['v' + axis])
    return dt, c, d, e


def simplify(dense, maximum_velocity=1., maximum_acceleration=3.):
    """Adaptive *curve* limits, not just bounds on sampled knot coordinates."""
    def reduce_shot(points):
        if len(points) <= 2:
            return points
        a, b = points[0], points[-1]
        acceptable = True
        for axis in ('x', 'y'):
            dt, c, d, e = coefficients(a, b, axis)
            extrema = [0., 1.] + ([-d / (3 * e)] if abs(e) > 1e-15 and 0 < -d / (3 * e) < 1 else [])
            acceptable &= max(abs((c + 2 * d * q + 3 * e * q * q) / dt) for q in extrema) <= maximum_velocity + .00001
            acceptable &= max(abs(2 * d), abs(2 * d + 6 * e)) / dt**2 <= maximum_acceleration + .0001
            if not acceptable:
                break
            for p in points[1:-1]:
                q = (p['source_ms'] - a['source_ms']) / (b['source_ms'] - a['source_ms'])
                value = a[axis] + q * (c + q * (d + q * e))
                if abs(value - p[axis]) > .0002:
                    acceptable = False
                    break
        if acceptable:
            return [a, b]
        mid = len(points) // 2
        return reduce_shot(points[:mid + 1])[:-1] + reduce_shot(points[mid:])
    result, shot = [], []
    for p in dense:
        if p['cut'] and shot:
            result.extend(reduce_shot(shot))
            shot = []
        shot.append(p)
    return result + reduce_shot(shot)


def detect(source, start, end):
    cv2.setNumThreads(2)
    cascade = cv2.CascadeClassifier(str(Path(cv2.data.haarcascades) / 'haarcascade_frontalface_default.xml'))
    if cascade.empty():
        raise ValueError('Classificatore locale assente')
    cap = cv2.VideoCapture(str(source), cv2.CAP_FFMPEG, [cv2.CAP_PROP_N_THREADS, 2])
    if not cap.isOpened():
        raise ValueError('Video non leggibile')
    samples, previous, last = [], None, None
    try:
        cap.set(cv2.CAP_PROP_POS_MSEC, start)
        due, previous_decode = start, -1
        while cap.grab():
            at = round(cap.get(cv2.CAP_PROP_POS_MSEC))
            if at <= previous_decode:
                raise ValueError('Timestamp decoder non monotoni: analisi non affidabile, usa composizione fissa/sfocata')
            previous_decode = at
            if at >= end:
                break
            if at < due:
                continue
            requested = due
            due = requested + 250
            ok, frame = cap.retrieve()
            if not ok:
                samples.append(dict(source_ms=at, requested_ms=requested, center=None, cut=False, faces=[], uncertainty='decode'))
                continue
            height, width = frame.shape[:2]
            small = cv2.resize(frame, (640, max(1, round(height * 640 / width))))
            gray = cv2.cvtColor(small, cv2.COLOR_BGR2GRAY)
            thumb = cv2.resize(gray, (48, 27)).astype('float32') / 255
            hist = cv2.calcHist([gray], [0], None, [32], [0, 256])
            cv2.normalize(hist, hist)
            cut = previous is not None and float(abs(thumb - previous[0]).mean()) > .22 and cv2.compareHist(hist, previous[1], cv2.HISTCMP_BHATTACHARYYA) > .5
            previous = (thumb, hist)
            if cut:
                last = None
            faces = [dict(x=float(x / gray.shape[1]), y=float(y / gray.shape[0]), width=float(w / gray.shape[1]), height=float(h / gray.shape[0])) for x, y, w, h in cascade.detectMultiScale(gray, scaleFactor=1.1, minNeighbors=6, minSize=(28, 28))]
            def score(f):
                center = (f['x'] + f['width'] / 2, f['y'] + f['height'] / 2)
                return math.dist(center, last) if last else -f['width'] * f['height']
            chosen = min(faces, key=score) if faces else None
            center = [chosen['x'] + chosen['width'] / 2, chosen['y'] + chosen['height'] / 2] if chosen else None
            uncertainty = 'multiple' if len(faces) > 1 else 'missing' if center is None else 'single'
            if center is not None:
                last = center
            samples.append(dict(source_ms=at, requested_ms=requested, decode_ms=at, center=center, cut=cut, faces=faces, uncertainty=uncertainty))
    finally:
        cap.release()
    if not samples or samples[-1]['source_ms'] < end:
        samples.append(dict(source_ms=end, center=None, cut=False, faces=[], uncertainty='end_boundary'))
    return samples


def trajectory(samples, stability=.65, response=.65, bounds=None, composed=False):
    if not samples:
        raise ValueError('Nessun campione')
    if not (0 <= stability <= 1 and 0 <= response <= 1):
        raise ValueError('Stabilità / reattività 0–1')
    bounds = bounds or [[0., 1.], [0., 1.]]
    def bounded(center):
        return [max(bounds[i][0], min(bounds[i][1], center[i])) for i in (0, 1)]
    position = bounded(samples[0].get('center') or [.5, .5])
    velocity, target, dense = [0., 0.], position[:], []
    previous_time = samples[0]['source_ms']
    def snapshot(at, cut=False):
        dense.append(dict(source_ms=at, x=position[0], y=position[1], vx=velocity[0], vy=velocity[1], cut=cut))
    for i, sample in enumerate(samples):
        at = sample['source_ms']
        cut = bool(sample.get('cut'))
        previous_target = target[:]
        observed = sample.get('center')
        # A lone distant detection is uncertain, not evidence of a new identity.
        if observed and not cut and math.dist(observed, target) > .28:
            following = samples[i + 1].get('center') if i + 1 < len(samples) else None
            if not following or math.dist(observed, following) > .15:
                observed = None
        if observed:
            target = list(observed)
            # Bounded offline anticipation of *observed* motion, within a shot.
            # It reduces framing lag during a fast move without following noise
            # or predicting through missing faces / cuts / subject uncertainty.
            if not composed and i > 0 and i + 1 < len(samples) and not cut:
                before, after = samples[i - 1], samples[i + 1]
                if before.get('center') and after.get('center') and not after.get('cut') and sample.get('uncertainty') != 'multiple' and math.dist(before['center'], observed) < .28 and math.dist(after['center'], observed) < .28:
                    span = (after['source_ms'] - before['source_ms']) / 1000
                    for axis in (0, 1):
                        drift = (after['center'][axis] - before['center'][axis]) / span
                        target[axis] = max(0, min(1, target[axis] + max(-.12, min(.12, drift * .1 * response))))
            target = bounded(target)
        if i:
            end = at - 1 if cut else at
            current = previous_time
            while current < end:
                next_time = min(end, current + 10)
                step = (next_time - current) / 1000
                for axis in (0, 1):
                    lower, upper = bounds[axis]
                    if upper == lower:
                        position[axis], velocity[axis] = lower, 0.
                        continue
                    error = (previous_target if cut else target)[axis] - position[axis]
                    crop_fraction = min(1., bounds[axis][0]*2) or 1.
                    zone = crop_fraction*(.12+.08*stability) if composed else .004 + .018 * stability
                    if abs(error) < zone:
                        error = 0.
                    else:
                        error -= math.copysign(zone, error)
                    # Critical damping with adaptive response to large movements.
                    omega = (2.5+3.5*response+4*min(1,abs(error)/.15)) if composed else 4 + 9 * response + 8 * min(1, abs(error) / .18) - 2 * stability
                    limit_a = .8+.7*response if composed else 3.
                    limit_v = .32+.18*response if composed else 1.
                    acceleration = max(-limit_a, min(limit_a, omega * omega * error - 2 * omega * velocity[axis]))
                    # Brake before the *crop* reaches an edge, rather than clip
                    # a moving camera against the boundary after interpolation.
                    speed = velocity[axis]
                    distance = upper - position[axis] if speed > 0 else position[axis] - lower
                    if speed and speed * speed / (2*limit_a) + abs(speed) * step * 2.5 + limit_a * step * step >= distance:
                        acceleration = -math.copysign(limit_a, speed)
                    old_velocity = velocity[axis]
                    velocity[axis] = max(-limit_v, min(limit_v, old_velocity + acceleration * step))
                    position[axis] = max(lower, min(upper, position[axis] + (old_velocity + velocity[axis]) * step / 2))
                    if position[axis] in (lower, upper):
                        velocity[axis] = 0.
                snapshot(next_time)
                current = next_time
        if cut:
            position, velocity = bounded(observed or [.5, .5]), [0., 0.]
            target = position[:]
        if not i or cut:
            snapshot(at, cut)
        previous_time = at
    points = simplify(dense, .32+.18*response if composed else 1., .8+.7*response if composed else 3.)
    return dict(version=VERSION, enabled=True, stability=stability, response=response, points=points, samples=samples, review_required=True)


def composed_trajectory(samples, stability=.65, response=.65, bounds=None, behavior='composed'):
    """Compose a shot, not every detection. Uncertain coverage is NOT a safe crop."""
    bounds = bounds or [[0., 1.], [0., 1.]]
    prepared, shots = [], []
    for s in samples:
        if not shots or s.get('cut'):
            shots.append([])
        shots[-1].append(s)
    fixed_shots, safe_shots = 0, 0
    for shot in shots:
        valid = [s for s in shot if s.get('center') and s.get('uncertainty') != 'multiple']
        early = [s for s in valid if s['source_ms']-shot[0]['source_ms'] <= 750][:3]
        anchor = [statistics.median(s['center'][a] for s in early) if early else .5 for a in (0,1)]
        fixed, contained = [], []
        for axis in (0,1):
            lo, hi = bounds[axis]
            half = lo if lo else .5
            low, high = lo, hi
            for s in valid:
                face = min(s.get('faces',[]),key=lambda f:math.dist([f['x']+f['width']/2,f['y']+f['height']/2],s['center']),default=None)
                radius = (face['width' if axis==0 else 'height']/2) if face else 0.
                low = max(low, s['center'][axis]+radius+.025-half)
                high = min(high, s['center'][axis]-radius-.025+half)
            can_lock = bool(valid) and low<=high
            contained.append(can_lock or lo==hi)
            fixed.append(can_lock or lo==hi or behavior=='fixed')
            anchor[axis] = max(lo,min(hi,max(low,min(high,anchor[axis])) if can_lock else anchor[axis]))
        fixed_shots += int(all(fixed))
        safe_shots += int(all(contained))
        for n,s in enumerate(shot):
            center = s.get('center') if s.get('uncertainty')!='multiple' else None
            nearby = [v['center'] for v in shot[max(0,n-1):n+2] if v.get('center') and v.get('uncertainty')!='multiple']
            if center:
                center = [anchor[a] if fixed[a] else statistics.median(v[a] for v in nearby) for a in (0,1)]
            if n==0:
                center = anchor[:]
            prepared.append({**s,'center':center})
    result = trajectory(prepared, stability, response, bounds, composed=True)
    useful = [s for s in samples if s.get('uncertainty')!='end_boundary']
    detected = sum(bool(s.get('center')) and s.get('uncertainty')!='multiple' for s in useful)
    coverage = detected/max(1,len(useful))
    gap,last = 0,samples[0]['source_ms']
    for s in samples:
        if s.get('center'):
            gap=max(gap,s['source_ms']-last);last=s['source_ms']
    gap=max(gap,samples[-1]['source_ms']-last)
    safe = coverage>=.8 and safe_shots==len(shots) and gap<=750
    result.update(version='camera-cpu-v3',behavior=behavior,samples=samples,
                  preferred_fit='cover' if safe else 'blur',
                  advice=('Inquadratura quasi fissa: i rilevamenti osservati rientrano nel crop con margine.' if safe else 'Crop non certificato: movimento, perdita volto o ambiguità. Auto preferisce sfondo sfocato; Riempi resta una scelta manuale da rivedere.'),
                  diagnostics=dict(coverage=coverage,maximum_gap_ms=gap,shots=len(shots),fixed_shots=fixed_shots,cover_safe_shots=safe_shots,
                                   timestamp_source='decoded PTS, not requested seek',perceptual_acceptance=False))
    return result


if __name__ == '__main__':
    request = json.loads(Path(sys.argv[1]).read_text(encoding='utf-8'))
    cache = Path(request['cache'])
    reused = cache.exists()
    samples = json.loads(cache.read_text(encoding='utf-8')) if reused else detect(request['source'], request['start_ms'], request['end_ms'])
    if not reused:
        cache.write_text(json.dumps(samples), encoding='utf-8')
    behavior = request.get('behavior','legacy')
    result = trajectory(samples, request['stability'], request['response'], request.get('bounds')) if behavior=='legacy' else composed_trajectory(samples, request['stability'], request['response'], request.get('bounds'),behavior)
    if behavior=='legacy':
        result.update(behavior='legacy',advice='Inseguimento precedente per confronto: preferisci Camera composta per nuovi lavori.')
    result['cache_hit'] = reused
    Path(sys.argv[2]).write_text(json.dumps(result), encoding='utf-8')
