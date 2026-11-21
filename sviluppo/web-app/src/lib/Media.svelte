<script lang="ts">
  import { post } from './api';
  import { playbackUrl } from './media';
  import type { AssetInfo, ProjectFull } from './types';

  let {
    project,
    onstatus,
    onadded,
    onopen,
  }: {
    project: ProjectFull;
    onstatus: (message: string) => void;
    onadded: () => void | Promise<void>;
    onopen: (assetId: string) => void;
  } = $props();

  let selectedId = $state<string | null>(null);
  let preview = $state<HTMLVideoElement | HTMLAudioElement | null>(null);
  let previewUrl = $state('');
  let previewKind = $state('');
  let busy = $state(false);

  const assets = $derived(project?.assets ?? []);
  const selected = $derived(assets.find((a) => a.id === selectedId) ?? null);
  const mediaAssets = $derived(assets.filter((a) => ['video', 'audio', 'image'].includes(a.kind)));

  const clock = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(1).padStart(4, '0')}`;

  async function choose(asset: AssetInfo) {
    selectedId = asset.id;
    previewKind = asset.kind;
    previewUrl = '';
    if (asset.kind === 'image') return;
    try {
      previewUrl = await playbackUrl(project.id, asset.id);
    } catch (error) {
      onstatus((error as Error).message);
    }
  }

  async function addSelected() {
    const asset = selected;
    if (!asset || busy) return;
    busy = true;
    try {
      const clip = await post<{ start_ms: number }>(`/api/projects/${project.id}/timeline/clips`, {
        asset_id: asset.id,
        label: asset.name,
        duration_ms: asset.media?.duration_ms,
      });
      await onadded();
      onstatus(`«${asset.name}» aggiunta alla timeline al minuto ${clock(clip.start_ms)}.`);
    } catch (error) {
      onstatus((error as Error).message);
    } finally {
      busy = false;
    }
  }

  function onDragStart(event: DragEvent, asset: AssetInfo) {
    event.dataTransfer?.setData('text/nl-asset-id', asset.id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
  }
</script>

<div class="media">
  <div class="grid" role="group" aria-label="Media del progetto">
    {#each assets as asset (asset.id)}
      <button
        type="button"
        class="card"
        class:selected={selectedId === asset.id}
        draggable="true"
        ondragstart={(e) => onDragStart(e, asset)}
        onclick={() => choose(asset)}
        title="Trascina sulla timeline per aggiungere"
      >
        <span class="thumb">
          {#if ['video', 'image'].includes(asset.kind)}
            <img src="/api/projects/{project.id}/assets/{asset.id}/thumbnail" alt="" loading="lazy" />
          {:else}
            <span class="audio-glyph" aria-hidden="true">♪</span>
          {/if}
        </span>
        <span class="name">{asset.name}</span>
        <span class="meta">
          <span class="kind">{asset.kind}</span>
          {#if asset.media?.duration_ms}<span>{clock(asset.media.duration_ms)}</span>{/if}
        </span>
      </button>
    {:else}
      <p class="muted empty">Nessun media: importa un file dalla scheda «Classica → Materiali».</p>
    {/each}
  </div>

  <aside class="detail">
    <h3>{selected?.name ?? 'Scegli un media'}</h3>
    {#if selected?.kind === 'image'}
      <img class="still" src="/api/projects/{project.id}/assets/{selected.id}/thumbnail" alt={selected.name} />
    {:else if selected}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={preview}
        class="still"
        controls
        playsinline
        preload="metadata"
        src={previewUrl}
        onloadeddata={() => onstatus(`Sorgente pronta: ${selected.name}`)}
      ></video>
    {/if}
    <div class="actions">
      <button onclick={addSelected} disabled={!selected || busy}>+ Timeline</button>
      <button class="ghost" onclick={() => selected && onopen(selected.id)} disabled={!selected}>
        Apri in Sorgente
      </button>
    </div>
    <p class="muted hint">
      Trascina una scheda sulla timeline (o usa «+ Timeline» per accodarla).
      I file originali restano intatti: la suite lavora su copie interne.
    </p>
  </aside>
</div>

<style>
  .media { display: grid; grid-template-columns: 1fr 300px; gap: 10px; flex: 1; min-height: 0; }
  .grid {
    display: grid; grid-template-columns: repeat(auto-fill, minmax(148px, 1fr));
    gap: 8px; overflow: auto; align-content: start; padding-right: 2px;
  }
  .card {
    display: flex; flex-direction: column; gap: 4px; padding: 6px; text-align: left;
    background: var(--bg-raised); border: 1px solid var(--line); border-radius: 8px; cursor: grab;
  }
  .card:active { cursor: grabbing; }
  .card.selected { border-color: var(--accent-2); }
  .thumb { aspect-ratio: 16 / 9; border-radius: 6px; overflow: hidden; background: #000; display: grid; place-items: center; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; }
  .audio-glyph { color: var(--accent-3); font-size: 26px; }
  .name { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-dim); }
  .kind { text-transform: uppercase; font-size: 9px; letter-spacing: .06em; }
  .empty { grid-column: 1 / -1; }
  .detail { display: flex; flex-direction: column; gap: 8px; border-left: 1px solid var(--line); padding-left: 10px; min-height: 0; }
  .detail h3 { margin: 0; font-size: 13px; }
  .still { width: 100%; border-radius: 8px; background: #000; object-fit: contain; max-height: 220px; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .ghost { background: transparent; }
  .hint { font-size: 11px; margin: 0; }
</style>
