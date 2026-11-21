<script lang="ts">
  import { playbackUrl } from './media';

  interface Clip {
    id: string;
    asset_id: string;
    track_id: string;
    start_ms: number;
    trim_in_ms: number;
    duration_ms: number;
    label?: string | null;
    volume?: number;
    opacity?: number;
    transform?: Record<string, unknown> | null;
  }
  interface Track {
    id: string;
    name: string;
    kind: string;
    visible: boolean;
    locked: boolean;
  }
  interface ProgramProject {
    id: string;
    updated_at: string;
    timeline: { tracks: Track[]; clips: Clip[] };
    assets: { id: string; name: string; kind: string }[];
  }

  let {
    project,
    playheadMs,
    onplayhead,
    onstatus,
    exposed,
  }: {
    project: ProgramProject;
    playheadMs: number;
    onplayhead: (ms: number) => void;
    onstatus: (message: string) => void;
    exposed?: (api: { toggle: () => void; rate: (value: number) => void }) => void;
  } = $props();

  let video = $state<HTMLVideoElement | null>(null);
  let current = $state<{ clip: Clip; sourceMs: number } | null>(null);
  let playing = $state(false);
  let note = $state('Scegli un punto della timeline.');
  let lastMediaKey = '';
  let safeMargins = $state(true);

  function topmostClipAt(ms: number): Clip | null {
    const pick = (kind: string): Clip | null => {
      const track = project?.timeline.tracks.find((t) => t.kind === kind && t.visible);
      if (!track) return null;
      return (
        project.timeline.clips.find(
          (c) => c.track_id === track.id && c.start_ms <= ms && c.start_ms + c.duration_ms > ms,
        ) ?? null
      );
    };
    return pick('overlay') ?? pick('video') ?? pick('audio');
  }

  // The program follows the playhead when paused and owns the clock while
  // playing, exactly like a monitor of a single composition.
  $effect(() => {
    void playheadMs;
    if (!project) return;
    const clip = topmostClipAt(playheadMs);
    if (!clip) {
      current = null;
      if (playing) {
        playing = false;
        video?.pause();
        note = 'Nessun media a questa altezza: intervallo vuoto (nero/silenzio).';
      } else {
        note = 'Intervallo vuoto: nero e silenzio come nell’export.';
      }
      return;
    }
    current = { clip, sourceMs: clip.trim_in_ms + (playheadMs - clip.start_ms) };
    void prepareMedia(clip);
  });

  async function prepareMedia(clip: Clip) {
    const key = `${project.id}:${clip.asset_id}`;
    if (playing && lastMediaKey === key) return;
    if (!playing && lastMediaKey === key && current && video && Math.abs(video.currentTime * 1000 - current.sourceMs) < 220) return;
    try {
      const url = await playbackUrl(project.id, clip.asset_id);
      if (lastMediaKey !== key) {
        lastMediaKey = key;
        if (video) {
          video.src = url;
          video.load();
        }
      }
      note = playing ? 'Riproduzione in corso.' : 'Pronto: Spazio per riprodurre, L/K per velocità.';
      const gain = current?.clip.volume ?? 1;
      if (gain > 1) {
        note += ` Audio in anteprima al 100%; l’export applica il gain ${gain.toFixed(2)}×.`;
      }
      if (playing) void video?.play();
    } catch (error) {
      lastMediaKey = '';
      note = (error as Error).message;
      onstatus(note);
    }
  }

  function syncClock() {
    if (!video || !current) return;
    if (Math.abs(video.currentTime * 1000 - current.sourceMs) > 180) {
      video.currentTime = current.sourceMs / 1000;
    }
  }

  // While paused, the playhead drives the media position.
  $effect(() => {
    void playheadMs;
    if (!playing) syncClock();
  });

  function onTimeUpdate() {
    if (!playing || !current || !video) return;
    const clip = current.clip;
    const timelineMs = clip.start_ms + (video.currentTime * 1000 - clip.trim_in_ms);
    if (timelineMs >= clip.start_ms + clip.duration_ms) {
      // continue across gaps: jump the playhead to the next media
      const nextStart = project.timeline.clips
        .filter((c) => c.start_ms > clip.start_ms + clip.duration_ms - 1 && topmostClipAt(c.start_ms)?.id === c.id)
        .map((c) => c.start_ms)
        .sort((a, b) => a - b)[0];
      if (nextStart === undefined) {
        playing = false;
        video.pause();
        onplayhead(clip.start_ms + clip.duration_ms);
        note = 'Fine del montaggio.';
        return;
      }
      onplayhead(nextStart);
      return;
    }
    if (Math.abs(timelineMs - playheadMs) > 33) onplayhead(Math.max(0, timelineMs));
  }

  function toggle() {
    if (!video) return;
    if (playing) {
      playing = false;
      video.pause();
      note = 'In pausa.';
      return;
    }
    if (!current) {
      note = 'Nessun media sotto la playhead.';
      return;
    }
    playing = true;
    syncClock();
    void video.play().catch(() => {
      playing = false;
      note = 'Riproduzione bloccata dal browser: clicca sul monitor una volta.';
    });
    note = 'Riproduzione in corso.';
  }

  function rate(value: number) {
    if (!video) return;
    video.playbackRate = value;
    onstatus(`Velocità anteprima: ${value}× (l’export non cambia).`);
  }

  function onLoadedMetadata() {
    syncClock();
  }

  $effect(() => {
    exposed?.({ toggle, rate });
  });

  const tc = $derived(
    (() => {
      const ms = Math.max(0, Math.round(playheadMs));
      const h = Math.floor(ms / 3_600_000);
      const m = Math.floor((ms % 3_600_000) / 60_000);
      const s = Math.floor((ms % 60_000) / 1000);
      const f = Math.floor((ms % 1000) / (1000 / 30));
      return [h, m, s, f].map((v) => String(v).padStart(2, '0')).join(':');
    })(),
  );

  // The Inspector writes volume, opacity, fades, fit and (from the classic UI)
  // scale/rotation/position; the monitor must show exactly what the export will
  // do, not an unfiltered decode.
  const look = $derived.by(() => {
    const clip = current?.clip;
    if (!clip) return { opacity: 1, volume: 1, fit: 'contain', transform: 'none', faded: false };
    const t = playheadMs;
    const rel = t - clip.start_ms;
    const remain = clip.start_ms + clip.duration_ms - t;
    const tf = (clip.transform ?? {}) as Record<string, unknown>;
    const num = (key: string, fallback: number) => {
      const value = tf[key];
      return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
    };
    const fadeIn = Math.max(0, num('fade_in_ms', 0));
    const fadeOut = Math.max(0, num('fade_out_ms', 0));
    let fade = 1;
    let faded = false;
    if (fadeIn > 0 && rel < fadeIn) {
      fade = Math.min(fade, Math.max(0, rel / fadeIn));
      faded = true;
    }
    if (fadeOut > 0 && remain < fadeOut) {
      fade = Math.min(fade, Math.max(0, remain / fadeOut));
      faded = true;
    }
    const opacity = Math.max(0, Math.min(1, (clip.opacity ?? 1) * fade));
    // HTMLMediaElement caps at 1; the export applies the full 0–4 gain.
    const volume = Math.max(0, Math.min(1, clip.volume ?? 1));
    const fit = typeof tf.fit === 'string' && ['contain', 'cover'].includes(tf.fit) ? tf.fit : 'contain';
    const scale = num('scale', 1);
    const rotation = num('rotation', 0);
    const x = num('x', 0);
    const y = num('y', 0);
    // The classic transform stores 0.5/0.5 for a centred subject (normalized
    // focus), so only values outside -1..1 are treated as percentage offsets.
    const px = Math.abs(x) > 1 ? x : 0;
    const py = Math.abs(y) > 1 ? y : 0;
    const parts: string[] = [];
    if (px || py) parts.push(`translate(${px}%, ${py}%)`);
    if (scale !== 1) parts.push(`scale(${scale})`);
    if (rotation) parts.push(`rotate(${rotation}deg)`);
    return {
      opacity,
      volume,
      fit,
      transform: parts.length ? parts.join(' ') : 'none',
      faded,
    };
  });

  $effect(() => {
    if (video) video.volume = look.volume;
  });
</script>

<div class="program">
  <div class="stage">
    <!-- svelte-ignore a11y_media_has_caption -->
    <video
      bind:this={video}
      playsinline
      preload="metadata"
      style="opacity: {look.opacity}; object-fit: {look.fit}; transform: {look.transform};"
      class:audio-only={!!current && project.assets.find((a) => a.id === current?.clip.asset_id)?.kind === 'audio'}
      onclick={toggle}
      ontimeupdate={onTimeUpdate}
      onloadedmetadata={onLoadedMetadata}
    ></video>
    <!-- Premiere-style guides: action-safe 90 %, title-safe 80 % -->
    <div class="safe" class:on={safeMargins} aria-hidden="true">
      <div class="action"></div>
      <div class="title"></div>
      <div class="cross"></div>
    </div>
  </div>
  <div class="controls">
    <button class="ghost" title="All'inizio (Home)" onclick={() => onplayhead(0)}>⏮</button>
    <button class="primary" title="Riproduci / pausa (Spazio)" onclick={toggle}>
      {playing ? '⏸' : '▶'}
    </button>
    <button class="ghost" title="Indietro di un frame (J)" onclick={() => onplayhead(Math.max(0, playheadMs - 33))}>
      ◀︎◀︎
    </button>
    <button class="ghost speed" title="Velocità 1× (K)" onclick={() => rate(1)}>1×</button>
    <button class="ghost speed" title="Velocità 2× (L)" onclick={() => rate(2)}>2×</button>
    <button
      class="ghost speed"
      aria-pressed={safeMargins}
      class:active={safeMargins}
      title="Mostra/nascondi le guide action-safe e title-safe"
      onclick={() => (safeMargins = !safeMargins)}
    >
      Safe
    </button>
    <span class="tc" aria-label="Timecode">{tc}</span>
    <span class="muted clip-name">{current ? current.clip.label ?? 'clip' : ''}</span>
  </div>
  <p class="muted note">{note}</p>
</div>

<style>
  .program { display: flex; flex-direction: column; gap: 6px; height: 100%; min-height: 0; }
  .stage { position: relative; flex: 1; min-height: 0; display: flex; }
  .stage video { flex: 1; min-height: 0; width: 100%; background: #000; border-radius: 8px; object-fit: contain; }
  .stage video.audio-only { flex: 0 0 40%; }
  .safe { position: absolute; inset: 0; pointer-events: none; display: none; border-radius: 8px; overflow: hidden; }
  .safe.on { display: block; }
  .safe .action { position: absolute; inset: 5%; border: 1px dashed rgba(255, 255, 255, 0.5); }
  .safe .title { position: absolute; inset: 10%; border: 1px dashed rgba(139, 228, 230, 0.75); }
  .safe .cross {
    position: absolute; left: 50%; top: 50%; width: 22px; height: 22px; transform: translate(-50%, -50%);
    background:
      linear-gradient(rgba(255,255,255,.55), rgba(255,255,255,.55)) center / 1px 100% no-repeat,
      linear-gradient(rgba(255,255,255,.55), rgba(255,255,255,.55)) center / 100% 1px no-repeat;
  }
  .controls { display: flex; gap: 6px; align-items: center; flex: none; }
  .tc { font: 12px Consolas, monospace; color: var(--accent-3); }
  .clip-name { font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ghost { background: transparent; }
  .ghost.active { border-color: var(--accent-2); color: var(--accent-2); }
  .speed { padding: 3px 7px; font-size: 11px; }
  .note { flex: none; margin: 0; font-size: 12px; }
</style>
