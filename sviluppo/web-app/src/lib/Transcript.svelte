<script lang="ts">
  import { get, post, put } from './api';

  interface Word {
    id: string;
    raw_text: string;
    corrected_text?: string | null;
    start_ms: number;
    end_ms: number;
    timing_unit: string;
  }
  interface EditState {
    schema_version: number;
    revision: number;
    corrections: Record<string, string>;
    decisions: unknown[];
  }
  interface LongformView {
    transcript_id: string | null;
    total_words: number;
    matched_words: number;
    duration_ms: number;
    text_start_ms: number;
    text_end_ms: number;
    page_start_ms: number;
    page_end_ms: number;
    offset: number;
    words: Word[];
    state: EditState;
    project_updated_at: string;
  }
  interface Proposal {
    word_id: string;
    start_ms: number;
    old: string;
    new: string;
    reason: string;
  }
  interface CorrectionReply {
    proposals: Proposal[];
    model: string;
    provider: string;
    notice: string;
  }
  interface TranslationRow {
    index: number;
    start_ms: number;
    source: string;
    text: string | null;
  }
  interface TranslationView {
    language: string;
    source_language: string;
    engine: string;
    model: string;
    reviewed: boolean;
    rows: TranslationRow[];
  }

  let {
    projectId,
    assetId,
    ontoseek,
    onstatus,
  }: {
    projectId: string;
    assetId: string;
    ontoseek: (ms: number) => void;
    onstatus: (message: string) => void;
  } = $props();

  let view = $state<LongformView | null>(null);
  let offset = $state(0);
  let search = $state('');
  let results = $state<{ total: number; hits: { word_id: string; text: string; start_ms: number; segment_text?: string | null }[] } | null>(null);
  let error = $state('');
  let busy = $state(false);

  // correction review state
  let picked = $state<Word | null>(null);
  let correctionText = $state('');
  let proposals = $state<Proposal[] | null>(null);
  let accepted = $state<Set<string>>(new Set());
  let note = $state('');

  // translation state
  let target = $state('en');
  let translation = $state<TranslationView | null>(null);
  let draft = $state<Record<number, string>>({});
  let translating = $state(false);
  let progress = $state('');

  const time = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(2).padStart(5, '0')}`;

  async function load() {
    error = '';
    try {
      view = await get<LongformView>(
        `/api/v1/projects/${projectId}/longform/${assetId}?offset=${offset}&limit=200&search=${encodeURIComponent(search)}`,
      );
      picked = null;
    } catch (e) {
      error = (e as Error).message;
      view = null;
    }
  }

  async function fullSearch() {
    if (!search.trim()) {
      results = null;
      await load();
      return;
    }
    busy = true;
    try {
      const [fts, lf] = await Promise.all([
        get<{ total: number; hits: NonNullable<typeof results>['hits'] }>(
          `/api/v1/projects/${projectId}/search?q=${encodeURIComponent(search)}&limit=60`,
        ),
        load(),
      ]);
      results = fts;
      void lf;
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
      proposals = null;
      translation = null;
      void load();
    }
  });

  function page(delta: number) {
    if (!view) return;
    offset = Math.max(0, Math.min(Math.max(0, view.matched_words - 1), offset + delta));
    void load();
  }

  async function applyCorrections(entries: [string, string][]): Promise<boolean> {
    if (!view?.transcript_id) return false;
    const state = structuredClone(view.state) as EditState;
    for (const [wordId, text] of entries) state.corrections[wordId] = text;
    try {
      const p = await get<{ updated_at: string }>(`/api/projects/${projectId}`);
      await put(`/api/v1/projects/${projectId}/longform`, {
        expected_updated_at: view.project_updated_at,
        state,
      });
      await load();
      onstatus(entries.length ? `${entries.length} correzioni applicate: il raw ASR resta immutato.` : 'Nessuna correzione selezionata.');
      return true;
    } catch (e) {
      onstatus(`Correzione non applicata: ${(e as Error).message}`);
      return false;
    }
  }

  function pick(word: Word) {
    picked = word;
    correctionText = word.corrected_text ?? word.raw_text;
    ontoseek(word.start_ms);
  }

  async function applyOne() {
    if (!picked) return;
    const text = correctionText.trim();
    if (!text) {
      onstatus('La correzione non può essere vuota: usa Rimuovi per tornare al testo ASR.');
      return;
    }
    if (await applyCorrections([[picked.id, text]])) {
      picked = null;
      correctionText = '';
    }
  }

  async function removeOne() {
    if (!picked || !view) return;
    const state = structuredClone(view.state) as EditState;
    delete state.corrections[picked.id];
    const p = await get<{ updated_at: string }>(`/api/projects/${projectId}`);
    try {
      await put(`/api/v1/projects/${projectId}/longform`, {
        expected_updated_at: view.project_updated_at,
        state,
      });
      await load();
      picked = null;
      onstatus('Correzione rimossa: torni al testo originale dell’ASR.');
    } catch (e) {
      onstatus((e as Error).message);
    }
    void p;
  }

  async function askProposals() {
    if (!view?.transcript_id) return;
    busy = true;
    error = '';
    try {
      const reply = await post<CorrectionReply>(
        `/api/v1/projects/${projectId}/transcripts/${view.transcript_id}/corrections`,
        {
          asset_id: assetId,
          from_ms: view.page_start_ms,
          to_ms: Math.max(view.page_start_ms + 1, view.page_end_ms + 1000),
          note: note.trim() || undefined,
        },
      );
      proposals = reply.proposals;
      accepted = new Set();
      if (!reply.proposals.length) onstatus('Nessuna proposta: il modello non ha correzioni sicure su questa pagina.');
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  function toggleProposal(wordId: string) {
    const next = new Set(accepted);
    if (next.has(wordId)) next.delete(wordId);
    else next.add(wordId);
    accepted = next;
  }

  async function applyAccepted() {
    if (!proposals) return;
    const entries = proposals
      .filter((p) => accepted.has(p.word_id))
      .map((p) => [p.word_id, p.new] as [string, string]);
    if (await applyCorrections(entries)) proposals = null;
  }

  async function translateNext(): Promise<boolean> {
    if (!view?.transcript_id) return false;
    const reply = await post<{ next_index: number | null; translated: number; segments_total: number; notice: string }>(
      `/api/v1/projects/${projectId}/translate`,
      { asset_id: assetId, transcript_id: view.transcript_id, language: target, max_segments: 20 },
    );
    progress = `${reply.translated}/${reply.segments_total} segmenti tradotti`;
    onstatus(reply.notice);
    return reply.next_index !== null && reply.next_index !== undefined;
  }

  async function translateAll() {
    translating = true;
    error = '';
    try {
      for (let round = 0; round < 400; round++) {
        if (!(await translateNext())) break;
      }
      await showTranslation();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      translating = false;
    }
  }

  async function showTranslation() {
    translation = await get<TranslationView>(
      `/api/v1/projects/${projectId}/transcripts/${view?.transcript_id}/translations/${target}`,
    );
    draft = {};
  }

  async function saveTranslation(markReviewed = false) {
    const edits = Object.entries(draft).map(([index, text]) => ({ index: Number(index), text }));
    if (!edits.length && !markReviewed) {
      onstatus('Nessuna modifica da salvare.');
      return;
    }
    try {
      await put(`/api/v1/projects/${projectId}/transcripts/${view?.transcript_id}/translations/${target}`, {
        edits,
        reviewed: markReviewed ? true : undefined,
      });
      await showTranslation();
      onstatus(markReviewed ? 'Traduzione marcata come revisionata.' : 'Traduzione aggiornata.');
    } catch (e) {
      onstatus((e as Error).message);
    }
  }
</script>

<div class="transcript">
  <div class="row">
    <input
      aria-label="Cerca nel parlato"
      bind:value={search}
      placeholder="Cerca nel parlato (indice SQLite, tutte le occorrenze)…"
      onkeydown={(e) => e.key === 'Enter' && fullSearch()}
    />
    <button onclick={fullSearch} disabled={busy}>Cerca</button>
    {#if search && results}
      <button class="ghost" onclick={() => { search = ''; results = null; void load(); }}>Azzera</button>
    {/if}
    {#if view}
      <span class="muted grow">
        {view.total_words} parole · sorgente {time(view.duration_ms)} · parlato {time(view.text_start_ms)}
        → {time(view.text_end_ms)}
      </span>
    {/if}
  </div>
  {#if error}
    <p class="bad">{error}</p>
  {/if}
  {#if results && results.hits.length}
    <details open class="hits">
      <summary>Riscontri dell’indice ({results.total}) · clic per raggiungere il punto</summary>
      {#each results.hits as hit (hit.word_id)}
        <button class="hit" onclick={() => ontoseek(hit.start_ms)}>
          <time>{time(hit.start_ms)}</time>
          <span>{hit.text}</span>
          <small class="muted">{(hit.segment_text ?? '').slice(0, 96)}</small>
        </button>
      {/each}
    </details>
  {/if}
  {#if view}
    <div class="words">
      {#each view.words as word (word.id)}
        <button
          class="word"
          class:picked={picked?.id === word.id}
          class:corrected={!!word.corrected_text}
          onclick={() => pick(word)}
          title={`${time(word.start_ms)} · ${word.raw_text}${word.corrected_text ? ` → ${word.corrected_text}` : ''}`}
        >
          <time>{time(word.start_ms)}</time>
          <span>{word.corrected_text ?? word.raw_text}</span>
        </button>
      {/each}
      {#if !view.words.length}
        <p class="muted">Nessuna parola in questa pagina.</p>
      {/if}
    </div>

    {#if picked}
      <div class="row fix">
        <span class="muted">Correggi «{picked.raw_text}»:</span>
        <input bind:value={correctionText} aria-label="Testo corretto" maxlength="2000" />
        <button onclick={applyOne}>Applica</button>
        {#if picked.corrected_text}
          <button class="ghost" onclick={removeOne}>Rimuovi correzione</button>
        {/if}
        <button class="ghost" onclick={() => (picked = null)}>Chiudi</button>
      </div>
    {/if}

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

    <section class="ia">
      <h3>Revisione assistita</h3>
      <div class="row">
        <input bind:value={note} placeholder="Contesto opzionale per il modello (nomi, termini)…" maxlength="240" />
        <button onclick={askProposals} disabled={busy || !view.transcript_id}>
          {busy ? '…' : 'L’IA propone correzioni (pagina)'}
        </button>
      </div>
      {#if proposals}
        {#if proposals.length}
          <p class="muted">
            Proposte del modello: seleziona quante ne approvi. Nulla è già applicato; il raw ASR non cambia.
          </p>
          <ul class="proposals">
            {#each proposals as proposal (proposal.word_id)}
              <li>
                <label>
                  <input
                    type="checkbox"
                    checked={accepted.has(proposal.word_id)}
                    onchange={() => toggleProposal(proposal.word_id)}
                  />
                  <time>{time(proposal.start_ms)}</time>
                  <s>{proposal.old}</s>
                  <strong>→ {proposal.new}</strong>
                  <small class="muted">{proposal.reason}</small>
                </label>
                <button class="ghost" onclick={() => ontoseek(proposal.start_ms)}>Ascolta</button>
              </li>
            {/each}
          </ul>
          <div class="row">
            <button onclick={applyAccepted} disabled={!accepted.size}>
              Applica selezionate ({accepted.size})
            </button>
            <button class="ghost" onclick={() => (proposals = null)}>Scarta tutte</button>
          </div>
        {:else}
          <p class="muted">Nessuna proposta sicura per questa pagina.</p>
        {/if}
      {/if}
    </section>

    <section class="ia">
      <h3>Traduzione</h3>
      <div class="row">
        <label class="muted">Lingua
          <select bind:value={target}>
            <option value="en">Inglese</option>
            <option value="es">Spagnolo</option>
            <option value="fr">Francese</option>
            <option value="de">Tedesco</option>
            <option value="pt">Portoghese</option>
            <option value="nl">Olandese</option>
            <option value="pl">Polacco</option>
          </select>
        </label>
        <button onclick={translateAll} disabled={translating || !view.transcript_id}>
          {translating ? 'Traduzione in corso…' : 'Traduci tutto col provider'}
        </button>
        {#if view.transcript_id && (translation || progress)}
          <button class="ghost" onclick={showTranslation}>Mostra traduzione</button>
        {/if}
        <span class="muted grow">{progress}</span>
      </div>
      {#if translation}
        <p class="muted">
          {translation.engine} · {translation.model} · da {translation.source_language} → {translation.language}
          · {translation.reviewed ? 'revisionata' : 'non revisionata: trattati come bozza'}
        </p>
        <div class="trows">
          {#each translation.rows as row (row.index)}
            <div class="trow">
              <time>{time(row.start_ms)}</time>
              <p class="src muted">{row.source}</p>
              <textarea
                rows="2"
                placeholder="non tradotto"
                value={draft[row.index] ?? row.text ?? ''}
                oninput={(e) => (draft = { ...draft, [row.index]: e.currentTarget.value })}
              ></textarea>
            </div>
          {/each}
        </div>
        <div class="row">
          <button onclick={() => saveTranslation(false)} disabled={!Object.keys(draft).length}>
            Salva modifiche ({Object.keys(draft).length})
          </button>
          <button class="ghost" onclick={() => saveTranslation(true)}>Segna come revisionata</button>
          <a href="/api/v1/projects/{projectId}/transcripts/{view.transcript_id}/translations/{translation.language}/srt" download>
            Scarica SRT tradotto
          </a>
        </div>
      {/if}
    </section>
  {:else if !error}
    <p class="muted">Trascrizione non ancora caricata per questo media.</p>
  {/if}
</div>

<style>
  .transcript { display: flex; flex-direction: column; gap: 8px; min-height: 0; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .row input { flex: 1; min-width: 200px; }
  .grow { flex: 1; }
  .ghost { background: transparent; }
  .words { display: flex; flex-wrap: wrap; gap: 2px 3px; overflow: auto; max-height: 260px; padding: 4px; }
  .word { border: none; background: transparent; border-radius: 5px; padding: 2px 6px; display: inline-flex; gap: 5px; align-items: baseline; }
  .word:hover { background: rgba(117, 49, 254, 0.25); }
  .word.picked { outline: 2px solid var(--accent); }
  .word.corrected span { color: var(--accent-2); }
  .word time { color: var(--text-dim); font: 10px Consolas, monospace; }
  .fix input { max-width: 340px; }
  .hits { border: 1px solid var(--line); border-radius: 8px; padding: 8px; max-height: 200px; overflow: auto; }
  .hit { display: flex; gap: 8px; align-items: baseline; border: none; background: transparent; width: 100%; text-align: left; }
  .hit:hover { background: rgba(228, 70, 155, 0.18); }
  .hit time { color: var(--accent-3); font: 10px Consolas, monospace; flex: none; }
  .pager { justify-content: space-between; }
  .ia { border: 1px solid var(--line); border-radius: 10px; padding: 10px; display: flex; flex-direction: column; gap: 8px; }
  .ia h3 { margin: 0; font-size: 13px; color: var(--accent-3); text-transform: uppercase; letter-spacing: .08em; }
  .proposals { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; max-height: 240px; overflow: auto; }
  .proposals li { display: flex; justify-content: space-between; gap: 8px; align-items: center; }
  .proposals label { display: flex; gap: 8px; align-items: baseline; flex: 1; }
  .proposals time { color: var(--accent-3); font: 10px Consolas, monospace; }
  .proposals s { color: var(--text-dim); }
  .proposals strong { color: var(--ok); }
  .trows { max-height: 320px; overflow: auto; display: flex; flex-direction: column; gap: 6px; }
  .trow { display: grid; grid-template-columns: 70px 1fr 1fr; gap: 8px; align-items: start; }
  .trow time { color: var(--accent-3); font: 10px Consolas, monospace; padding-top: 6px; }
  .trow .src { margin: 0; }
  .trow textarea { width: 100%; }
</style>
