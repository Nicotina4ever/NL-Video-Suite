# -*- coding: utf-8 -*-
"""
audio_events.py — Rilevamento eventi audio (copiato dal progetto Diarizzazione,
Diarizzazione/src/audio_events.py — NESSUNA modifica là, solo copia qui).

Classificatore neurale MIT/AST (AudioSet): scorre l'audio a finestre e
etichetta cosa contiene (fiati, risate, musica, effetti, silenzio...).

UNICA DIFFERENZA rispetto all'originale: la categoria "Verso"
(breathing/gasp/pant/sigh/...) è rinominata "Fiato" per le annotazioni [S#].
"""

import math
from typing import Any

import numpy as np


AUDIO_EVENT_MODEL = "MIT/ast-finetuned-audioset-10-10-0.4593"

CATEGORY_RULES = {
    "Canto": ("singing", "song", "choir", "yodeling", "chant", "lullaby", "a capella", "vocal music", "rapping", "humming"),
    "Risata": ("laughter", "giggle", "snicker", "chuckle", "belly laugh"),
    "Pianto": ("crying", "sobbing", "wail", "whimper"),
    "Urlo": ("screaming", "shout", "yell", "battle cry", "whoop"),
    "Sussurro": ("whisper",),
    "Applausi": ("applause", "cheering"),
    "Silenzio": ("silence",),
    "Musica": ("music", "musical instrument", "piano", "guitar", "orchestra", "synthesizer", "soundtrack", "theme music"),
    "Effetti": ("sound effect", "explosion", "boom", "bang", "crash", "whoosh", "gunshot", "door", "knock", "alarm", "siren"),
    "Fiato": ("breathing", "gasp", "pant", "groan", "grunt", "sigh", "hiccup", "snowing", "sneeze", "cough"),
}

THRESHOLDS = {
    "Canto": 0.035,
    "Risata": 0.08,
    "Pianto": 0.08,
    "Urlo": 0.08,
    "Sussurro": 0.08,
    "Applausi": 0.08,
    "Silenzio": 0.10,
    "Musica": 0.08,
    "Effetti": 0.08,
    "Fiato": 0.08,
}


def _category_for_label(label: str) -> str | None:
    normalized = label.lower()
    for category, tokens in CATEGORY_RULES.items():
        if any(token in normalized for token in tokens):
            return category
    return None


def _merge_events(events: list[dict[str, Any]], max_gap: float = 1.5) -> list[dict[str, Any]]:
    merged: list[dict[str, Any]] = []
    for event in sorted(events, key=lambda item: (item["label"], item["start"])):
        if not merged or merged[-1]["label"] != event["label"] or event["start"] - merged[-1]["end"] > max_gap:
            merged.append(event.copy())
            continue
        previous = merged[-1]
        old_duration = max(previous["end"] - previous["start"], 0.001)
        new_duration = max(event["end"] - event["start"], 0.001)
        combined = old_duration + new_duration
        previous["confidence"] = round(
            ((previous["confidence"] * old_duration) + (event["confidence"] * new_duration)) / combined,
            3,
        )
        previous["end"] = max(previous["end"], event["end"])
        previous["duration"] = round(previous["end"] - previous["start"], 3)
    return merged


def detect_pauses(segments: list[dict[str, Any]], min_pause: float = 0.8) -> list[dict[str, Any]]:
    pauses: list[dict[str, Any]] = []
    ordered = sorted(segments, key=lambda segment: float(segment.get("start", 0.0)))
    for previous, current in zip(ordered, ordered[1:]):
        start = float(previous.get("end", 0.0))
        end = float(current.get("start", 0.0))
        duration = end - start
        if duration >= min_pause:
            pauses.append(
                {
                    "kind": "pause",
                    "label": "Pausa",
                    "start": round(start, 3),
                    "end": round(end, 3),
                    "duration": round(duration, 3),
                    "confidence": 1.0,
                    "source": "timeline",
                }
            )
    return pauses


def attach_segment_tags(segments: list[dict[str, Any]], events: list[dict[str, Any]]) -> None:
    for segment in segments:
        start = float(segment.get("start", 0.0))
        end = float(segment.get("end", start))
        tags: list[str] = []
        for event in sorted(events, key=lambda item: item["start"]):
            if event.get("kind") == "pause":
                continue
            event_start = float(event.get("start", 0.0))
            event_end = float(event.get("end", 0.0))
            overlap = max(0.0, min(end, event_end) - max(start, event_start))
            if overlap >= 0.25 or (end > start and overlap / (end - start) >= 0.2):
                if event["label"] not in tags:
                    tags.append(event["label"])
        segment["tags"] = tags


def detect_audio_events(
    audio: np.ndarray,
    sample_rate: int,
    model_name: str,
    cache_dir: str,
    device: str,
    window_seconds: float = 10.0,
    hop_seconds: float = 5.0,
    batch_size: int = 4,
    progress_callback=None,
) -> list[dict[str, Any]]:
    from transformers import AutoFeatureExtractor, AutoModelForAudioClassification
    import torch

    if audio.size == 0:
        return []

    extractor = AutoFeatureExtractor.from_pretrained(model_name, cache_dir=cache_dir)
    model = AutoModelForAudioClassification.from_pretrained(model_name, cache_dir=cache_dir)
    model.to(device)
    model.eval()

    window_samples = max(1, int(window_seconds * sample_rate))
    hop_samples = max(1, int(hop_seconds * sample_rate))
    chunks: list[tuple[float, float, np.ndarray]] = []
    start = 0
    while start < audio.size:
        end = min(audio.size, start + window_samples)
        if end - start >= int(1.5 * sample_rate):
            chunks.append((start / sample_rate, end / sample_rate, audio[start:end]))
        if end == audio.size:
            break
        start += hop_samples

    if not chunks:
        return []

    events: list[dict[str, Any]] = []
    effective_batch = max(1, min(batch_size, 8))
    label_map = model.config.id2label

    try:
        for batch_start in range(0, len(chunks), effective_batch):
            batch = chunks[batch_start : batch_start + effective_batch]
            inputs = extractor(
                [chunk for _, _, chunk in batch],
                sampling_rate=sample_rate,
                return_tensors="pt",
            )
            input_values = inputs["input_values"].to(device)
            with torch.inference_mode():
                logits = model(input_values).logits
                probabilities = torch.softmax(logits.float(), dim=-1).cpu().numpy()

            for (begin, end, _), scores in zip(batch, probabilities):
                category_scores: dict[str, float] = {}
                for index in np.argsort(scores)[::-1][:12]:
                    label = str(label_map.get(int(index), f"LABEL_{index}"))
                    category = _category_for_label(label)
                    if not category:
                        continue
                    score = float(scores[index])
                    if score > category_scores.get(category, 0.0):
                        category_scores[category] = score

                for category, score in sorted(category_scores.items(), key=lambda item: item[1], reverse=True):
                    if score >= THRESHOLDS.get(category, 0.08):
                        events.append(
                            {
                                "kind": "audio-event",
                                "label": category,
                                "start": round(begin, 3),
                                "end": round(end, 3),
                                "duration": round(end - begin, 3),
                                "confidence": round(score, 3),
                                "source": model_name,
                            }
                        )

            if progress_callback:
                progress_callback(batch_start + len(batch), len(chunks))
    finally:
        del model
        if device != "cpu":
            try:
                torch.cuda.empty_cache()
            except RuntimeError:
                pass

    return _merge_events(events)


def analyze_events(
    audio: np.ndarray,
    sample_rate: int,
    segments: list[dict[str, Any]],
    enabled: bool,
    model_name: str,
    cache_dir: str,
    device: str,
    min_pause: float,
    window_seconds: float,
    hop_seconds: float,
    batch_size: int,
    log=print,
) -> list[dict[str, Any]]:
    events = detect_pauses(segments, min_pause)
    if enabled:
        try:
            audio_events = detect_audio_events(
                audio=audio,
                sample_rate=sample_rate,
                model_name=model_name,
                cache_dir=cache_dir,
                device=device,
                window_seconds=window_seconds,
                hop_seconds=hop_seconds,
                batch_size=batch_size,
                progress_callback=lambda done, total: log(f"Eventi audio: {done}/{total}", end="\r"),
            )
            events.extend(audio_events)
            if audio_events:
                log("\nEventi audio rilevati:                    " + str(len(audio_events)))
        except Exception as exc:
            log(f"\nAnalisi eventi audio non completata: {exc}")
            events.append(
                {
                    "kind": "warning",
                    "label": "Analisi eventi non completata",
                    "start": 0.0,
                    "end": 0.0,
                    "duration": 0.0,
                    "confidence": 0.0,
                    "source": "pipeline",
                    "message": str(exc),
                }
            )
    attach_segment_tags(segments, events)
    return sorted(events, key=lambda event: (float(event.get("start", 0.0)), str(event.get("label", ""))))
