"""Bounded CPU face-location suggestion, not identity or motion tracking.

OpenCV Haar data ships with the explicitly approved headless wheel. No network,
model download or source mutation. Ambiguous/moving/missing faces -> contain.
"""
import hashlib
import json
from pathlib import Path
import sys
import cv2

VERSION = "haar-static-v1"


def suggest(source, ranges):
    cv2.setNumThreads(2)
    classifier_path = Path(cv2.data.haarcascades) / "haarcascade_frontalface_default.xml"
    cascade = cv2.CascadeClassifier(str(classifier_path))
    if cascade.empty():
        raise RuntimeError("Classificatore Haar locale assente")
    points = []
    # At most seven frames, distributed over the full set of kept source spans.
    duration = sum(r["end_ms"] - r["start_ms"] for r in ranges)
    for fraction in (.05, .2, .35, .5, .65, .8, .95):
        offset = round(duration * fraction)
        for r in ranges:
            length = r["end_ms"] - r["start_ms"]
            if offset < length:
                points.append(r["start_ms"] + offset)
                break
            offset -= length
    capture = cv2.VideoCapture(str(source), cv2.CAP_FFMPEG, [cv2.CAP_PROP_N_THREADS, 2])
    if not capture.isOpened():
        raise RuntimeError("Impossibile leggere il video per il reframe CPU")
    samples, centers = [], []
    try:
        for at in points:
            capture.set(cv2.CAP_PROP_POS_MSEC, at)
            ok, frame = capture.read()
            if not ok:
                samples.append({"source_ms": at, "faces": [], "decoded": False})
                continue
            h, w = frame.shape[:2]
            ratio = min(1, 640 / w)
            small = cv2.resize(frame, (round(w * ratio), round(h * ratio)))
            gray = cv2.cvtColor(small, cv2.COLOR_BGR2GRAY)
            faces = cascade.detectMultiScale(gray, scaleFactor=1.1, minNeighbors=5, minSize=(30, 30))
            faces = [{"x": round(x / small.shape[1], 6), "y": round(y / small.shape[0], 6),
                      "width": round(fw / small.shape[1], 6), "height": round(fh / small.shape[0], 6)}
                     for x, y, fw, fh in faces]
            samples.append({"source_ms": at, "faces": faces, "decoded": True})
            if len(faces) == 1:
                face = faces[0]
                centers.append((face["x"] + face["width"] / 2, face["y"] + face["height"] / 2))
    finally:
        capture.release()
    stable = len(centers) >= 5 and all(len(s["faces"]) <= 1 and s["decoded"] for s in samples)
    stable = stable and all(max(p[i] for p in centers) - min(p[i] for p in centers) < .12 for i in (0, 1))
    crop_fraction = min(1., (9 / 16) / (w / h)) if samples else 1.
    crop_height = min(1., (w / h) / (9 / 16)) if samples else 1.
    stable = stable and all(face['width'] + .06 <= crop_fraction and face['height'] + .08 <= crop_height
                            for sample in samples for face in sample['faces'])
    if stable:
        # Cover's focus is crop travel, not the subject's normalized coordinate.
        x, y = (sum(p[i] for p in centers) / len(centers) for i in (0, 1))
        focus_x = min(1, max(0, (x - crop_fraction / 2) / (1 - crop_fraction))) if crop_fraction < 1 else .5
        focus_y = min(1, max(0, (y - crop_height / 2) / (1 - crop_height))) if crop_height < 1 else .5
        transform = {"fit": "cover", "focus_x": focus_x, "focus_y": focus_y}
        reason = "Un solo volto in almeno 5/7 campioni, posizione orizzontale stabile. Verifica visivamente tutto il clip."
    else:
        transform = {"fit": "contain", "focus_x": .5, "focus_y": .5}
        reason = "Volti assenti, multipli, mobili o campioni insufficienti: conservata l'intera inquadratura. Usa il crop manuale."
    return {"engine": VERSION, "opencv_version": cv2.__version__, "device": "cpu", "tracking": False,
            "classifier_sha256": hashlib.sha256(classifier_path.read_bytes()).hexdigest(),
            "suggestion": transform, "reason": reason, "samples": samples, "review_required": True}


if __name__ == "__main__":
    request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    result = suggest(Path(request["source"]), request["ranges"])
    Path(sys.argv[2]).write_text(json.dumps(result, ensure_ascii=False), encoding="utf-8")
