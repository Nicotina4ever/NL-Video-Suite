<script lang="ts">
  import { get, post, put } from './api';
  import type { AssetInfo, Clip, EditTool, ProjectFull, Track } from './types';
  import { rippleTrim, roll, slip, slide, trackClips } from './editAlgebra';
  import type { AClip, MediaMap } from './editAlgebra';

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
    exposed?: (api: { seek: (ms: number) => void; razor: () => void; setTool: (tool: EditTool) => void }) => void;
  } = $props();



  let canvas = $state<HTMLCanvasElement | null>(null);
  let pps = $state(60); // pixels per second
  let scrollX = $state(0);
  let selected = $state<string | null>(null);
  let mediaTick = $state(0);
  let snapEnabled = $state(true);
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
    const ensure = async () => {
      for (let attempt = 0; attempt < 60; attempt++) {
        try {
          const value = await get<{ peaks?: [number, number][]; bin_ms?: number }>(
            `/api/v1/projects/${project.id}/assets/${asset.id}/peaks`,
          );
          if (Array.isArray(value.peaks) && value.bin_ms) {
            waveforms.set(asset.id, { peaks: value.peaks, bin_ms: value.bin_ms });
            mediaTick++;
          }
          waveformPending.delete(asset.id);
          return;
        } catch {
          if (attempt === 0) {
            // no cache yet: trigger the (heavy, background) preparation
            // job the classic UI also uses, then keep retrying the GET
            await post(`/api/v1/projects/${project.id}/assets/${asset.id}/peaks`, {}).catch(() => undefined);
          }
          await new Promise((resolve) => setTimeout(resolve, 3000));
        }
      }
      waveformPending.delete(asset.id);
    };
    void ensure();
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

  const durationMs = $derived(
    project.timeline.clips.reduce((end, c) => Math.max(end, c.start_ms + c.duration_ms), 0),
  );
  const orderedTracks = $derived(
    (['overlay', 'video', 'audio'] as const)
      .map((kind) => project.timeline.tracks.find((t) => t.kind === kind))
      .filter((t): t is Track => !!t),
  );
  function tc(ms: number): string {
    const value = Math.max(0, Math.round(ms));
    const m = Math.floor(value / 60_000);
    const s = Math.floor((value % 60_000) / 1000);
    const f = Math.floor((value % 1000) / (1000 / 30));
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}:${String(f).padStart(2, '0')}`;
  }
  function fit() {
    const width = canvas?.clientWidth || 800;
    const total = durationMs || 10_000;
    pps = Math.min(600, Math.max(4, (width - 24) / (total / 1000)));
    scrollX = 0;
  }
  async function toggleTrack(trackId: string, key: 'visible' | 'locked') {
    const timeline = {
      ...project.timeline,
      tracks: project.timeline.tracks.map((t) => (t.id === trackId ? { ...t, [key]: !t[key] } : t)),
    };
    try {
      const saved = await put<FullProject>(`/api/projects/${project.id}/timeline`, {
        timeline,
        expected_updated_at: project.updated_at,
      });
      onupdate(saved);
      const track = timeline.tracks.find((t) => t.id === trackId);
      onstatus(
        `${track?.name ?? 'traccia'}: ${key === 'visible' ? (track?.visible ? 'visibile' : 'nascosta') : track?.locked ? 'bloccata' : 'sbloccata'}.`,
      );
    } catch (error) {
      onstatus(`Traccia non aggiornata: ${(error as Error).message}`);
    }
  }

  function snapCandidates(): number[] {
    const values = [0, playheadMs];
    for (const c of project.timeline.clips) {
      values.push(c.start_ms, c.start_ms + c.duration_ms);
    }
    return values;
  }

  function snapMs(ms: number): { value: number; snapped: boolean } {
    if (!snapEnabled) return { value: ms, snapped: false };
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
      for (const clip of algebraPreview ?? project.timeline.clips) {
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
    void drag?.previewMs; void drag?.previewDurationMs; void algebraPreview;
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

  // ---- edit tools (algebra from ./editAlgebra) ----
  type Tool = EditTool;
  const TOOLS: [Tool, string][] = [
    ['trim', 'Taglio'],
    ['ripple', 'Ripple'],
    ['roll', 'Roll'],
    ['slip', 'Slip'],
    ['slide', 'Slide'],
  ];
  let tool = $state<Tool>('trim');
  let algebraBase = $state<Clip[] | null>(null);
  let algebraPreview = $state<Clip[] | null>(null);
  let algebraReason = $state('');
  let algebraTarget = '';
  let algebraEdge: 'in' | 'out' | 'body' = 'body';
  let algebraTrack = '';
  let downPx = 0;

  function mediaDurations(): MediaMap {
    const map: MediaMap = {};
    for (const a of project.assets) map[a.id] = a.media?.duration_ms;
    return map;
  }

  function adjacentAround(clipId: string, edge: 'in' | 'out'): AClip | null {
    const track = trackClips(project.timeline.clips as AClip[], algebraTrack);
    const i = track.findIndex((c) => c.id === clipId);
    if (i < 0) return null;
    if (edge === 'out') {
      const next = track[i + 1];
      const here = track[i];
      return next && here.start_ms + here.duration_ms === next.start_ms ? next : null;
    }
    const prev = track[i - 1];
    const here = track[i];
    return prev && prev.start_ms + prev.duration_ms === here.start_ms ? prev : null;
  }

  function algebraStart(clipId: string, edge: 'in' | 'out' | 'body', startX: number) {
    const clip = project.timeline.clips.find((c) => c.id === clipId);
    if (!clip) return;
    algebraTrack = clip.track_id;
    algebraTarget = clipId;
    algebraEdge = edge;
    downPx = startX;
    algebraBase = JSON.parse(JSON.stringify(project.timeline.clips)) as Clip[];
    algebraPreview = null;
    algebraReason = '';
  }

  function algebraMove(px: number) {
    if (!algebraBase) return;
    // delta in ms from the press point; screen px are time at the current zoom
    const delta = Math.round(xToTime(px) - xToTime(downPx));
    const m = mediaDurations();
    if (tool === 'ripple' && algebraEdge !== 'body') {
      const res = rippleTrim(algebraBase, algebraTrack, algebraTarget, algebraEdge === 'in' ? 'start' : 'end', delta, m);
      algebraPreview = res.clips as Clip[];
      algebraReason = res.reason ?? '';
    } else if (tool === 'roll' && algebraEdge !== 'body') {
      if (algebraEdge === 'out') {
        const next = adjacentAround(algebraTarget, 'out');
        if (!next) {
          algebraPreview = null;
          algebraReason = 'roll: serve un clip adiacente a destra (taglio condiviso)';
          return;
        }
        const res = roll(algebraBase, algebraTrack, algebraTarget, next.id, delta, m);
        algebraPreview = res.clips as Clip[];
          algebraReason = res.reason ?? '';
      } else {
        const prev = adjacentAround(algebraTarget, 'in');
        if (!prev) {
          algebraPreview = null;
          algebraReason = 'roll: serve un clip adiacente a sinistra (taglio condiviso)';
          return;
        }
        // dragging the left cut with a negative delta mirrors the right-cut roll
        const res = roll(algebraBase, algebraTrack, prev.id, algebraTarget, delta, m);
        algebraPreview = res.clips as Clip[];
          algebraReason = res.reason ?? '';
      }
    } else if (tool === 'slip' && algebraEdge === 'body') {
      const res = slip(algebraBase, algebraTarget, delta, m);
      algebraPreview = res.clips as Clip[];
      algebraReason = res.reason ?? '';
    } else if (tool === 'slide' && algebraEdge === 'body') {
      const res = slide(algebraBase, algebraTrack, algebraTarget, delta, m);
      algebraPreview = res.clips as Clip[];
      algebraReason = res.reason ?? '';
    }
  }

  async function algebraFinish() {
    const base = algebraBase;
    const preview = algebraPreview;
    const reason = algebraReason;
    const trackId = algebraTrack;
    algebraBase = null;
    algebraPreview = null;
    algebraReason = '';
    if (!base || !preview) {
      if (reason) onstatus(reason);
      return;
    }
    const changed = JSON.stringify(base) !== JSON.stringify(preview);
    if (!changed) return;
    const previewForTrack = preview.filter((c) => c.track_id === trackId);
    await enqueue((clips) => [
      ...clips.filter((c) => c.track_id !== trackId),
      ...previewForTrack.map((c) => ({ ...c, label: c.label ?? null })),
    ]);
    onstatus(`Modifica ${tool} applicata (traccia preservata, gli altri clip intatti).`);
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
    if (tool !== 'trim') {
      algebraStart(clip.id, edge, event.clientX - rect.left);
      canvas!.setPointerCapture(event.pointerId);
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
    const rect = canvas!.getBoundingClientRect();
    if (algebraBase) {
      algebraMove(event.clientX - rect.left);
      return;
    }
    if (!drag) return;
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
    if (algebraBase) {
      await algebraFinish();
      return;
    }
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

  function onDragOver(event: DragEvent) {
    if (!event.dataTransfer || !event.dataTransfer.types.includes('text/nl-asset-id')) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
  }

  async function onDrop(event: DragEvent) {
    const assetId = event.dataTransfer?.getData('text/nl-asset-id');
    if (!assetId) return;
    event.preventDefault();
    const rect = canvas!.getBoundingClientRect();
    const px = event.clientX - rect.left;
    const py = event.clientY - rect.top;
    const lane = Math.floor((py - RULER) / LANE);
    const laneKinds = ['overlay', 'video', 'audio'];
    if (lane < 0 || lane >= laneKinds.length) return;
    const asset = project.assets.find((a) => a.id === assetId);
    if (!asset) return;
    // audio goes to the audio lane, images to overlay; video follows the lane
    const kind = asset.kind === 'audio' ? 'audio' : asset.kind === 'image' ? 'overlay' : laneKinds[lane];
    const track = project.timeline.tracks.find((t) => t.kind === kind);
    if (!track) {
      onstatus('Nessuna traccia adatta a questo media: aggiungila dal pannello classico.');
      return;
    }
    const at = Math.max(0, Math.round(snapMs(xToTime(px)).value));
    try {
      const clip = await post<{ start_ms: number }>(`/api/projects/${project.id}/timeline/clips`, {
        asset_id: asset.id,
        track_id: track.id,
        start_ms: at,
        label: asset.name,
      });
      onupdate(await get<FullProject>(`/api/projects/${project.id}`));
      onstatus(`«${asset.name}» rilasciata al minuto ${(clip.start_ms / 1000).toFixed(2)} s.`);
    } catch (error) {
      onstatus(`Rilascio non riuscito: ${(error as Error).message}`);
    }
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
  function setTool(next: Tool) {
    tool = next;
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
    exposed?.({ seek, razor, setTool });
  });
</script>

<div class="tl-wrap">
  <div class="tools" role="toolbar" aria-label="Strumenti di montaggio">
    {#each TOOLS as [value, label] (value)}
      <button class="tool" class:active={tool === value} onclick={() => (tool = value)}
        title={value === 'trim' ? 'Taglio classico: bordi liberi (V)'
          : value === 'ripple' ? 'Ripple: le teste si consumano e il seguito si avvicina (N)'
          : value === 'roll' ? 'Roll: sposti il taglio condiviso, la durata totale non cambia (R)'
          : value === 'slip' ? 'Slip: finestra sorgente in movimento, posizione fissa (Y)'
          : 'Slide: il clip scorre, i vicini assorbono (U)'}>
        {label}
      </button>
    {/each}
    {#if algebraReason}
      <span class="muted reason">{algebraReason}</span>
    {/if}
    {#if tool !== 'trim'}
      <span class="muted hint">{tool === 'slip' || tool === 'slide' ? 'trascina il corpo del clip' : 'trascina un bordo del clip'}</span>
    {/if}
  </div>
  <div class="tools view" role="toolbar" aria-label="Vista, snap e tracce">
    <span class="tc" aria-label="Tempo della playhead">{tc(playheadMs)}</span>
    <span class="muted total">/ {tc(durationMs)}</span>
    <span class="spacer"></span>
    <label class="inline"><input type="checkbox" bind:checked={snapEnabled} /> Snap</label>
    <label class="inline zoom">Zoom<input type="range" min="4" max="300" step="1" bind:value={pps} /></label>
    <button class="tool" onclick={fit} title="Adatta l’intero montaggio alla larghezza">Adatta</button>
    <span class="tracks">
      {#each orderedTracks as track (track.id)}
        <span class="track-chip" class:off={!track.visible} class:locked={track.locked}>
          <button
            class="chip-name"
            title={track.visible ? 'Nascondi traccia' : 'Mostra traccia'}
            onclick={() => toggleTrack(track.id, 'visible')}
          >
            {track.name}
          </button>
          <button
            class="chip-lock"
            title={track.locked ? 'Sblocca traccia' : 'Blocca traccia'}
            onclick={() => toggleTrack(track.id, 'locked')}
          >
            {track.locked ? '🔒' : '🔓'}
          </button>
        </span>
      {/each}
    </span>
  </div>
  <canvas
    class="timeline-canvas"
    class:algebraic={tool !== 'trim'}
    bind:this={canvas}
    tabindex="0"
    aria-label="Timeline: frecce per spostare la clip selezionata, rotella per scorrere, Ctrl+rotella per lo zoom"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={() => {
      drag = null;
      algebraBase = null;
      algebraPreview = null;
    }}
    ondragover={onDragOver}
    ondrop={onDrop}
    onwheel={onWheel}
    onkeydown={onKeydown}
  ></canvas>
</div>

<style>
  .tl-wrap { display: flex; flex-direction: column; height: 100%; min-height: 0; gap: 4px; }
  .tools { display: flex; gap: 4px; align-items: center; flex-wrap: wrap; }
  .tool { padding: 2px 9px; font-size: 12px; background: transparent; border-color: var(--line); }
  .tool.active { border-color: var(--accent-2); background: rgba(228, 70, 155, 0.16); }
  .view { font-size: 12px; }
  .view .spacer { flex: 1; }
  .view .inline { display: flex; align-items: center; gap: 4px; color: var(--text-dim); }
  .view .zoom input[type='range'] { width: 96px; }
  .view .tc { font: 12px Consolas, monospace; color: var(--accent-3); }
  .view .total { font: 11px Consolas, monospace; }
  .tracks { display: flex; gap: 4px; }
  .track-chip { display: flex; align-items: center; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  .track-chip.off { opacity: 0.5; }
  .track-chip.locked { border-color: var(--warn); }
  .chip-name, .chip-lock { background: transparent; border: 0; padding: 2px 7px; font-size: 11px; cursor: pointer; color: var(--text); }
  .chip-lock { border-left: 1px solid var(--line); }
  .reason { font-size: 12px; color: var(--warn); }
  .hint { font-size: 12px; }
  .timeline-canvas { flex: 1; min-height: 0; width: 100%; display: block; border-radius: 8px; }
  .algebraic { cursor: crosshair; }
</style>
