// Pure editing algebra for the timeline: ripple, roll, slip and slide.
// The semantics follow the public editing definitions and were studied in the
// headless `edit` algebra layer of FilmCraft (storytold/filmcraft, dual
// MIT OR Apache-2.0): adjacency checks, minimum-duration clamps, media-head
// limits and "the cut moves, the total does not". This file is an independent
// implementation over the suite's integer-millisecond clip model.
//
// Clip: start = timeline position ms, trim = source-in ms, dur = duration ms.
// Media lengths are per asset and optional: unknown media means "no clamp on
// the source side, only on the timeline side" (never invented head room).

export interface AClip {
  id: string;
  track_id: string;
  start_ms: number;
  trim_in_ms: number;
  duration_ms: number;
  asset_id: string;
}

export type MediaMap = Record<string, number | undefined>;

const MIN_MS = 1;

export function trackClips(clips: AClip[], trackId: string): AClip[] {
  return clips
    .filter((c) => c.track_id === trackId)
    .sort((a, b) => a.start_ms - b.start_ms)
    .map((c) => ({ ...c }));
}

function merge(clips: AClip[], trackId: string, updated: AClip[]): AClip[] {
  return [...clips.filter((c) => c.track_id !== trackId), ...updated];
}

function mediaRemaining(media: MediaMap, clip: AClip): number {
  const total = media[clip.asset_id];
  if (total === undefined) return Number.POSITIVE_INFINITY;
  return Math.max(0, total - clip.trim_in_ms - clip.duration_ms);
}

export interface RippleResult {
  clips: AClip[];
  applied: number; // actual delta in ms after clamping
  reason?: string;
}

/// Ripple trim of one edge on one track, semantics verified against the
/// public edit-algebra pattern (FilmCraft ripple_trim_group/move_edge):
/// - In by +d consumes the clip head: start stays, duration shrinks,
///   source-in advances, and every clip at or after the cut slides left.
/// - Out by +d grows the clip (limited by its media tail) and every later
///   clip slides right. Linked/sync-locked tracks do not exist in the suite
///   model, so no partner propagation happens here.
export function rippleTrim(
  clips: AClip[],
  trackId: string,
  clipId: string,
  edge: 'start' | 'end',
  delta: number,
  media: MediaMap,
): RippleResult {
  const track = trackClips(clips, trackId);
  const index = track.findIndex((c) => c.id === clipId);
  if (index < 0) return { clips, applied: 0, reason: 'clip estranea alla traccia' };
  const clip = track[index];
  if (edge === 'start') {
    let d = delta;
    d = Math.max(d, MIN_MS - clip.duration_ms);
    d = Math.max(d, -clip.trim_in_ms); // extending the head never passes the source start
    if (d === 0) return { clips, applied: 0, reason: 'nessun margine per il ripple' };
    clip.trim_in_ms += d;
    clip.duration_ms -= d;
    // d>0 (head consumed): later clips shift left by d; d<0: right
    for (let i = index + 1; i < track.length; i++) track[i].start_ms -= d;
    return { clips: merge(clips, trackId, track), applied: d };
  }
  let d = delta;
  d = Math.max(d, MIN_MS - clip.duration_ms);
  d = Math.min(d, mediaRemaining(media, clip));
  if (d === 0) return { clips, applied: 0, reason: 'nessun margine per il ripple' };
  clip.duration_ms += d;
  for (let i = index + 1; i < track.length; i++) track[i].start_ms += d;
  return { clips: merge(clips, trackId, track), applied: d };
}

export interface RollResult {
  clips: AClip[];
  applied: number;
  reason?: string;
}

/// Slip: the clip keeps timeline position and duration; its source window
/// slides (clamped to the media, when the media length is known).
export function slip(
  clips: AClip[],
  clipId: string,
  deltaMedia: number,
  media: MediaMap,
): RollResult {
  const target = clips.find((c) => c.id === clipId);
  if (!target) return { clips, applied: 0, reason: 'clip estranea' };
  const copy = { ...target };
  let d = deltaMedia;
  d = Math.max(d, -copy.trim_in_ms);
  const total = media[copy.asset_id];
  if (total !== undefined) d = Math.min(d, total - copy.trim_in_ms - copy.duration_ms);
  if (d === 0) return { clips, applied: 0, reason: 'il contenuto sorgente è già all’estremo' };
  copy.trim_in_ms += d;
  return { clips: clips.map((c) => (c.id === clipId ? copy : c)), applied: d };
}

/// Roll moves the cut between two adjacent clips: total duration unchanged,
/// each side limited by its own media (left: remaining forward; right:
/// rewinding cannot pass its source head).
export function roll(
  clips: AClip[]
, trackId: string,
  leftId: string,
  rightId: string,
  delta: number,
  media: MediaMap,
): RollResult {
  const track = trackClips(clips, trackId);
  const li = track.findIndex((c) => c.id === leftId);
  const ri = track.findIndex((c) => c.id === rightId);
  if (li < 0 || ri < 0) return { clips, applied: 0, reason: 'clip estranee alla traccia' };
  const left = track[li];
  const right = track[ri];
  if (left.start_ms + left.duration_ms !== right.start_ms) {
    return { clips, applied: 0, reason: 'i due clip non sono adiacenti: il roll richiede un taglio condiviso' };
  }
  let d = delta;
  d = Math.max(d, MIN_MS - left.duration_ms);
  d = Math.min(d, right.duration_ms - MIN_MS);
  d = Math.max(d, -right.trim_in_ms); // regaining head never passes the right source start
  d = Math.min(d, mediaRemaining(media, left)); // the left clip grows into its own media tail
  if (d === 0) return { clips, applied: 0, reason: 'nessun margine per il roll' };
  left.duration_ms += d;
  right.start_ms += d;
  right.trim_in_ms += d;
  right.duration_ms -= d;
  return { clips: merge(clips, trackId, track), applied: d };
}

/// Slide moves a clip along its track: duration and source window fixed.
/// Open gaps are closed first (nothing else changes); beyond the gap the
/// neighbours absorb the movement: the previous clip grows into its own
/// media tail, the next one loses duration at its tail (end pinned).
export function slide(
  clips: AClip[],
  trackId: string,
  clipId: string,
  delta: number,
  media: MediaMap,
): RollResult {
  const track = trackClips(clips, trackId);
  const index = track.findIndex((c) => c.id === clipId);
  if (index < 0) return { clips, applied: 0, reason: 'clip estranea alla traccia' };
  const clip = track[index];
  const prev = track[index - 1];
  const next = track[index + 1];
  let d = delta;
  const gapRight = next ? next.start_ms - (clip.start_ms + clip.duration_ms) : Number.POSITIVE_INFINITY;
  const gapLeft = prev ? clip.start_ms - (prev.start_ms + prev.duration_ms) : clip.start_ms;
  if (d > 0) {
    const closed = Math.min(d, gapRight);
    clip.start_ms += closed;
    d -= closed;
    if (d > 0) {
      let rem = d;
      if (!next) return { clips, applied: closed, reason: 'lo slide oltre l’ultimo clip non ha limite: fermato al gap' };
      rem = Math.min(rem, next.duration_ms - MIN_MS);
      if (prev) rem = Math.min(rem, mediaRemaining(media, prev));
      rem = Math.max(0, rem);
      if (prev) prev.duration_ms += rem;
      clip.start_ms += rem;
      next.start_ms += rem;
      next.duration_ms -= rem;
      d = closed + rem;
    } else {
      d = closed;
    }
  } else if (d < 0) {
    const closed = Math.max(d, -gapLeft); // negative, up to the gap
    clip.start_ms += closed;
    let rem = d - closed; // remaining negative deformation
    if (rem < 0) {
      if (!prev) return { clips, applied: closed, reason: 'lo slide oltre il primo clip non è limitato a sinistra' };
      rem = Math.max(rem, MIN_MS - prev.duration_ms);
      // next regains duration at its tail (end pinned): needs its own media tail
      const tailNext = next ? mediaRemaining(media, next) : Number.POSITIVE_INFINITY;
      rem = Math.max(rem, Number.isFinite(tailNext) ? -tailNext : rem);
      prev.duration_ms += rem;
      clip.start_ms += rem;
      if (next) {
        next.start_ms += rem;
        next.duration_ms -= rem; // rem<0 → duration grows
      }
    }
    d = closed + rem;
  }
  if (d === 0) return { clips, applied: 0, reason: 'nessuno spazio per lo slide' };
  return { clips: merge(clips, trackId, track), applied: d };
}
