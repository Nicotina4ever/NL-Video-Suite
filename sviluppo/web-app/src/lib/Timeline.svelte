<script lang="ts">
  import { get, put } from './api';

  interface Track { id: string; name: string; kind: string; visible: boolean; locked: boolean }
  interface Clip {
    id: string; asset_id: string; track_id: string; start_ms: number;
    trim_in_ms: number; duration_ms: number; label?: string | null;
  }
  interface FullProject {
    id: string; title: string; updated_at: string;
    timeline: { tracks: Track[]; clips: Clip[] };
    assets: { id: string; name: string; kind: string }[];
  }

  let {
    project,
    onstatus,
    onupdate,
    exposed,
  }: {
    project: FullProject;
    onstatus: (message: string) => void;
    onupdate: (project: FullProject) => void;
    exposed?: (api: { seek: (ms: number) => void }) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let pps = $state(60); // pixels per second
  let scrollX = $state(0);
  let playheadMs = $state(0);
  let selected = $state<string | null>(null);
  let drag: {
    clipId: string;
    mode: 'move' | 'trim-in' | 'trim-out';
    grabMs: number;
    previewMs: number;
    previewDurationMs: number;
    snapped: boolean;
  } | null = null;

  const RULER = 26;
  const LANE = 54;
  const EDGE = 7; // trim-edge tolerance in pixels, like the hit-test spec
  const SNAP = 6; // pixels

  const laneIndex = (trackId: string) => {
    const order = ['overlay', 'video', 'audio'];
    const track = project?.timeline.tracks.find((t) => t.id === trackId);
    return track ? order.indexOf(track.kind) : 1;
  };

  function timeToX(ms: number): number {
    return (ms / 1000) * pps - scrollX;
  }
  function xToTime(px: number): number {
    return ((px + scrollX) / pps) * 1000;
  }

  function snapCandidates(): number[] {
    const values = [0, playheadMs];
    for (const c of project.timeline.clips) {
      values.push(c.start_ms, c.start_ms + c.duration_ms);
    }
    return values;
  }

  function snapMs(ms: number): { value: number; snapped: boolean } {
    const tolerance = (SNAP / pps) * 1000;
    let best = { value: ms, snapped: false };
    for (const candidate of snapCandidates()) {
      const distance = Math.abs(candidate - ms);
      if (distance <= tolerance && distance < Math.abs(best.value - ms)) {
        best = { value: candidate, snapped: true };
      }
    }
    return best;
  }

  function hit(px: number, py: number): { clip: Clip; edge: 'in' | 'out' | 'body' } | null {
    if (!project) return null;
    const lane = Math.floor((py - RULER) / LANE);
    const tracks = ['overlay', 'video', 'audio'];
    if (lane < 0 || lane >= tracks.length) return null;
    const kind = tracks[lane];
    const track = project.timeline.tracks.find((t) => t.kind === kind);
    if (!track) return null;
    for (const clip of [...project.timeline.clips].reverse()) {
      if (clip.track_id !== track.id) continue;
      const x0 = timeToX(clip.start_ms);
      const x1 = timeToX(clip.start_ms + clip.duration_ms);
      if (px < x0 - EDGE || px > x1 + EDGE) continue;
      const edge = px <= x0 + EDGE ? 'in' : px >= x1 - EDGE ? 'out' : 'body';
      return { clip, edge };
    }
    return null;
  }

  function draw() {
    const c = canvas;
    if (!c || !project) return;
    const liveDrag = drag;
    const box = c.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.max(1, Math.round(box.width * dpr));
    c.height = Math.max(1, Math.round(box.height * dpr));
    const g = c.getContext('2d')!;
    g.scale(dpr, dpr);
    const css = getComputedStyle(document.documentElement);
    const ink = (name: string) => css.getPropertyValue(name).trim();
    g.fillStyle = ink('--bg-panel');
    g.fillRect(0, 0, box.width, box.height);

    // ruler with adaptive ticks
    const stepChoices = [5000, 2000, 1000, 500, 200, 100];
    const step = stepChoices.find((s) => (s / 1000) * pps > 64) ?? 100;
    g.fillStyle = ink('--bg-raised');
    g.fillRect(0, 0, box.width, RULER);
    g.strokeStyle = ink('--line');
    g.beginPath();
    g.moveTo(0, RULER + 0.5);
    g.lineTo(box.width, RULER + 0.5);
    g.stroke();
    g.fillStyle = ink('--text-dim');
    g.font = '10px Consolas, monospace';
    const first = Math.floor(xToTime(0) / step) * step;
    for (let ms = first; ms < xToTime(box.width) + step; ms += step) {
      const x = timeToX(ms);
      if (x < -40) continue;
      g.fillRect(x, RULER - 6, 1, 6);
      if (ms % (step * 5) === 0 || step >= 1000) {
        const sec = Math.abs(ms) / 1000;
        const label = `${Math.floor(sec / 60)}:${(sec % 60).toFixed(ms % step === 0 && step < 1000 ? 1 : 0).padStart(4, '0')}`;
        g.fillText(label, x + 3, 13);
      }
    }

    // lanes
    const lanes = [
      { kind: 'overlay', label: 'OVERLAY' },
      { kind: 'video', label: 'VIDEO' },
      { kind: 'audio', label: 'AUDIO' },
    ];
    lanes.forEach((laneItem, lane) => {
      const y = RULER + lane * LANE;
      const track = project.timeline.tracks.find((t) => t.kind === laneItem.kind);
      g.strokeStyle = ink('--line');
      g.beginPath();
      g.moveTo(0, y + LANE - 0.5);
      g.lineTo(box.width, y + LANE - 0.5);
      g.stroke();
      if (track && !track.visible) {
        g.fillStyle = 'rgba(128,128,160,0.06)';
        g.fillRect(0, y, box.width, LANE);
      }
      if (!track) return;
      for (const clip of project.timeline.clips) {
        if (clip.track_id !== track.id) continue;
        const draggingThis = !!liveDrag && liveDrag.clipId === clip.id;
        const live = draggingThis ? liveDrag : null;
        const start = live && live.mode !== 'trim-in' ? live.previewMs : clip.start_ms;
        const duration = live
          ? live.mode === 'trim-out'
            ? Math.max(1, live.previewDurationMs)
            : live.mode === 'trim-in'
              ? clip.start_ms + clip.duration_ms - live.previewMs
              : clip.duration_ms
          : clip.duration_ms;
        const x0 = timeToX(start);
        const w = Math.max(2, (duration / 1000) * pps);
        const isSel = selected === clip.id;
        g.fillStyle =
          laneItem.kind === 'audio'
            ? 'rgba(70,214,140,0.28)'
            : laneItem.kind === 'overlay'
              ? 'rgba(228,70,155,0.28)'
              : 'rgba(117,49,254,0.30)';
        const yTop = y + 8;
        const h = LANE - 16;
        g.beginPath();
        g.roundRect(x0, yTop, w, h, 5);
        g.fill();
        g.strokeStyle = isSel ? ink('--accent-2') : ink('--line-strong');
        g.lineWidth = isSel ? 2 : 1;
        g.stroke();
        g.lineWidth = 1;
        if (track.locked) {
          g.strokeStyle = ink('--warn');
          g.strokeRect(x0 + 1.5, yTop + 1.5, w - 3, h - 3);
        }
        if (w > 46) {
          g.fillStyle = ink('--text');
          g.font = '11px Segoe UI, sans-serif';
          const name = clip.label || project.assets.find((a) => a.id === clip.asset_id)?.name || clip.asset_id.slice(0, 6);
          g.fillText(w < 90 ? name.slice(0, 14) : name.slice(0, 34), x0 + 6, yTop + 15);
          g.fillStyle = ink('--text-dim');
          g.font = '10px Consolas, monospace';
          g.fillText(`${(duration / 1000).toFixed(2)}s`, x0 + 6, yTop + h - 6);
        }
        if (live && live.mode === 'trim-in') {
          g.fillStyle = 'rgba(0,0,0,0.45)';
          g.fillRect(x0, yTop, timeToX(clip.start_ms) - x0, h);
        }
      }
      g.fillStyle = ink('--text-dim');
      g.font = '9px Consolas, monospace';
      g.fillText(laneItem.label, 4, y + LANE - 6);
    });

    // playhead
    const px = timeToX(playheadMs);
    g.strokeStyle = ink('--accent-3');
    g.beginPath();
    g.moveTo(px + 0.5, 0);
    g.lineTo(px + 0.5, box.height);
    g.stroke();
    g.fillStyle = ink('--accent-3');
    g.beginPath();
    g.moveTo(px - 5, 0);
    g.lineTo(px + 5, 0);
    g.lineTo(px, 9);
    g.closePath();
    g.fill();
  }

  $effect(() => {
    // redraw on every relevant change
    void pps; void scrollX; void playheadMs; void selected; void project; void drag?.previewMs; void drag?.previewDurationMs;
    const raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  });

  let saving: Promise<void> = Promise.resolve();

  async function applyNow(mutate: (clips: Clip[]) => Clip[]) {
    const timeline = { ...project.timeline, clips: mutate(project.timeline.clips) };
    try {
      const saved = await put<FullProject>(`/api/projects/${project.id}/timeline`, {
        timeline,
        expected_updated_at: project.updated_at,
      });
      onupdate(saved);
    } catch (error) {
      onstatus(`Timeline non salvata: ${(error as Error).message}`);
      try {
        onupdate(await get<FullProject>(`/api/projects/${project.id}`));
      } catch {
        /* the next manual refresh will retry */
      }
    }
  }

  // Fast keystrokes must not race the revision guard: edits queue one by one
  // and each mutator recomputes from the latest saved state.
  function enqueue(mutate: (clips: Clip[]) => Clip[]) {
    saving = saving.then(() => applyNow(mutate), () => applyNow(mutate));
    return saving;
  }

  function onPointerDown(event: PointerEvent) {
    const rect = canvas!.getBoundingClientRect();
    const found = hit(event.clientX - rect.left, event.clientY - rect.top);
    if (!found) {
      const ms = snapMs(xToTime(event.clientX - rect.left)).value;
      playheadMs = Math.max(0, ms);
      selected = null;
      return;
    }
    const { clip, edge } = found;
    selected = clip.id;
    const track = project.timeline.tracks.find((t) => t.id === clip.track_id);
    if (track?.locked) {
      onstatus(`Traccia «${track.name}» bloccata: sbloccala dal pannello classico per modificarla.`);
      return;
    }
    const grabMs = xToTime(event.clientX - rect.left) - clip.start_ms;
    drag = {
      clipId: clip.id,
      mode: edge === 'body' ? 'move' : edge === 'in' ? 'trim-in' : 'trim-out',
      grabMs,
      previewMs: clip.start_ms,
      previewDurationMs: clip.duration_ms,
      snapped: false,
    };
    canvas!.setPointerCapture(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    if (!drag) return;
    const rect = canvas!.getBoundingClientRect();
    const clip = project.timeline.clips.find((c) => c.id === drag!.clipId);
    if (!clip) return;
    const px = event.clientX - rect.left;
    if (drag.mode === 'move') {
      const snapped = snapMs(xToTime(px) - drag.grabMs);
      drag = { ...drag, previewMs: Math.max(0, snapped.value), snapped: snapped.snapped };
    } else if (drag.mode === 'trim-in') {
      const end = clip.start_ms + clip.duration_ms;
      const snapped = snapMs(xToTime(px));
      const value = Math.min(Math.max(0, snapped.value), end - 1);
      drag = { ...drag, previewMs: value, snapped: snapped.snapped };
    } else {
      const snapped = snapMs(xToTime(px));
      const duration = Math.max(1, snapped.value - clip.start_ms);
      drag = { ...drag, previewDurationMs: duration, snapped: snapped.snapped };
    }
  }

  async function onPointerUp() {
    if (!drag) return;
    const { clipId, mode, previewMs, previewDurationMs } = drag;
    drag = null;
    const clip = project.timeline.clips.find((c) => c.id === clipId);
    if (!clip) return;
    if (mode === 'move') {
      if (previewMs === clip.start_ms) return;
      const target = Math.round(previewMs);
      await enqueue((clips) =>
        clips.map((c) => (c.id === clipId ? { ...c, start_ms: Math.max(0, target) } : c)),
      );
    } else if (mode === 'trim-in') {
      if (previewMs === clip.start_ms) return;
      const requestedStart = Math.round(previewMs);
      await enqueue((clips) =>
        clips.map((c) => {
          if (c.id !== clipId) return c;
          const end = c.start_ms + c.duration_ms;
          const advance = Math.min(Math.max(0, requestedStart - c.start_ms), c.duration_ms - 1);
          return {
            ...c,
            start_ms: c.start_ms + advance,
            trim_in_ms: c.trim_in_ms + advance,
            duration_ms: end - c.start_ms - advance,
          };
        }),
      );
    } else {
      if (previewDurationMs === clip.duration_ms) return;
      const requestedEnd = Math.round(clip.start_ms + previewDurationMs);
      await enqueue((clips) =>
        clips.map((c) =>
          c.id === clipId ? { ...c, duration_ms: Math.max(1, requestedEnd - c.start_ms) } : c,
        ),
      );
    }
    onstatus(mode === 'move' ? 'Clip spostato e salvato.' : 'Trim applicato e salvato.');
  }

  function onWheel(event: WheelEvent) {
    event.preventDefault();
    if (event.ctrlKey) {
      const rect = canvas!.getBoundingClientRect();
      const anchor = xToTime(event.clientX - rect.left);
      const factor = Math.exp(-event.deltaY * 0.0015);
      pps = Math.min(600, Math.max(4, pps * factor));
      scrollX = Math.max(0, (anchor / 1000) * pps - (event.clientX - rect.left));
    } else {
      scrollX = Math.max(0, scrollX + (event.shiftKey ? event.deltaY : event.deltaX || event.deltaY));
    }
  }

  function nudge(frames: number) {
    const clip = project?.timeline.clips.find((c) => c.id === selected);
    if (!clip) return;
    const ms = Math.round((frames * 1000) / 30);
    void enqueue((clips) =>
      clips.map((c) =>
        c.id === clip.id ? { ...c, start_ms: Math.max(0, Math.round(c.start_ms + ms)) } : c,
      ),
    );
  }

  function onKeydown(event: KeyboardEvent) {
    if (!selected) return;
    if (event.key === 'ArrowLeft') nudge(event.shiftKey ? -5 : -1);
    else if (event.key === 'ArrowRight') nudge(event.shiftKey ? 5 : 1);
    else return;
    event.preventDefault();
  }

  function seek(ms: number) {
    playheadMs = Math.max(0, ms);
  }
  $effect(() => {
    exposed?.({ seek });
  });
</script>

<canvas
  class="timeline-canvas"
  bind:this={canvas}
  tabindex="0"
  aria-label="Timeline: frecce per spostare la clip selezionata, rotella per scorrere, Ctrl+rotella per lo zoom"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={() => (drag = null)}
  onwheel={onWheel}
  onkeydown={onKeydown}
></canvas>

<style>
  .timeline-canvas {
    width: 100%;
    height: 100%;
    display: block;
    border-radius: 8px;
  }
</style>
