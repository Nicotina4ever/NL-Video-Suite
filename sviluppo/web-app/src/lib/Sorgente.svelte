<script lang="ts">
  import { post } from './api';
  import { playbackUrl } from './media';

  interface Asset {
    id: string;
    name: string;
    kind: string;
    media?: { duration_ms?: number; has_audio?: boolean } | null;
  }
  interface FullProject {
    id: string;
    updated_at: string;
    assets: Asset[];
    timeline: { clips: unknown[]; tracks: unknown[] };
  }

  let {
    project,
    onstatus,
    onadded,
  }: {
    project: FullProject;
    onstatus: (message: string) => void;
    onadded: () => void | Promise<void>;
  } = $props();

  let video = $state<HTMLVideoElement | null>(null);
  let asset = $state<Asset | null>(null);
  let loading = $state(false);
  let note = $state('Scegli un media del progetto.');
  let inMs: number | null = null;
  let outMs: number | null = null;
  let marks = $state({ in: null as number | null, out: null as number | null });

  const mediaAssets = $derived(project?.assets.filter((a) => ['video', 'audio', 'image'].includes(a.kind)) ?? []);

  $effect(() => {
    if (!asset && mediaAssets.length) asset = mediaAssets[0];
  });

  $effect(() => {
    const current = asset;
    if (!current || !video || current.kind === 'image') return;
    let cancelled = false;
    loading = true;
    note = 'Preparazione sorgente…';
    playbackUrl(project.id, current.id)
      .then((url) => {
        if (cancelled) return;
        loading = false;
        video!.src = url;
        video!.load();
        note = current.name;
      })
      .catch((error) => {
        if (cancelled) return;
        loading = false;
        note = (error as Error).message;
        onstatus(note);
      });
    return () => {
      cancelled = true;
    };
  });

  const time = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(2).padStart(5, '0')}`;

  function setMark(which: 'in' | 'out') {
    if (!video) return;
    const ms = Math.round(video.currentTime * 1000);
    if (which === 'in') {
      inMs = ms;
      outMs = null;
    } else if (inMs === null || ms > inMs) {
      outMs = ms;
    }
    marks = { in: inMs, out: outMs };
  }

  async function addToTimeline() {
    if (!asset || !video) return;
    const durationMs = Math.round(video.duration * 1000);
    const start = marks.in ?? 0;
    const end = Math.min(marks.out ?? durationMs, durationMs);
    if (end - start < 100) {
      onstatus('La selezione sorgente è troppo corta (minimo 100 ms).');
      return;
    }
    try {
      const clip = await post<{ start_ms: number }>(`/api/projects/${project.id}/timeline/clips`, {
        asset_id: asset.id,
        trim_in_ms: start,
        duration_ms: end - start,
        label: asset.name,
      });
      await onadded();
      onstatus(`Clip aggiunta al minuto ${time(clip.start_ms)}: sorgente ${time(start)} → ${time(end)}.`);
    } catch (error) {
      onstatus((error as Error).message);
    }
  }
</script>

<div class="source">
  <label class="picker">
    <span class="muted">Media</span>
    <select
      value={asset?.id ?? ''}
      onchange={(e) => {
        asset = mediaAssets.find((a) => a.id === (e.target as HTMLSelectElement).value) ?? null;
        marks = { in: null, out: null };
        inMs = null;
        outMs = null;
      }}
    >
      {#each mediaAssets as a (a.id)}
        <option value={a.id}>{a.name}</option>
      {/each}
    </select>
  </label>
  {#if asset?.kind === 'image'}
    <img class="preview" src="/api/projects/{project.id}/assets/{asset.id}/thumbnail" alt={asset.name} />
  {:else}
    <!-- svelte-ignore a11y_media_has_caption -->
    <video bind:this={video} controls playsinline preload="metadata" onloadeddata={() => (note = asset?.name ?? '')}></video>
  {/if}
  <div class="marks">
    <button class="ghost" onclick={() => setMark('in')}>⟦ In</button>
    <button class="ghost" onclick={() => setMark('out')}>Out ⟧</button>
    <span class="muted">
      {marks.in !== null ? `in ${time(marks.in)}` : 'in = inizio'} ·
      {marks.out !== null ? `out ${time(marks.out)}` : 'out = fine'}
    </span>
    <button onclick={addToTimeline} disabled={loading || !asset}>+ Timeline</button>
  </div>
  <p class="muted note">{note}</p>
</div>

<style>
  .source { display: flex; flex-direction: column; gap: 6px; height: 100%; min-height: 0; }
  .picker { display: flex; gap: 6px; align-items: center; }
  .picker select { flex: 1; min-width: 0; }
  video, .preview { flex: 1; min-height: 0; width: 100%; background: #000; border-radius: 8px; object-fit: contain; }
  .marks { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .ghost { background: transparent; }
  .note { margin: 0; font-size: 12px; }
</style>
