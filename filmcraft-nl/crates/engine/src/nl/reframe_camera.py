"""Camera trajectory for the automatic reframe, ported from NL Video Suite's own camera-cpu-v3.

Source time, not timeline time: trimming or splitting a clip does not restart the camera.
No neural network and no download: OpenCV's bundled Haar cascade detects faces, a scene cut is a
large thumbnail/histogram change, and the camera follows the chosen target with a bounded,
critically damped path. This is our own pre-fork algorithm (`sviluppo/bridge/camera_path.py`),
not code from a third-party repository.

Usage: python reframe_camera.py request.json
The request is JSON:
  {"source": path, "start_ms": int, "end_ms": int, "stability": 0..1, "response": 0..1,
   "behavior": "legacy"|"composed", "bounds": [[lo,hi],[lo,hi]]?}
or, when OpenCV cannot read the video, the Rust side extracts frames and sends:
  {"frames": [png...], "times_ms": [int...], "stability":..., "response":...,
   "behavior":..., "bounds":...}
The reply is JSON on stdout: points, samples, preferred_fit, advice, diagnostics.
"""

import json
import math
import statistics
import sys
from pathlib import Path

import cv2

VERSION = "camera-cpu-v3"
SAMPLE_MS = 250
THUMB = (48, 27)


def _detector():
    cascade = cv2.CascadeClassifier(str(Path(cv2.data.haarcascades) / "haarcascade_frontalface_default.xml"))
    if cascade.empty():
        raise ValueError("classificatore Haar locale assente")
    return cascade


def _faces(cascade, frame):
    height, width = frame.shape[:2]
    scale = 640.0 / max(1, width)
    small = cv2.resize(frame, (640, max(1, int(round(height * scale)))))
    gray = cv2.cvtColor(small, cv2.COLOR_BGR2GRAY)
    thumb = cv2.resize(gray, THUMB).astype("float32") / 255.0
    hist = cv2.calcHist([gray], [0], None, [32], [0, 256])
    cv2.normalize(hist, hist)
    boxes = cascade.detectMultiScale(gray, scaleFactor=1.1, minNeighbors=6, minSize=(28, 28))
    faces = [
        {
            "x": float(x / gray.shape[1]),
            "y": float(y / gray.shape[0]),
            "width": float(w / gray.shape[1]),
            "height": float(h / gray.shape[0]),
        }
        for x, y, w, h in boxes
    ]
    return thumb, hist, faces


def _append(cascade, samples, at, frame, previous, last):
    if frame is None:
        samples.append({"source_ms": at, "center": None, "cut": False, "faces": [], "uncertainty": "decode"})
        return previous, last
    thumb, hist, faces = _faces(cascade, frame)
    cut = previous is not None and float(abs(thumb - previous[0]).mean()) > 0.22 and cv2.compareHist(hist, previous[1], cv2.HISTCMP_BHATTACHARYYA) > 0.5
    previous = (thumb, hist)
    if cut:
        last = None
    if faces:
        # Same metric as the original: nearest to the locked target, else the largest face.
        def score(face):
            center = (face["x"] + face["width"] / 2, face["y"] + face["height"] / 2)
            if last:
                return math.dist(center, last)
            return -face["width"] * face["height"]

        chosen = min(faces, key=score)
        center = [chosen["x"] + chosen["width"] / 2, chosen["y"] + chosen["height"] / 2]
        last = center
    else:
        center = None
    uncertainty = "multiple" if len(faces) > 1 else ("missing" if center is None else "single")
    samples.append({"source_ms": at, "center": center, "cut": cut, "faces": faces, "uncertainty": uncertainty})
    return previous, last


def detect_video(source, start_ms, end_ms):
    cascade = _detector()
    cv2.setNumThreads(2)
    cap = cv2.VideoCapture(str(source), cv2.CAP_FFMPEG, [cv2.CAP_PROP_N_THREADS, 2])
    if not cap.isOpened():
        raise ValueError("video non leggibile da OpenCV")
    samples, previous, last = [], None, None
    try:
        fps = float(cap.get(cv2.CAP_PROP_FPS) or 0.0)
        if fps < 1.0 or fps > 480.0:
            fps = 30.0
        first = max(0, int(round(start_ms / 1000.0 * fps)))
        end_frame = max(first + 1, int(round(end_ms / 1000.0 * fps)))
        step = max(1, int(round(fps * SAMPLE_MS / 1000.0)))
        cap.set(cv2.CAP_PROP_POS_FRAMES, first)
        index = first
        while index < end_frame:
            ok, frame = cap.read()
            if not ok:
                break
            if (index - first) % step == 0:
                at = int(round(index * 1000.0 / fps))
                previous, last = _append(cascade, samples, at, frame, previous, last)
            index += 1
    finally:
        cap.release()
    if not samples or samples[-1]["source_ms"] < end_ms:
        samples.append({"source_ms": end_ms, "center": None, "cut": False, "faces": [], "uncertainty": "end_boundary"})
    return samples


def detect_frames(paths, times_ms):
    cascade = _detector()
    samples, previous, last = [], None, None
    for path, at in zip(paths, times_ms):
        image = cv2.imread(str(path))
        previous, last = _append(cascade, samples, int(at), image, previous, last)
    if not samples:
        raise ValueError("nessun frame da analizzare")
    samples.append({"source_ms": int(times_ms[-1]), "center": None, "cut": False, "faces": [], "uncertainty": "end_boundary"})
    return samples


def _coefficients(a, b, axis):
    dt = (b["source_ms"] - a["source_ms"]) / 1000.0
    va, vb = a["v" + axis], b["v" + axis]
    return dt, dt * va, 3 * (b[axis] - a[axis]) - dt * (2 * va + vb), 2 * (a[axis] - b[axis]) + dt * (va + vb)


def simplify(dense, maximum_velocity=1.0, maximum_acceleration=3.0):
    """Adaptive curve limits, not just bounds on the sampled knots."""

    def reduce_shot(points):
        if len(points) <= 2:
            return points
        a, b = points[0], points[-1]
        acceptable = True
        for axis in ("x", "y"):
            dt, c, d, e = _coefficients(a, b, axis)
            inner = [-d / (3 * e)] if abs(e) > 1e-15 and 0 < -d / (3 * e) < 1 else []
            extrema = [0.0, 1.0] + inner
            if max(abs((c + 2 * d * q + 3 * e * q * q) / dt) for q in extrema) > maximum_velocity + 0.00001:
                acceptable = False
                break
            if max(abs(2 * d), abs(2 * d + 6 * e)) / dt**2 > maximum_acceleration + 0.0001:
                acceptable = False
                break
            for p in points[1:-1]:
                q = (p["source_ms"] - a["source_ms"]) / (b["source_ms"] - a["source_ms"])
                value = a[axis] + q * (c + q * (d + q * e))
                if abs(value - p[axis]) > 0.0002:
                    acceptable = False
                    break
            if not acceptable:
                break
        if acceptable:
            return [a, b]
        mid = len(points) // 2
        return reduce_shot(points[: mid + 1])[:-1] + reduce_shot(points[mid:])

    result, shot = [], []
    for point in dense:
        if point["cut"] and shot:
            result.extend(reduce_shot(shot))
            shot = []
        shot.append(point)
    return result + reduce_shot(shot)


def trajectory(samples, stability=0.65, response=0.65, bounds=None, composed=False):
    if not samples:
        raise ValueError("nessun campione")
    if not (0 <= stability <= 1 and 0 <= response <= 1):
        raise ValueError("stabilità/reattività 0–1")
    bounds = bounds or [[0.0, 1.0], [0.0, 1.0]]

    def bounded(center):
        return [max(bounds[i][0], min(bounds[i][1], center[i])) for i in (0, 1)]

    position = bounded(samples[0].get("center") or [0.5, 0.5])
    velocity, target, dense = [0.0, 0.0], position[:], []
    previous_time = samples[0]["source_ms"]

    def snapshot(at, cut=False):
        dense.append(dict(source_ms=at, x=position[0], y=position[1], vx=velocity[0], vy=velocity[1], cut=cut))

    for i, sample in enumerate(samples):
        at = sample["source_ms"]
        cut = bool(sample.get("cut"))
        previous_target = target[:]
        observed = sample.get("center")
        # A lone distant detection is uncertain, not evidence of a new identity.
        if observed and not cut and math.dist(observed, target) > 0.28:
            following = samples[i + 1].get("center") if i + 1 < len(samples) else None
            if not following or math.dist(observed, following) > 0.15:
                observed = None
        if observed:
            target = list(observed)
            if not composed and i > 0 and i + 1 < len(samples) and not cut:
                before, after = samples[i - 1], samples[i + 1]
                if (
                    before.get("center")
                    and after.get("center")
                    and not after.get("cut")
                    and sample.get("uncertainty") != "multiple"
                    and math.dist(before["center"], observed) < 0.28
                    and math.dist(after["center"], observed) < 0.28
                ):
                    span = (after["source_ms"] - before["source_ms"]) / 1000.0
                    for axis in (0, 1):
                        drift = (after["center"][axis] - before["center"][axis]) / span
                        target[axis] = max(0, min(1, target[axis] + max(-0.12, min(0.12, drift * 0.1 * response))))
            target = bounded(target)
        if i:
            end = at - 1 if cut else at
            current = previous_time
            while current < end:
                next_time = min(end, current + 10)
                step = (next_time - current) / 1000.0
                for axis in (0, 1):
                    lower, upper = bounds[axis]
                    if upper == lower:
                        position[axis], velocity[axis] = lower, 0.0
                        continue
                    error = (previous_target if cut else target)[axis] - position[axis]
                    crop_fraction = min(1.0, bounds[axis][0] * 2) or 1.0
                    zone = crop_fraction * (0.12 + 0.08 * stability) if composed else 0.004 + 0.018 * stability
                    if abs(error) < zone:
                        error = 0.0
                    else:
                        error -= math.copysign(zone, error)
                    omega = (
                        (2.5 + 3.5 * response + 4 * min(1, abs(error) / 0.15))
                        if composed
                        else 4 + 9 * response + 8 * min(1, abs(error) / 0.18) - 2 * stability
                    )
                    limit_a = 0.8 + 0.7 * response if composed else 3.0
                    limit_v = 0.32 + 0.18 * response if composed else 1.0
                    acceleration = max(-limit_a, min(limit_a, omega * omega * error - 2 * omega * velocity[axis]))
                    speed = velocity[axis]
                    distance = upper - position[axis] if speed > 0 else position[axis] - lower
                    if speed and speed * speed / (2 * limit_a) + abs(speed) * step * 2.5 + limit_a * step * step >= distance:
                        acceleration = -math.copysign(limit_a, speed)
                    old_velocity = velocity[axis]
                    velocity[axis] = max(-limit_v, min(limit_v, old_velocity + acceleration * step))
                    position[axis] = max(lower, min(upper, position[axis] + (old_velocity + velocity[axis]) * step / 2))
                    if position[axis] in (lower, upper):
                        velocity[axis] = 0.0
                snapshot(next_time)
                current = next_time
        if cut:
            position, velocity = bounded(observed or [0.5, 0.5]), [0.0, 0.0]
            target = position[:]
        if not i or cut:
            snapshot(at, cut)
        previous_time = at
    points = simplify(dense, 0.32 + 0.18 * response if composed else 1.0, 0.8 + 0.7 * response if composed else 3.0)
    return dict(version=VERSION, enabled=True, stability=stability, response=response, points=points, samples=samples, review_required=True)


def composed_trajectory(samples, stability=0.65, response=0.65, bounds=None, behavior="composed"):
    """Compose a shot, not every detection. Uncertain coverage is not a safe crop."""
    bounds = bounds or [[0.0, 1.0], [0.0, 1.0]]
    prepared, shots = [], []
    for sample in samples:
        if not shots or sample.get("cut"):
            shots.append([])
        shots[-1].append(sample)
    fixed_shots, safe_shots = 0, 0
    for shot in shots:
        valid = [s for s in shot if s.get("center") and s.get("uncertainty") != "multiple"]
        early = [s for s in valid if s["source_ms"] - shot[0]["source_ms"] <= 750][:3]
        anchor = [statistics.median(s["center"][a] for s in early) if early else 0.5 for a in (0, 1)]
        fixed, contained = [], []
        for axis in (0, 1):
            lo, hi = bounds[axis]
            half = lo if lo else 0.5
            low, high = lo, hi
            for s in valid:
                face = min(
                    s.get("faces", []),
                    key=lambda f: math.dist([f["x"] + f["width"] / 2, f["y"] + f["height"] / 2], s["center"]),
                    default=None,
                )
                radius = (face["width" if axis == 0 else "height"] / 2) if face else 0.0
                low = max(low, s["center"][axis] + radius + 0.025 - half)
                high = min(high, s["center"][axis] - radius - 0.025 + half)
            can_lock = bool(valid) and low <= high
            contained.append(can_lock or lo == hi)
            fixed.append(can_lock or lo == hi or behavior == "fixed")
            anchor[axis] = max(lo, min(hi, max(low, min(high, anchor[axis])) if can_lock else anchor[axis]))
        fixed_shots += int(all(fixed))
        safe_shots += int(all(contained))
        for n, s in enumerate(shot):
            center = s.get("center") if s.get("uncertainty") != "multiple" else None
            nearby = [v["center"] for v in shot[max(0, n - 1) : n + 2] if v.get("center") and v.get("uncertainty") != "multiple"]
            if center:
                center = [anchor[a] if fixed[a] else statistics.median(v[a] for v in nearby) for a in (0, 1)]
            if n == 0:
                center = anchor[:]
            prepared.append({**s, "center": center})
    result = trajectory(prepared, stability, response, bounds, composed=True)
    useful = [s for s in samples if s.get("uncertainty") != "end_boundary"]
    detected = sum(bool(s.get("center")) and s.get("uncertainty") != "multiple" for s in useful)
    coverage = detected / max(1, len(useful))
    gap, last = 0, samples[0]["source_ms"]
    for s in samples:
        if s.get("center"):
            gap = max(gap, s["source_ms"] - last)
            last = s["source_ms"]
    gap = max(gap, samples[-1]["source_ms"] - last)
    safe = coverage >= 0.8 and safe_shots == len(shots) and gap <= 750
    result.update(
        version="camera-cpu-v3",
        behavior=behavior,
        samples=samples,
        preferred_fit="cover" if safe else "blur",
        advice=(
            "Inquadratura quasi fissa: i rilevamenti osservati rientrano nel crop con margine."
            if safe
            else "Crop non certificato: movimento, perdita volto o ambiguità. Rivedi il reframe; il fit resta cover."
        ),
        diagnostics=dict(
            coverage=coverage,
            maximum_gap_ms=gap,
            shots=len(shots),
            fixed_shots=fixed_shots,
            cover_safe_shots=safe_shots,
            timestamp_source="decoded PTS, not requested seek",
            perceptual_acceptance=False,
        ),
    )
    return result


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: reframe_camera.py request.json")
    request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    if request.get("frames"):
        samples = detect_frames(request["frames"], request["times_ms"])
    else:
        samples = detect_video(request["source"], int(request["start_ms"]), int(request["end_ms"]))
    stability = float(request.get("stability", 0.65))
    response = float(request.get("response", 0.65))
    bounds = request.get("bounds")
    behavior = request.get("behavior", "composed")
    if behavior == "legacy":
        result = trajectory(samples, stability, response, bounds)
        result.update(behavior="legacy", preferred_fit="cover", advice="Inseguimento precedente per confronto: preferisci Camera composta per nuovi lavori.")
    else:
        result = composed_trajectory(samples, stability, response, bounds, behavior)
    # The Rust side only needs the path and a compact diagnostic; faces are dropped.
    result["samples"] = [
        {"source_ms": s["source_ms"], "center": s.get("center"), "cut": s.get("cut", False), "uncertainty": s.get("uncertainty", "")}
        for s in result.get("samples", [])
    ]
    print(json.dumps(result, separators=(",", ":")))


if __name__ == "__main__":
    main()
