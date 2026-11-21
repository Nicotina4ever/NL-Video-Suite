<script lang="ts">
  import { get } from './api';

  interface Word {
    id: string;
    raw_text: string;
    corrected_text?: string | null;
    start_ms: number;
    end_ms: number;
    timing_unit: string;
  }
  interface LongformView {
    total_words: number;
    matched_words: number;
    duration_ms: number;
    text_start_ms: number;
    text_end_ms: number;
    page_start_ms: number;
    page_end_ms: number;
    offset: number;
    words: Word[];
  }
  interface SearchHit {
    word_id: string;
    asset_id: string;
    text: string;
    start_ms: number;
    end_ms: number;
    segment_text?: string | null;
  }
  interface SearchResult {
    total: number;
    hits: SearchHit[];
    note: string;
  }

  let {
    projectId,
    assetId,
    ontoseek,
  }: {
    projectId: string;
    assetId: string;
    ontoseek: (ms: number) => void;
  } = $props();

  let view = $state<LongformView | null>(null);
  let offset = $state(0);
  let search = $state('');
  let results = $state<SearchResult | null>(null);
  let error = $state('');
  let busy = $state(false);

  const time = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(2).padStart(5, '0')}`;

  async function load() {
    busy = true;
    error = '';
    try {
      view = await get<LongformView>(
        `/api/v1/projects/${projectId}/longform/${assetId}?offset=${offset}&limit=200&search=${encodeURIComponent(search)}`,
      );
    } catch (e) {
      error = (e as Error).message;
      view = null;
    } finally {
      busy = false;
    }
  }

  async function fullSearch() {
    busy = true;
    error = '';
    try {
      const [fts, lf] = await Promise.all([
        get<SearchResult>(
          `/api/v1/projects/${projectId}/search?q=${encodeURIComponent(search)}&limit=60`,
        ),
        get<LongformView>(
          `/api/v1/projects/${projectId}/longform/${assetId}?offset=${offset}&limit=200&search=${encodeURIComponent(search)}`,
        ),
      ]);
      results = fts;
      view = lf;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  let lastKey = '';

  $effect(() => {
    const key = `${projectId}:${assetId}`;
    if (key !== lastKey) {
      lastKey = key;
      offset = 0;
      search = '';
      results = null;
    }
    if (projectId && assetId) void load();
  });

  function page(delta: number) {
    if (!view) return;
    offset = Math.max(0, Math.min(view.matched_words - 1, offset + delta));
    void load();
  }
</script>

<div class="transcript">
  <div class="row">
    <input
      aria-label="Cerca nel parlato"
      bind:value={search}
      placeholder="Cerca nel parlato (ricerca piena nel parlato del progetto)…"
      onkeydown={(e) => e.key === 'Enter' && fullSearch()}
    />
    <button onclick={fullSearch} disabled={busy || !search.trim()}>Cerca</button>
    {#if results?.total}
      <span class="muted">{results.total} riscontri indicizzati</span>
    {/if}
    {#if view}
      <span class="muted">
        {view.total_words} parole · sorgente {time(view.duration_ms)} · parlato {time(view.text_start_ms)}
        → {time(view.text_end_ms)}
      </span>
    {/if}
  </div>
  {#if error}
    <p class="bad">{error}</p>
  {:else if !view}
    <p class="muted">Trascrizione non ancora caricata per questo media.</p>
  {:else}
    {#if results && results.hits.length}
      <details open class="hits">
        <summary>Riscontri indicizzati (SQLite FTS) · clic per raggiungere il punto</summary>
        {#each results.hits as hit (hit.word_id)}
          <button class="hit" onclick={() => ontoseek(hit.start_ms)}>
            <time>{time(hit.start_ms)}</time>
            <span>{hit.text}</span>
            <small class="muted">{(hit.segment_text ?? '').slice(0, 90)}</small>
          </button>
        {/each}
      </details>
    {/if}
    <div class="words">
      {#each view.words as word (word.id)}
        <button
          class="word"
          onclick={() => ontoseek(word.start_ms)}
          title={time(word.start_ms)}
        >
          <time>{time(word.start_ms)}</time>
          <span>{word.corrected_text ?? word.raw_text}</span>
        </button>
      {/each}
      {#if !view.words.length}
        <p class="muted">Nessuna parola in questa pagina.</p>
      {/if}
    </div>
    <div class="row pager">
      <button class="ghost" onclick={() => page(-200)} disabled={offset === 0 || busy}>← Precedenti</button>
      <span class="muted">
        {view.offset + 1}–{Math.min(view.offset + 200, view.matched_words)} di {view.matched_words}
        {#if view.page_end_ms > view.page_start_ms}
          · {time(view.page_start_ms)} → {time(view.page_end_ms)}
        {/if}
      </span>
      <button class="ghost" onclick={() => page(200)} disabled={offset + 200 >= view.matched_words || busy}>
        Successive →
      </button>
    </div>
  {/if}
</div>

<style>
  .transcript { display: flex; flex-direction: column; gap: 8px; min-height: 0; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .row input { flex: 1; min-width: 220px; }
  .words { display: flex; flex-wrap: wrap; gap: 2px 3px; overflow: auto; min-height: 0; padding: 4px; }
  .word {
    border: none;
    background: transparent;
    border-radius: 5px;
    padding: 2px 6px;
    display: inline-flex;
    gap: 5px;
    align-items: baseline;
  }
  .word:hover { background: rgba(117, 49, 254, 0.25); }
  .word time { color: var(--text-dim); font: 10px Consolas, monospace; }
  .hits { border: 1px solid var(--line); border-radius: 8px; padding: 8px; max-height: 180px; overflow: auto; }
  .hit { display: flex; gap: 8px; align-items: baseline; border: none; background: transparent; width: 100%; text-align: left; }
  .hit:hover { background: rgba(228, 70, 155, 0.18); }
  .hit time { color: var(--accent-3); font: 10px Consolas, monospace; flex: none; }
  .hit span { flex: none; }
  .pager { justify-content: space-between; }
  .ghost { background: transparent; }
</style>
