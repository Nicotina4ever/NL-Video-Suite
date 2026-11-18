"""Short candidate model and scoring for ZP Shorts."""

from __future__ import annotations

from dataclasses import dataclass, field, asdict
import json
import uuid
from typing import Any, Optional


class ShortValidationError(ValueError):
    """Raised when a ShortCandidate fails constraint validation."""
    pass


# ZP Shorts Fundamental Editorial Categories
# Content is always anchored to REAL WORK CASES (no tutorials, no gurus, no prescriptive lessons).
EDITORIAL_CATEGORIES: set[str] = {
    "unclassified",          # Baseline chunk with no editorial judgement yet
    "real_work_case",          # Caso reale di lavoro / situazione concreta
    "behind_the_scenes",       # Dietro le quinte autentico della sessione
    "technical_observation",   # Osservazione tecnica nata dalla sessione
    "decision_in_context",     # Decisione o scelta presa sul momento
    "problem_in_session",      # Imprevisto, dubbio o errore in corso d'opera
    "experiment",              # Tentativo, prova o test A/B sul campo
    "performance",             # Esecuzione / recitazione / take reale
    "human_moment",            # Momento umano, reazione spontanea o stacco
    "workflow_moment",         # Passaggio pratico nel flusso di lavoro
}

EDITORIAL_ROLES: set[str] = {"single", "compilation_member", "both"}

CANDIDATE_RELATIONS: set[str] = {
    "setup_for",
    "payoff_of",
    "follows",
    "contrasts_with",
    "same_problem",
    "same_workflow",
}

# Legacy category mapping for backward compatibility
LEGACY_CATEGORY_MAP: dict[str, str] = {
    "mini_tutorial": "workflow_moment",
    "technical_concept": "technical_observation",
    "common_mistake": "problem_in_session",
    "demonstration": "performance",
    "hook_statement": "real_work_case",
    "opinion": "decision_in_context",
    "qa": "decision_in_context",
    "story": "behind_the_scenes",
}


def normalize_category(category: str) -> str:
    """Normalize and map legacy categories to the modern ZP Shorts editorial categories."""
    clean = str(category).strip().lower()
    if clean in EDITORIAL_CATEGORIES:
        return clean
    if clean in LEGACY_CATEGORY_MAP:
        return LEGACY_CATEGORY_MAP[clean]
    return clean or "real_work_case"


@dataclass
class ShortScore:
    """Individual and composite editorial evaluation scores (0.0 to 1.0)."""

    standalone: float
    hook: float
    value: float
    closure: float
    overall: float

    def __post_init__(self) -> None:
        self.standalone = round(float(self.standalone), 3)
        self.hook = round(float(self.hook), 3)
        self.value = round(float(self.value), 3)
        self.closure = round(float(self.closure), 3)
        self.overall = round(float(self.overall), 3)

    def validate(self) -> None:
        for name, val in [
            ("standalone", self.standalone),
            ("hook", self.hook),
            ("value", self.value),
            ("closure", self.closure),
            ("overall", self.overall),
        ]:
            if not (0.0 <= val <= 1.0):
                raise ShortValidationError(
                    f"Score '{name}' must be between 0.0 and 1.0, got {val}"
                )

    def to_dict(self) -> dict[str, float]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ShortScore:
        standalone = float(data.get("standalone", 0.0))
        hook = float(data.get("hook", 0.0))
        value = float(data.get("value", 0.0))
        closure = float(data.get("closure", 0.0))
        # If overall is not provided, calculate a weighted average
        if "overall" in data and data["overall"] is not None:
            overall = float(data["overall"])
        else:
            overall = (standalone * 0.35) + (hook * 0.25) + (value * 0.25) + (closure * 0.15)
        return cls(
            standalone=standalone,
            hook=hook,
            value=value,
            closure=closure,
            overall=overall,
        )


@dataclass
class HumanReview:
    """Human editorial review metadata and boundary overrides."""

    status: str = "unreviewed"  # unreviewed, approved, rejected, needs_revision
    selection: Optional[Any] = None
    cut_in: Optional[float] = None
    cut_out: Optional[float] = None
    notes: str = ""
    reviewed_at: Optional[str] = None

    ALLOWED_STATUSES = {"unreviewed", "approved", "rejected", "needs_revision"}

    def __post_init__(self) -> None:
        if self.status not in self.ALLOWED_STATUSES:
            raise ShortValidationError(
                f"Invalid human_review status '{self.status}'. Allowed: {sorted(self.ALLOWED_STATUSES)}"
            )
        if self.cut_in is not None:
            self.cut_in = round(float(self.cut_in), 3)
        if self.cut_out is not None:
            self.cut_out = round(float(self.cut_out), 3)
        if self.cut_in is not None and self.cut_out is not None:
            if self.cut_out <= self.cut_in:
                raise ShortValidationError(
                    f"Human review cut_out ({self.cut_out}) must be greater than cut_in ({self.cut_in})"
                )

    @property
    def human_cut_duration(self) -> Optional[float]:
        """Duration computed from human-adjusted cut points, or None if not set."""
        if self.cut_in is not None and self.cut_out is not None:
            return round(self.cut_out - self.cut_in, 3)
        return None

    def to_dict(self) -> dict[str, Any]:
        return {
            "status": self.status,
            "selection": self.selection,
            "cut_in": self.cut_in,
            "cut_out": self.cut_out,
            "notes": self.notes,
            "reviewed_at": self.reviewed_at,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> HumanReview:
        return cls(
            status=data.get("status", "unreviewed"),
            selection=data.get("selection"),
            cut_in=float(data["cut_in"]) if data.get("cut_in") is not None else None,
            cut_out=float(data["cut_out"]) if data.get("cut_out") is not None else None,
            notes=str(data.get("notes", "")),
            reviewed_at=data.get("reviewed_at"),
        )


def _generate_excluded_range_id(existing_ids: set[str]) -> str:
    """Generate a short, stable, human-readable ID for a new excluded range.

    Sequential 'cut_NNN' form for readability in logs/UI, with a random
    fallback suffix on the rare chance of a collision (e.g. after ranges
    were removed and re-added out of order).
    """
    n = len(existing_ids) + 1
    candidate_id = f"cut_{n:03d}"
    while candidate_id in existing_ids:
        candidate_id = f"cut_{n:03d}_{uuid.uuid4().hex[:4]}"
        n += 1
    return candidate_id


@dataclass
class ExcludedRange:
    """A single interior exclusion inside a candidate's effective cut range.

    Coordinates are float seconds in the same domain as
    effective_cut_start/effective_cut_end (i.e. relative to the original
    source video, not to the candidate's own local timeline).

    `id` is a stable, UI-facing identifier used by candidate-cut remove so
    the CLI/UI never has to match on float start/end values (which can
    drift by a rounding hair, e.g. 12.300000 vs 12.299999).
    """

    id: str
    start: float
    end: float

    def __post_init__(self) -> None:
        self.id = str(self.id).strip()
        if not self.id:
            raise ShortValidationError("ExcludedRange id cannot be empty.")
        self.start = round(float(self.start), 3)
        self.end = round(float(self.end), 3)
        if self.end <= self.start:
            raise ShortValidationError(
                f"ExcludedRange '{self.id}': end ({self.end}) must be greater than start ({self.start})"
            )

    @property
    def duration(self) -> float:
        return round(self.end - self.start, 3)

    def to_dict(self) -> dict[str, Any]:
        return {"id": self.id, "start": self.start, "end": self.end}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "ExcludedRange":
        return cls(
            id=str(data.get("id") or _generate_excluded_range_id(set())),
            start=float(data["start"]),
            end=float(data["end"]),
        )


def normalize_excluded_ranges(
    ranges: list["ExcludedRange"],
    effective_start: float,
    effective_end: float,
) -> list["ExcludedRange"]:
    """Validate, sort, and merge overlapping/touching excluded ranges.

    Authoritative single point of normalization (spec FASE 5.4 sez. 8):
    - every range must be entirely inside [effective_start, effective_end]
      (a range outside the candidate's effective boundaries is an editorial
      error and raises ShortValidationError -- it is never silently clamped)
    - ranges are sorted ascending by start
    - overlapping or touching ranges (next.start <= current.end) are merged
      into one, keeping the earlier range's id
    - the returned list is a new list; the input is never mutated in place
    """
    if not ranges:
        return []

    for r in ranges:
        if r.start < effective_start - 1e-6 or r.end > effective_end + 1e-6:
            raise ShortValidationError(
                f"Excluded range '{r.id}' ({r.start}s -> {r.end}s) is outside the "
                f"candidate's effective boundaries ({effective_start}s -> {effective_end}s)"
            )

    ordered = sorted(ranges, key=lambda r: (r.start, r.end))

    merged: list[ExcludedRange] = []
    for r in ordered:
        if merged and r.start <= merged[-1].end + 1e-6:
            prev = merged[-1]
            if r.end > prev.end:
                merged[-1] = ExcludedRange(id=prev.id, start=prev.start, end=r.end)
            # else: fully contained in prev, drop it (already covered)
        else:
            merged.append(ExcludedRange(id=r.id, start=r.start, end=r.end))

    return merged


def resolve_keep_segments(
    lo: float,
    hi: float,
    exclusions: list[ExcludedRange],
    entity_label: str = "Range",
) -> list[tuple[float, float]]:
    """Derive ordered list of KEEP (start, end) segments inside [lo, hi]
    after subtracting exclusions.

    This is the single authoritative pure-math algorithm for KEEP segment
    resolution across candidates, version clips, and compilation timelines.
    Never duplicated in Swift.

    - Every excluded range must fall within [lo - 1e-6, hi + 1e-6], otherwise
      raises ShortValidationError.
    - Exclusions are normalized (sorted and overlapping/touching ranges merged).
    - If exclusions is empty, returns [(lo, hi)].
    - Returns non-overlapping, strictly positive duration segments: [(s1, e1), (s2, e2), ...].
    """
    lo = round(float(lo), 3)
    hi = round(float(hi), 3)
    if hi <= lo:
        raise ShortValidationError(
            f"{entity_label}: end ({hi}s) must be greater than start ({lo}s)"
        )
    if not exclusions:
        return [(lo, hi)]

    for r in exclusions:
        if r.start < lo - 1e-6 or r.end > hi + 1e-6:
            raise ShortValidationError(
                f"{entity_label}: excluded range '{r.id}' "
                f"({r.start}s -> {r.end}s) falls outside the current "
                f"effective boundary ({lo}s -> {hi}s). The active cut "
                "changed and this exclusion is no longer valid inside "
                "it -- remove or adjust the exclusion before exporting "
                "(it is never silently ignored or clamped)."
            )

    normalized = normalize_excluded_ranges(exclusions, lo, hi)

    segments: list[tuple[float, float]] = []
    cursor = lo
    for r in normalized:
        if r.start > cursor:
            segments.append((cursor, r.start))
        cursor = max(cursor, r.end)
    if cursor < hi:
        segments.append((cursor, hi))
    return segments


@dataclass
class ShortCandidate:
    """Represents an editorially selected candidate for a vertical Short clip.

    Temporal Model Contract:
    - semantic_start / semantic_end: the exact bounds of the spoken idea/thought.
    - semantic_duration: (semantic_end - semantic_start).
    - suggested_cut_start / suggested_cut_end: actual editing/cutting bounds with
      natural head/tail pause headroom without cutting words.
    - suggested_cut_duration: (suggested_cut_end - suggested_cut_start).
    - start / end / duration: aliases to suggested cut bounds for backward compatibility.
    - human_review: structured editorial decision (unreviewed, approved, rejected, needs_revision)
      with optional custom cut_in/cut_out.
    - approved: legacy boolean strictly tied to human_review.status == 'approved'.
    """

    id: str
    title: str = ""
    hook: str = ""
    summary: str = ""
    reason: str = ""
    category: str = "general"
    score: Optional[ShortScore] = None
    origin: str = "unknown"
    semantic_start: float = 0.0
    semantic_end: float = 0.0
    suggested_cut_start: Optional[float] = None
    suggested_cut_end: Optional[float] = None
    start: Optional[float] = None
    end: Optional[float] = None
    duration: Optional[float] = None
    requires_visual_context: bool = False
    visual_note: str = ""
    approved: bool = False
    human_review: HumanReview = field(default_factory=lambda: HumanReview())
    notes: str = ""
    # Editorial selector fields (Phase 4)
    editorial_role: Optional[str] = None  # single, compilation_member, both
    editorial_score: Optional[float] = None  # 0.0 -> 1.0
    editorial_reasons: list[str] = field(default_factory=list)
    editorial_flags: list[str] = field(default_factory=list)
    related_candidates: list[dict[str, str]] = field(default_factory=list)
    # FASE 5.4: interior exclusions inside the candidate's effective cut range.
    # Purely technical/editing metadata -- never changes candidate identity,
    # score, category, or human_review (see normalize_excluded_ranges).
    excluded_ranges: list["ExcludedRange"] = field(default_factory=list)
    # Version Architecture v1 / NOTAM 1: optional explicit envelope of reusable
    # conversational context. If None, fallback is raw candidate start/end.
    source_envelope: Optional[tuple[float, float]] = None

    def __post_init__(self) -> None:
        self.id = str(self.id).strip()

        # Resolve raw candidate boundaries (start / end)
        raw_start = self.start
        raw_end = self.end

        # Resolve semantic bounds from start/end if semantic_* not explicitly provided
        if self.semantic_start == 0.0 and raw_start is not None:
            self.semantic_start = float(raw_start)
        if self.semantic_end == 0.0 and raw_end is not None:
            self.semantic_end = float(raw_end)

        self.semantic_start = round(float(self.semantic_start), 3)
        self.semantic_end = round(float(self.semantic_end), 3)

        # Resolve suggested cut bounds (defaults to semantic bounds if omitted)
        if self.suggested_cut_start is None:
            self.suggested_cut_start = self.semantic_start
        else:
            self.suggested_cut_start = round(float(self.suggested_cut_start), 3)

        if self.suggested_cut_end is None:
            self.suggested_cut_end = self.semantic_end
        else:
            self.suggested_cut_end = round(float(self.suggested_cut_end), 3)

        # Raw candidate envelope bounds (candidate.start / candidate.end)
        # Bounded so that raw envelope always encompasses suggested cut bounds
        if raw_start is not None:
            self.start = min(round(float(raw_start), 3), self.suggested_cut_start)
        else:
            self.start = self.suggested_cut_start

        if raw_end is not None:
            self.end = max(round(float(raw_end), 3), self.suggested_cut_end)
        else:
            self.end = self.suggested_cut_end

        if self.duration is None:
            self.duration = round(self.end - self.start, 3)
        else:
            self.duration = round(float(self.duration), 3)

        self.requires_visual_context = bool(self.requires_visual_context)

        # Ensure human_review instance
        if isinstance(self.human_review, dict):
            self.human_review = HumanReview.from_dict(self.human_review)
        elif self.human_review is None:
            self.human_review = HumanReview()

        # Backward compatibility sync: if approved was passed as True and review was unreviewed
        if self.approved and self.human_review.status == "unreviewed":
            self.human_review.status = "approved"

        # Canonical relationship:
        # human_review.status == 'approved' -> approved = True
        # all other statuses -> approved = False
        self.approved = (self.human_review.status == "approved")

        # Editorial selector fields validation
        if self.editorial_role is not None:
            self.editorial_role = str(self.editorial_role).strip().lower()
            if self.editorial_role not in EDITORIAL_ROLES:
                raise ShortValidationError(
                    f"Invalid editorial_role '{self.editorial_role}'. Allowed: {sorted(EDITORIAL_ROLES)}"
                )

        if self.editorial_score is not None:
            self.editorial_score = round(float(self.editorial_score), 3)
            if not (0.0 <= self.editorial_score <= 1.0):
                raise ShortValidationError(
                    f"editorial_score must be between 0.0 and 1.0, got {self.editorial_score}"
                )

        self.editorial_reasons = list(self.editorial_reasons or [])
        self.editorial_flags = list(self.editorial_flags or [])
        self.related_candidates = list(self.related_candidates or [])
        for rel in self.related_candidates:
            if not isinstance(rel, dict) or "candidate_id" not in rel or "relation" not in rel:
                raise ShortValidationError(
                    f"Invalid related_candidates entry: {rel}. Must contain 'candidate_id' and 'relation'."
                )
            if rel["relation"] not in CANDIDATE_RELATIONS:
                raise ShortValidationError(
                    f"Invalid candidate relation '{rel['relation']}'. Allowed: {sorted(CANDIDATE_RELATIONS)}"
                )

        # Normalize excluded_ranges (dict -> ExcludedRange, sort, merge overlaps,
        # bound-check against this candidate's effective cut range). Raises
        # ShortValidationError on an out-of-bounds range rather than clamping
        # silently (spec FASE 5.4 sez. 8).
        normalized_input: list[ExcludedRange] = []
        seen_ids: set[str] = set()
        for r in (self.excluded_ranges or []):
            if isinstance(r, ExcludedRange):
                obj = r
            else:
                d = dict(r)
                if not d.get("id"):
                    d["id"] = _generate_excluded_range_id(seen_ids)
                obj = ExcludedRange.from_dict(d)
            seen_ids.add(obj.id)
            normalized_input.append(obj)
        self.excluded_ranges = normalize_excluded_ranges(
            normalized_input, self.effective_cut_start, self.effective_cut_end
        )

        # Version Architecture v1 / NOTAM 1 / Step 1.1: validate source_envelope if provided
        # Rule: candidate.start <= envelope.start < envelope.end <= candidate.end
        if self.source_envelope is not None:
            env_start = round(float(self.source_envelope[0]), 3)
            env_end = round(float(self.source_envelope[1]), 3)
            if env_end <= env_start:
                raise ShortValidationError(
                    f"Candidate '{self.id}': source_envelope end ({env_end}s) "
                    f"must be greater than start ({env_start}s)"
                )
            if env_start < self.start - 1e-6 or env_end > self.end + 1e-6:
                raise ShortValidationError(
                    f"Candidate '{self.id}': source_envelope ({env_start}s -> {env_end}s) "
                    f"must fall within candidate raw bounds ({self.start}s -> {self.end}s)"
                )
            self.source_envelope = (env_start, env_end)

    @property
    def semantic_duration(self) -> float:
        """Duration of the core spoken concept/sentence (semantic_end - semantic_start)."""
        return round(self.semantic_end - self.semantic_start, 3)

    @property
    def suggested_cut_duration(self) -> float:
        """Actual duration of the exported clip (suggested_cut_end - suggested_cut_start)."""
        return round(self.suggested_cut_end - self.suggested_cut_start, 3)

    @property
    def effective_cut_start(self) -> float:
        """Primary start time for editing/cutting."""
        return self.suggested_cut_start

    @property
    def effective_cut_end(self) -> float:
        """Primary end time for editing/cutting."""
        return self.suggested_cut_end

    @property
    def effective_duration(self) -> float:
        """Primary duration for editing/cutting."""
        return self.suggested_cut_duration

    @property
    def effective_cut_duration(self) -> float:
        """Alias for effective_duration."""
        return self.suggested_cut_duration

    @property
    def effective_source_envelope(self) -> tuple[float, float]:
        """Source envelope bounds: explicit source_envelope if present,
        otherwise candidate raw bounds (self.start, self.end).
        (NOTAM 1 / Step 1.1: NEVER falls back automatically to full media bounds).
        """
        if self.source_envelope is not None:
            return self.source_envelope
        return (self.start, self.end)

    @property
    def excluded_duration(self) -> float:
        """Total duration removed by interior exclusions (FASE 5.4).

        0.0 when excluded_ranges is empty. Always accurate to 3 decimal places.
        """
        return round(sum(r.duration for r in self.excluded_ranges), 3)

    @property
    def final_duration(self) -> float:
        """Duration of the candidate after interior exclusions are removed.

        Deliberately NOT named `effective_duration` to avoid silently changing
        the meaning of that already-established property (effective_cut_end -
        effective_cut_start, i.e. the outer IN/OUT span used throughout the
        rest of the codebase -- compilation timeline math, duration
        validation, etc). `final_duration` is the new, narrower concept:
        "how long is this candidate once the interior cuts are applied."
        """
        return round(self.effective_cut_duration - self.excluded_duration, 3)

    def derive_keep_segments(
        self,
        boundary: Optional[tuple[float, float]] = None,
    ) -> list[tuple[float, float]]:
        """Derive the ordered list of KEEP (start, end) segments inside the
        candidate's effective cut range, after removing excluded_ranges.

        Authoritative single source of truth (spec FASE 5.4 sez. 2) -- delegates
        to pure resolve_keep_segments without logic duplication.
        """
        lo, hi = boundary if boundary is not None else (self.effective_cut_start, self.effective_cut_end)
        return resolve_keep_segments(
            lo=lo,
            hi=hi,
            exclusions=self.excluded_ranges,
            entity_label=f"Candidate '{self.id}'",
        )

    @property
    def human_cut_in(self) -> Optional[float]:
        """Human review cut_in, if defined."""
        return self.human_review.cut_in

    @property
    def human_cut_out(self) -> Optional[float]:
        """Human review cut_out, if defined."""
        return self.human_review.cut_out

    @property
    def human_cut_duration(self) -> Optional[float]:
        """Duration of human review cut points, if both cut_in and cut_out are defined."""
        return self.human_review.human_cut_duration

    def set_human_review(
        self,
        status: str,
        cut_in: Optional[float] = None,
        cut_out: Optional[float] = None,
        notes: str = "",
        selection: Optional[Any] = None,
        reviewed_at: Optional[str] = None,
    ) -> None:
        """Apply human editorial review decision and synchronize the legacy approved boolean.

        FASE 5.4: when this call actually changes cut_in/cut_out (i.e. the
        caller passed one of them), the new human-cut boundary is checked
        against any existing excluded_ranges BEFORE it is committed. If the
        new boundary would leave an exclusion partially or fully outside it,
        this raises ShortValidationError and leaves human_review untouched --
        the conflict must be resolved (adjust/remove the exclusion, or the
        cut) rather than silently ignoring the exclusions or clamping the
        new cut. A status-only update (cut_in/cut_out both left as None,
        i.e. "keep the existing boundary") never re-triggers this check.
        """
        from datetime import datetime
        if reviewed_at is None:
            reviewed_at = datetime.now().astimezone().isoformat(timespec="seconds")

        new_cut_in = cut_in if cut_in is not None else self.human_review.cut_in
        new_cut_out = cut_out if cut_out is not None else self.human_review.cut_out

        if (cut_in is not None or cut_out is not None) and self.excluded_ranges:
            if new_cut_in is not None and new_cut_out is not None:
                for r in self.excluded_ranges:
                    if r.start < new_cut_in - 1e-6 or r.end > new_cut_out + 1e-6:
                        raise ShortValidationError(
                            f"Candidate '{self.id}': cannot set human cut "
                            f"({new_cut_in}s -> {new_cut_out}s) because excluded "
                            f"range '{r.id}' ({r.start}s -> {r.end}s) would fall "
                            "outside it. Remove or adjust that exclusion first "
                            "(candidate-cut remove), then set the human cut -- "
                            "exclusions are never silently dropped or clamped."
                        )

        self.human_review = HumanReview(
            status=status,
            selection=selection if selection is not None else self.human_review.selection,
            cut_in=new_cut_in,
            cut_out=new_cut_out,
            notes=notes if notes else self.human_review.notes,
            reviewed_at=reviewed_at,
        )
        self.approved = (self.human_review.status == "approved")

    def get_cut_boundaries(
        self,
        use_semantic: bool = False,
        use_human: bool = False,
    ) -> tuple[float, float, float]:
        """Return (start, end, duration) depending on whether semantic bounds, human cut bounds, or suggested cut bounds are requested."""
        if (
            use_human
            and self.human_review.cut_in is not None
            and self.human_review.cut_out is not None
            and self.human_cut_duration is not None
        ):
            return self.human_review.cut_in, self.human_review.cut_out, self.human_cut_duration
        if use_semantic:
            return self.semantic_start, self.semantic_end, self.semantic_duration
        return self.effective_cut_start, self.effective_cut_end, self.effective_cut_duration

    def validate(
        self,
        max_duration: float = 180.0,
        min_duration: float = 3.0,
        video_duration: float | None = None,
    ) -> None:
        """Validate candidate timings, boundaries and constraints."""
        if not self.id:
            raise ShortValidationError("Candidate ID cannot be empty.")

        # 1. Semantic boundary validations
        if self.semantic_start < 0:
            raise ShortValidationError(f"Semantic start time cannot be negative: {self.semantic_start}")
        if self.semantic_end <= self.semantic_start:
            raise ShortValidationError(
                f"Semantic end ({self.semantic_end}) must be greater than semantic start ({self.semantic_start})"
            )
        if self.semantic_duration > max_duration:
            raise ShortValidationError(
                f"Semantic duration {self.semantic_duration}s exceeds maximum allowed {max_duration}s"
            )

        # 2. Suggested cut boundary validations
        if self.suggested_cut_start < 0:
            raise ShortValidationError(f"suggested_cut_start cannot be negative: {self.suggested_cut_start}")
        if self.suggested_cut_end <= self.suggested_cut_start:
            raise ShortValidationError(
                f"suggested_cut_end ({self.suggested_cut_end}) must be greater than suggested_cut_start ({self.suggested_cut_start})"
            )
        if self.suggested_cut_duration < min_duration:
            raise ShortValidationError(
                f"Suggested cut duration {self.suggested_cut_duration}s is below minimum allowed {min_duration}s"
            )
        if self.suggested_cut_duration > max_duration:
            raise ShortValidationError(
                f"Suggested cut duration {self.suggested_cut_duration}s exceeds maximum allowed {max_duration}s"
            )

        # 3. Video boundary constraint
        if video_duration is not None:
            max_bound = max(self.semantic_end, self.suggested_cut_end)
            if max_bound > video_duration + 0.1:
                raise ShortValidationError(
                    f"Candidate boundary {max_bound}s exceeds video total duration {video_duration}s"
                )

        if self.score is not None:
            self.score.validate()

    def overlap_ratio(self, other: ShortCandidate) -> float:
        """Calculate intersection over union (IoU) of suggested cut ranges with another candidate."""
        inter_start = max(self.suggested_cut_start, other.suggested_cut_start)
        inter_end = min(self.suggested_cut_end, other.suggested_cut_end)
        inter_len = max(0.0, inter_end - inter_start)
        if inter_len == 0.0:
            return 0.0
        union_start = min(self.suggested_cut_start, other.suggested_cut_start)
        union_end = max(self.suggested_cut_end, other.suggested_cut_end)
        union_len = union_end - union_start
        return round(inter_len / union_len, 3) if union_len > 0 else 0.0

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "title": self.title,
            "hook": self.hook,
            "summary": self.summary,
            "reason": self.reason,
            "category": self.category,
            "score": self.score.to_dict() if self.score is not None else None,
            "origin": self.origin,
            # Explicit temporal model
            "semantic_start": round(self.semantic_start, 3),
            "semantic_end": round(self.semantic_end, 3),
            "semantic_duration": round(self.semantic_duration, 3),
            "suggested_cut_start": round(self.suggested_cut_start, 3),
            "suggested_cut_end": round(self.suggested_cut_end, 3),
            "suggested_cut_duration": round(self.suggested_cut_duration, 3),
            # Backward compatibility fields (raw candidate bounds)
            "start": round(self.start, 3),
            "end": round(self.end, 3),
            "duration": round(self.duration, 3),
            "requires_visual_context": self.requires_visual_context,
            "visual_note": self.visual_note,
            "approved": self.approved,
            "human_review": self.human_review.to_dict(),
            "notes": self.notes,
            # Phase 4 Editorial Selector fields
            "editorial_role": self.editorial_role,
            "editorial_score": self.editorial_score,
            "editorial_reasons": self.editorial_reasons,
            "editorial_flags": self.editorial_flags,
            "related_candidates": self.related_candidates,
            # FASE 5.4: interior exclusions + derived durations (always present,
            # backward-compatible default []/full-duration when there are none).
            "excluded_ranges": [r.to_dict() for r in self.excluded_ranges],
            "excluded_duration": self.excluded_duration,
            "final_duration": self.final_duration,
            # Version Architecture v1: explicit reusable envelope bounds (or None)
            "source_envelope": list(self.source_envelope) if self.source_envelope is not None else None,
        }

    def to_json(self, indent: int = 2) -> str:
        return json.dumps(self.to_dict(), ensure_ascii=False, indent=indent)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ShortCandidate:
        # Resolve semantic boundaries
        sem_start = float(data.get("semantic_start", data.get("start", 0.0)))
        sem_end = float(data.get("semantic_end", data.get("end", 0.0)))

        # Resolve suggested cut boundaries
        cut_start = float(data["suggested_cut_start"]) if data.get("suggested_cut_start") is not None else sem_start
        cut_end = float(data["suggested_cut_end"]) if data.get("suggested_cut_end") is not None else sem_end

        # Score parsing
        raw_score = data.get("score")
        origin = str(data.get("origin") or "legacy_unverified")
        score = ShortScore.from_dict(raw_score) if isinstance(raw_score, dict) else None
        if "origin" not in data and score is not None:
            signature = (score.standalone, score.hook, score.value, score.closure, score.overall)
            if signature in {(0.90, 0.92, 0.90, 0.88, 0.90),
                             (0.90, 0.90, 0.90, 0.88, 0.90)}:
                # Exact constants of the removed transcript chunker (6ee4e13,
                # src/selection/candidate.py): provenance is demonstrable.
                score = None
                origin = "legacy_baseline"
            elif signature == (0.8, 0.8, 0.8, 0.8, 0.8):
                # Former model default for a missing score: a filler value
                # whose real origin cannot be known.
                score = None
        if "confidence" in data and raw_score is None:
            origin = "legacy_confidence_unverified"

        # Human review parsing
        raw_hr = data.get("human_review")
        if isinstance(raw_hr, dict):
            hr = HumanReview.from_dict(raw_hr)
        else:
            init_status = "approved" if data.get("approved") is True else "unreviewed"
            hr = HumanReview(status=init_status)

        # Editorial fields parsing
        editorial_role = data.get("editorial_role")
        editorial_score = float(data["editorial_score"]) if data.get("editorial_score") is not None else None
        editorial_reasons = list(data.get("editorial_reasons", []))
        editorial_flags = list(data.get("editorial_flags", []))
        related_candidates = list(data.get("related_candidates", []))
        excluded_ranges_raw = data.get("excluded_ranges", [])
        excluded_ranges: list[ExcludedRange] = []
        _seen_range_ids: set[str] = set()
        for r in excluded_ranges_raw:
            if isinstance(r, ExcludedRange):
                obj = r
            else:
                rd = dict(r)
                if not rd.get("id"):
                    rd["id"] = _generate_excluded_range_id(_seen_range_ids)
                obj = ExcludedRange.from_dict(rd)
            _seen_range_ids.add(obj.id)
            excluded_ranges.append(obj)

        source_envelope = None
        if data.get("source_envelope") is not None:
            se = data["source_envelope"]
            source_envelope = (float(se[0]), float(se[1]))

        # Resolve raw candidate boundaries (start / end)
        raw_start = float(data["start"]) if data.get("start") is not None else cut_start
        raw_end = float(data["end"]) if data.get("end") is not None else cut_end

        return cls(
            id=str(data.get("id", "short_unnamed")),
            title=str(data.get("title", "")).strip(),
            hook=str(data.get("hook", "")).strip(),
            summary=str(data.get("summary", "")).strip(),
            reason=str(data.get("reason", "")).strip(),
            category=str(data.get("category", "general")).strip(),
            score=score,
            origin=origin,
            semantic_start=sem_start,
            semantic_end=sem_end,
            suggested_cut_start=cut_start,
            suggested_cut_end=cut_end,
            start=raw_start,
            end=raw_end,
            duration=round(raw_end - raw_start, 3),
            requires_visual_context=bool(data.get("requires_visual_context", False)),
            visual_note=str(data.get("visual_note", "")).strip(),
            approved=bool(data.get("approved", False)),
            human_review=hr,
            notes=str(data.get("notes", "")).strip(),
            editorial_role=editorial_role,
            editorial_score=editorial_score,
            editorial_reasons=editorial_reasons,
            editorial_flags=editorial_flags,
            related_candidates=related_candidates,
            excluded_ranges=excluded_ranges,
            source_envelope=source_envelope,
        )
