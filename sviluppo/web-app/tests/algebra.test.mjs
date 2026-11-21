// Pure algebra tests (node --test). Run: npm run test:algebra
import test from 'node:test';
import assert from 'node:assert/strict';
import { rippleTrim, roll, slip, slide, trackClips } from '../src/lib/editAlgebra.ts';

// Three adjacent clips on track V: A[0..5s] B[5..10s] C[10..15s]
const three = () => [
  { id: 'A', track_id: 'V', start_ms: 0, trim_in_ms: 1000, duration_ms: 5000, asset_id: 'm1' },
  { id: 'B', track_id: 'V', start_ms: 5000, trim_in_ms: 1000, duration_ms: 5000, asset_id: 'm1' },
  { id: 'C', track_id: 'V', start_ms: 10000, trim_in_ms: 1000, duration_ms: 5000, asset_id: 'm1' },
];
const media = { m1: 20000 };

const clip = (clips, id) => trackClips(clips, 'V').find((c) => c.id === id);

test('ripple In consumes the head and later clips pack left', () => {
  const r = rippleTrim(three(), 'V', 'B', 'start', 2000, media);
  assert.equal(r.applied, 2000);
  const b = clip(r.clips, 'B');
  const c = clip(r.clips, 'C');
  assert.equal(b.start_ms, 5000);
  assert.equal(b.trim_in_ms, 3000);
  assert.equal(b.duration_ms, 3000);
  assert.equal(c.start_ms, 8000); // following content moved left by d
});

test('ripple In extending the head is clamped at the source start', () => {
  const r = rippleTrim(three(), 'V', 'B', 'start', -5000, media);
  assert.equal(r.applied, -1000); // B had exactly 1000 ms of head room
  const b = clip(r.clips, 'B');
  assert.equal(b.trim_in_ms, 0);
  assert.equal(b.duration_ms, 6000);
  assert.equal(clip(r.clips, 'C').start_ms, 11000);
});

test('ripple Out grows and shifts following right; media tail clamps', () => {
  const r = rippleTrim(three(), 'V', 'B', 'end', 1500, media);
  assert.equal(r.applied, 1500);
  assert.equal(clip(r.clips, 'B').duration_ms, 6500);
  assert.equal(clip(r.clips, 'C').start_ms, 11500);
  const r2 = rippleTrim(three(), 'V', 'B', 'end', 30000, media);
  assert.equal(r2.applied, 14000); // 20000 - 1000 trim - 5000 dur
});

test('roll moves the shared cut keeping total duration and content continuity', () => {
  const r = roll(three(), 'V', 'B', 'C', 1200, media);
  assert.equal(r.applied, 1200);
  const b = clip(r.clips, 'B');
  const c = clip(r.clips, 'C');
  assert.equal(b.duration_ms, 6200);
  assert.equal(c.start_ms, 11200); // the cut moved right
  assert.equal(c.trim_in_ms, 2200); // C gave up head content (trim advanced)
  assert.equal(c.duration_ms, 3800);
  assert.equal(c.start_ms + c.duration_ms, 15000); // track end unchanged
  assert.equal(clip(r.clips, 'A').start_ms, 0);
});

test('roll refuses non-adjacent clips and clamps at the right head', () => {
  const r = roll(three(), 'V', 'A', 'C', 1000, media);
  assert.equal(r.applied, 0);
  assert.match(r.reason ?? '', /non sono adiacenti/);
  const r2 = roll(three(), 'V', 'B', 'C', -2000, media);
  assert.equal(r2.applied, -1000); // only 1000 ms of head available on C
  const c = clip(r2.clips, 'C');
  assert.equal(c.trim_in_ms, 0);
  assert.equal(c.duration_ms, 6000);
  assert.equal(c.start_ms, 9000);
});

test('slip moves the source window, timeline untouched, clamped by media', () => {
  const r = slip(three(), 'B', 3000, media);
  assert.equal(r.applied, 3000);
  const b = clip(r.clips, 'B');
  assert.equal(b.start_ms, 5000);
  assert.equal(b.duration_ms, 5000);
  assert.equal(b.trim_in_ms, 4000);
  const r2 = slip(three(), 'B', 30000, media);
  assert.equal(r2.applied, 14000);
  const r3 = slip(three(), 'B', -5000, media);
  assert.equal(r3.applied, -1000); // the head cannot pass zero
});

test('slide moves the clip and neighbours absorb it; gaps close first', () => {
  const r = slide(three(), 'V', 'B', 1500, media);
  assert.equal(r.applied, 1500);
  assert.equal(clip(r.clips, 'A').duration_ms, 6500); // A grows into its own tail
  const b = clip(r.clips, 'B');
  assert.equal(b.start_ms, 6500);
  assert.equal(b.duration_ms, 5000);
  const c = clip(r.clips, 'C');
  assert.equal(c.start_ms, 11500);
  assert.equal(c.duration_ms, 3500);
  assert.equal(c.start_ms + c.duration_ms, 15000);
  // gap-only case: C far away, B moves right without touching anything
  const gapped = three().map((c2) => (c2.id === 'C' ? { ...c2, start_ms: 20000 } : c2));
  const r2 = slide(gapped, 'V', 'B', 4000, media);
  assert.equal(r2.applied, 4000);
  assert.equal(clip(r2.clips, 'C').start_ms, 20000); // untouched
  assert.equal(clip(r2.clips, 'A').duration_ms, 5000);
});

test('unknown media never blocks the timeline side by invented limits', () => {
  const r = rippleTrim(three(), 'V', 'B', 'end', 9000, {});
  assert.equal(r.applied, 9000);
});
