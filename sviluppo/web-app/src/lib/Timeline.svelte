<script lang="ts">
  import { get, put } from './api';
  import type { AssetInfo, Clip, ProjectFull } from './types';

  type FullProject = ProjectFull;

  let {
    project,
    playheadMs,
    onstatus,
    onupdate,
    onplayhead,
    onselect,
    exposed,
  }: {
    project: FullProject;
    playheadMs: number;
    onstatus: (message: string) => void;
    onupdate: (project: FullProject) => void;
    onplayhead: (ms: number) => void;
    onselect?: (clipId: string | null) => void;
    exposed?: (api: { seek: (ms: number) => void; razor: () => void }) => void;
  } = $props();



  let canvas = $state<HTMLCanvasElement | null>(null);
  let pps = $state(60); // pixels per second
  let scrollX = $state(0);
  let selected = $state<string | null>(null);
  let mediaTick = $state(0);
  const posters = new Map<string, HTMLImageElement>();
  const waveforms = new Map<string, { peaks: [number, number][]; bin_ms: number }>();
  const waveformPending = new Set<string>();

  const posterPending = new Set<string>();

  function loadPoster(asset: AssetInfo) {
    if (!['video', 'image'].includes(asset.kind)) return;
    if (posters.has(asset.id) || posterPending.has(asset.id)) return;
    posterPending.add(asset.id);
    const img = new Image();
    img.onload = () => {
      posterPending.delete(asset.id);
      posters.set(asset.id, img);
      mediaTick++;
    };
    img.onerror = () => posterPending.delete(asset.id);
    img.src = `/api/projects/${project.id}/assets/${asset.id}/thumbnail`;
  }

  const peakCache = new Map<string, { count: number; value: number }>();
  const MAX_BOOST = 4;

  function isSmallLane(h: number): boolean {
    return h < 50;
  }

  function displayGain(assetId: string, wf: { peaks: [number, number][]; bin_ms: number }): number {
    // Suite peaks are signed 16-bit samples (±32768), unlike float sources.
    // Normalize the loudest bin to ~92 % of the zone, capped like FilmCraft's
    // MAX_BOOST so near-silent sources are not amplified into noise.
    const seen = peakCache.get(assetId);
    if (!seen || seen.count !== wf.peaks.length) {
      let peak = 0;
      for (const pair of wf.peaks) peak = Math.max(peak, Math.abs(pair[0]), Math.abs(pair[1]));
      peakCache.set(assetId, { count: wf.peaks.length, value: peak });
      return peak > 0 ? Math.min((32768 / peak) * 0.92, MAX_BOOST) : 1;
    }
    return seen.value > 0 ? Math.min((32768 / seen.value) * 0.92, MAX_BOOST) : 1;
  }

  function loadWaveform(asset: AssetInfo) {
    if (!['video', 'audio'].includes(asset.kind)) return;
    if (waveforms.has(asset.id) || waveformPending.has(asset.id)) return;
    waveformPending.add(asset.id);
    void get<{ peaks?: [number, number][]; bin_ms?: number }>(
      `/api/v1/projects/${project.id}/assets/${asset.id}/peaks`,
    )
      .then((value) => {
        waveformPending.delete(asset.id);
        if (Array.isArray(value.peaks) && value.bin_ms) {
          waveforms.set(asset.id, { peaks: value.peaks, bin_ms: value.bin_ms });
          mediaTick++;
        }
      })
      .catch(() => {
        /* peaks are optional decoration: a missing cache simply stays silent */
        waveformPending.delete(asset.id);
      });
  }
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
        const asset = project.assets.find((a) => a.id === clip.asset_id);
        if (asset) {
          const img = posters.get(asset.id);
          if (img && laneItem.kind !== 'audio' && w > 36) {
            g.save();
            g.beginPath();
            g.roundRect(x0, yTop, w, h, 5);
            g.clip();
            const ratio = (img.naturalWidth || 16) / (img.naturalHeight || 9);
            const tileW = Math.max(28, (h - 2) * ratio);
            for (let tx = x0 + 1; tx < x0 + w - 2; tx += tileW) {
              g.drawImage(img, tx, yTop + 1, tileW, h - 2);
            }
            g.restore();
          }
          const wf = waveforms.get(asset.id);
          if (wf && w > 18) {
            // Premiere-style audio band, following the FilmCraft timeline pattern:
            // the zone is the lower 42 % of the clip body (all of it for tiny rows
            // and for the audio lane), one column per screen pixel aggregating the
            // source bins it covers, and a bounded peak-normalisation boost so a
            // quiet voice still draws a readable envelope (never a fake curve).
            const isAudioLane = laneItem.kind === 'audio';
            const hasAudio = asset.media?.has_audio !== false;
            if (hasAudio && (isAudioLane || laneItem.kind === 'video')) {
              const zone = isAudioLane ? h - 4 : Math.max(8, isSmallLane(h) ? h - 2 : h * 0.42);
              const areaTop = yTop + h - 2 - zone;
              const bottom = yTop + h - 2;
              const gain = displayGain(asset.id, wf);
              const scale = (zone / 2 - 1) / 32768;
              const midY = areaTop + zone / 2;
              g.save();
              g.beginPath();
              g.roundRect(x0, yTop, w, h, 5);
              g.clip();
              if (isAudioLane) {
                g.strokeStyle = 'rgba(236,234,246,0.14)';
                g.beginPath();
                g.moveTo(x0, midY + 0.5);
                g.lineTo(x0 + w, midY + 0.5);
                g.stroke();
              }
              g.fillStyle = isAudioLane ? 'rgba(139,228,230,0.8)' : 'rgba(139,228,230,0.55)';
              const from = Math.max(0, Math.floor(x0));
              for (let sx = from; sx < x0 + w; sx++) {
                const t0 = xToTime(sx);
                const t1 = xToTime(sx + 1);
                const s0 = Math.floor((clip.trim_in_ms + Math.max(0, t0 - start)) / wf.bin_ms);
                let s1 = Math.floor((clip.trim_in_ms + Math.max(0, t1 - start)) / wf.bin_ms);
                if (s1 <= s0) s1 = s0 + 1;
                let m = 0;
                for (let i = s0; i < s1 && i < wf.peaks.length; i++) {
                  const pair = wf.peaks[i];
                  if (!pair) continue;
                  m = Math.max(m, Math.abs(pair[0]), Math.abs(pair[1]));
                }
                const half = Math.min(zone / 2, Math.max(0.5, m * gain * scale));
                g.fillRect(sx, midY - half, 1, half * 2);
              }
              g.restore();
            }
          }
        }
        if (w > 46) {
          g.fillStyle = 'rgba(10,10,18,0.72)';
          const name = clip.label || asset?.name || clip.asset_id.slice(0, 6);
          const shown = w < 90 ? name.slice(0, 14) : name.slice(0, 34);
          g.font = '11px Segoe UI, sans-serif';
          const textW = g.measureText(shown).width;
          g.beginPath();
          g.roundRect(x0 + 4, yTop + 2, textW + 10, 17, 4);
          g.fill();
          g.fillStyle = '#f2f0fa';
          g.fillText(shown, x0 + 9, yTop + 15);
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

  let lastProjectId = '';

  $effect(() => {
    // redraw on every relevant change
    void pps; void scrollX; void playheadMs; void selected; void mediaTick; void project;
    void drag?.previewMs; void drag?.previewDurationMs;
    if (project && project.id !== lastProjectId) {
      lastProjectId = project.id;
      posters.clear();
      waveforms.clear();
      peakCache.clear();
      posterPending.clear();
      waveformPending.clear();
    }
    if (project) {
      for (const asset of project.assets) {
        if (project.timeline.clips.some((c) => c.asset_id === asset.id)) {
          loadPoster(asset);
          loadWaveform(asset);
        }
      }
    }
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
      onplayhead(Math.max(0, ms));
      selected = null;
      onselect?.(null);
      return;
    }
    const { clip, edge } = found;
    selected = clip.id;
    onselect?.(clip.id);
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
    onplayhead(Math.max(0, ms));
  }

  // Razor: split the selected clip at the playhead. Same arithmetic as the
  // server endpoint (first keeps duration=offset, second shifts start and
  // trim_in), but committed through the revision-guarded PUT so it joins the
  // existing write queue instead of racing it.
  function razor() {
    const clip = project?.timeline.clips.find((c) => c.id === selected);
    if (!clip) {
      onstatus('Seleziona prima una clip sulla timeline, poi premi C.');
      return;
    }
    const at = playheadMs;
    const end = clip.start_ms + clip.duration_ms;
    if (at <= clip.start_ms || at >= end) {
      onstatus('La playhead deve essere dentro la clip selezionata.');
      return;
    }
    const offset = Math.round(at - clip.start_ms);
    const first: Clip = { ...clip, duration_ms: offset };
    const second: Clip = {
      ...clip,
      id: crypto.randomUUID(),
      start_ms: clip.start_ms + offset,
      trim_in_ms: clip.trim_in_ms + offset,
      duration_ms: clip.duration_ms - offset,
    };
    void enqueue((clips) => clips.flatMap((c) => (c.id === clip.id ? [first, second] : c)));
    onstatus('Clip divisa alla playhead.');
  }

  $effect(() => {
    exposed?.({ seek, razor });
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
