<script lang="ts">
  import { get, post, put } from './api';
  import type { ProjectFull } from './types';

  interface Style {
    font: string;
    size: number;
    color: string;
    highlight: string;
    outline: string;
    outline_size: number;
    x: number;
    y: number;
    max_chars: number;
    mode: string;
  }
  interface Word {
    id: string;
    text: string;
    source: { start_ms: number; end_ms: number };
  }
  interface Cue {
    id: string;
    asset_id: string;
    transcript_id: string;
    source: { start_ms: number; end_ms: number };
    raw_text: string;
    text: string;
    language: string;
    words: Word[];
    align_status: string;
    approved: boolean;
    style: Style | null;
    alignment?: unknown;
    offset_ms: number;
    translation_of?: string | null;
  }
  interface SubtitleDoc {
    schema_version: number;
    revision: number;
    enabled: boolean;
    language: string;
    style: Style;
    units: Cue[];
  }
  interface SubtitleView {
    schema_version: number;
    project_updated_at: string;
    document: SubtitleDoc;
    projection: unknown[];
    fonts: string;
    font_missing: boolean;
    renderer: string;
  }

  let { project, onstatus }: { project: ProjectFull; onstatus: (message: string) => void } = $props();

  let view = $state<SubtitleView | null>(null);
  let doc = $state<SubtitleDoc>({
    schema_version: 1,
    revision: 0,
    enabled: false,
    language: 'it',
    style: {
      font: 'Arial',
      size: 0.048,
      color: '#FFFFFF',
      highlight: '#E5469C',
      outline: '#101018',
      outline_size: 3,
      x: 0.5,
      y: 0.82,
      max_chars: 32,
      mode: 'plain',
    },
    units: [],
  });
  let dirty = $state(false);
  let loading = $state(false);
  let busy = $state(false);
  let selectedId = $state<string | null>(null);
  let manual = $state({ start: 0, end: 2, text: '', language: 'it' });
  let manualAsset = $state('');

  const mediaAssets = $derived(project?.assets.filter((a) => ['video', 'audio'].includes(a.kind)) ?? []);
  const units = $derived(doc.units);
  const selected = $derived(units.find((u) => u.id === selectedId) ?? null);

  const clock = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(2).padStart(5, '0')}`;
  const sec = (ms: number) => (ms / 1000).toFixed(2);

  function adopt(next: SubtitleView) {
    view = next;
    doc = JSON.parse(JSON.stringify(next.document)) as SubtitleDoc;
    dirty = false;
    if (!manualAsset) manualAsset = mediaAssets[0]?.id ?? '';
    if (!selectedId || !doc.units.some((u) => u.id === selectedId)) {
      selectedId = doc.units[0]?.id ?? null;
    }
  }

  async function load() {
    if (!project) return;
    loading = true;
    try {
      adopt(await get<SubtitleView>(`/api/v1/projects/${project.id}/subtitles`));
    } catch (error) {
      onstatus((error as Error).message);
    } finally {
      loading = false;
    }
  }

  let lastProject = '';
  $effect(() => {
    const id = project?.id ?? '';
    if (!id || id === lastProject) return;
    lastProject = id;
    void load();
  });

  function touch() {
    dirty = true;
  }

  async function save() {
    if (!view || busy) return;
    busy = true;
    try {
      await put(`/api/v1/projects/${project.id}/subtitles`, {
        expected_updated_at: view.project_updated_at,
        document: doc,
      });
      onstatus('Sottotitoli salvati.');
      await load();
    } catch (error) {
      onstatus((error as Error).message);
    } finally {
      busy = false;
    }
  }

  async function syncFromSpeech() {
    if (!view || busy) return;
    busy = true;
    try {
      await post(`/api/v1/projects/${project.id}/subtitles/sync`, {
        expected_updated_at: view.project_updated_at,
      });
      onstatus('Cue sincronizzati dal parlato approvato.');
      await load();
    } catch (error) {
      onstatus((error as Error).message);
    } finally {
      busy = false;
    }
  }

  async function addManual() {
    if (!view || busy) return;
    if (!manualAsset) {
      onstatus('Serve un media audio/video nel progetto.');
      return;
    }
    if (!manual.text.trim()) {
      onstatus('Scrivi il testo del cue manuale.');
      return;
    }
    busy = true;
    try {
      await post(`/api/v1/projects/${project.id}/subtitles/manual`, {
        expected_updated_at: view.project_updated_at,
        asset_id: manualAsset,
        source: { start_ms: Math.round(manual.start * 1000), end_ms: Math.round(manual.end * 1000) },
        text: manual.text.trim(),
        language: manual.language,
      });
      manual = { ...manual, text: '' };
      onstatus('Cue manuale aggiunto.');
      await load();
    } catch (error) {
      onstatus((error as Error).message);
    } finally {
      busy = false;
    }
  }

  async function structure(operation: 'split' | 'merge') {
    if (!view || !selected || busy) return;
    const index = units.findIndex((u) => u.id === selected!.id);
    const next = operation === 'merge' ? units[index + 1] : undefined;
    if (operation === 'merge' && !next) {
      onstatus('Nessun cue successivo da unire.');
      return;
    }
    busy = true;
    try {
      await post(`/api/v1/projects/${project.id}/subtitles/structure`, {
        expected_updated_at: view.project_updated_at,
        operation,
        unit_id: selected.id,
        at_source_ms: operation === 'split' ? selected.source.start_ms + Math.floor((selected.source.end_ms - selected.source.start_ms) / 2) : undefined,
        next_unit_id: next?.id,
      });
      onstatus(operation === 'split' ? 'Cue diviso a metà.' : 'Cue unito al successivo.');
      await load();
    } catch (error) {
      onstatus((error as Error).message);
    } finally {
      busy = false;
    }
  }

  function previewAt() {
    if (!selected) return;
    window.open(
      `/api/v1/projects/${project.id}/subtitles/frame?at_ms=${selected.source.start_ms}`,
      '_blank',
      'noopener',
    );
  }

  const exportBase = $derived(`/api/v1/projects/${project.id}/subtitles/export`);
</script>

<div class="subs">
  <div class="head">
    <label class="toggle">
      <input type="checkbox" bind:checked={doc.enabled} onchange={touch} /> Sottotitoli attivi
    </label>
    <label>Lingua
      <select bind:value={doc.language} onchange={touch}>
        <option value="it">Italiano</option>
        <option value="en">Inglese</option>
        <option value="es">Spagnolo</option>
        <option value="unknown">Sconosciuta</option>
        <option value="auto">Auto</option>
      </select>
    </label>
    <span class="muted">{units.length} cue · rev {doc.revision} · {view?.renderer ?? ''}{view?.font_missing ? ' · font mancante (fallback Arial)' : ''}</span>
    <div class="spacer"></div>
    <button class="ghost" onclick={load} disabled={busy || loading}>Ricarica</button>
    <button class="ghost" onclick={syncFromSpeech} disabled={busy || !view}>Sincronizza dal parlato</button>
    <button onclick={save} disabled={!dirty || busy}>{dirty ? 'Salva sottotitoli' : 'Salvato'}</button>
  </div>

  <div class="body">
    <div class="list">
      {#if !units.length}
        <p class="muted empty">
          Nessun cue. Trascrivi il media e premi «Sincronizza dal parlato», oppure aggiungi un cue manuale.
        </p>
      {/if}
      {#each units as cue (cue.id)}
        <button
          class="cue"
          class:selected={cue.id === selectedId}
          class:approved={cue.approved}
          onclick={() => (selectedId = cue.id)}
        >
          <span class="tc">{clock(cue.source.start_ms)} → {clock(cue.source.end_ms)}</span>
          <span class="text">{cue.text}</span>
          <span class="meta">
            {cue.language} · {cue.align_status}
            {#if cue.approved}· ✓{/if}
            {#if cue.translation_of}· trad.{/if}
          </span>
        </button>
      {/each}
    </div>

    <aside class="editor">
      {#if selected}
        <h3>Cue selezionato</h3>
        <label>Testo
          <textarea rows="3" bind:value={selected.text} oninput={touch} maxlength="4000"></textarea>
        </label>
        <div class="row">
          <label>Inizio (s)
            <input type="number" min="0" step="0.01" value={sec(selected.source.start_ms)}
              oninput={(e) => { selected!.source.start_ms = Math.round(Number((e.target as HTMLInputElement).value) * 1000); touch(); }} />
          </label>
          <label>Fine (s)
            <input type="number" min="0" step="0.01" value={sec(selected.source.end_ms)}
              oninput={(e) => { selected!.source.end_ms = Math.round(Number((e.target as HTMLInputElement).value) * 1000); touch(); }} />
          </label>
        </div>
        <label class="toggle">
          <input type="checkbox" bind:checked={selected.approved} onchange={touch} /> Approvato per l’export
        </label>
        <div class="row">
          <button class="ghost" onclick={() => structure('split')} disabled={busy}>Dividi a metà</button>
          <button class="ghost" onclick={() => structure('merge')} disabled={busy}>Unisci al successivo</button>
          <button class="ghost" onclick={previewAt} disabled={busy}>Anteprima frame</button>
        </div>
        {#if selected.words.length}
          <p class="muted small">{selected.words.length} parole allineate · offset {selected.offset_ms} ms</p>
        {/if}
      {:else}
        <p class="muted">Seleziona un cue nell’elenco.</p>
      {/if}

      <h3>Stile</h3>
      {#if doc}
        <div class="row">
          <label>Font<input bind:value={doc.style.font} oninput={touch} maxlength="80" /></label>
          <label>Dimensione<input type="number" min="0.01" max="0.15" step="0.001" bind:value={doc.style.size} oninput={touch} /></label>
        </div>
        <div class="row">
          <label>Testo<input type="color" bind:value={doc.style.color} oninput={touch} /></label>
          <label>Evidenzia<input type="color" bind:value={doc.style.highlight} oninput={touch} /></label>
          <label>Bordo<input type="color" bind:value={doc.style.outline} oninput={touch} /></label>
        </div>
        <div class="row">
          <label>Spessore bordo<input type="number" min="0" max="12" step="0.5" bind:value={doc.style.outline_size} oninput={touch} /></label>
          <label>Max caratteri<input type="number" min="8" max="120" step="1" bind:value={doc.style.max_chars} oninput={touch} /></label>
          <label>Modo
            <select bind:value={doc.style.mode} onchange={touch}>
              <option value="plain">Statico</option>
              <option value="word">Parola per parola</option>
            </select>
          </label>
        </div>
        <div class="row">
          <label>X (0–1)<input type="number" min="0" max="1" step="0.01" bind:value={doc.style.x} oninput={touch} /></label>
          <label>Y (0–1)<input type="number" min="0" max="1" step="0.01" bind:value={doc.style.y} oninput={touch} /></label>
        </div>
      {/if}

      <h3>Nuovo cue manuale</h3>
      <label>Media
        <select bind:value={manualAsset}>
          {#each mediaAssets as a (a.id)}
            <option value={a.id}>{a.name}</option>
          {/each}
        </select>
      </label>
      <label>Testo
        <textarea rows="2" bind:value={manual.text} maxlength="4000" placeholder="Testo del cue"></textarea>
      </label>
      <div class="row">
        <label>Da (s)<input type="number" min="0" step="0.01" bind:value={manual.start} /></label>
        <label>A (s)<input type="number" min="0" step="0.01" bind:value={manual.end} /></label>
        <label>Lingua
          <select bind:value={manual.language}>
            <option value="it">IT</option>
            <option value="en">EN</option>
            <option value="es">ES</option>
          </select>
        </label>
      </div>
      <button onclick={addManual} disabled={busy}>+ Aggiungi cue</button>

      <h3>Esporta</h3>
      <p class="muted small">L’export include solo i cue approvati.</p>
      <div class="row exports">
        <a href="{exportBase}/srt" download>SRT</a>
        <a href="{exportBase}/vtt" download>VTT</a>
        <a href="{exportBase}/ass" download>ASS</a>
        <a href="{exportBase}/json" download>JSON</a>
      </div>
    </aside>
  </div>
</div>

<style>
  .subs { display: flex; flex-direction: column; gap: 8px; height: 100%; min-height: 0; }
  .head { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .head label { display: flex; gap: 4px; align-items: center; font-size: 12px; }
  .toggle { width: auto; }
  .spacer { flex: 1; }
  .body { flex: 1; display: grid; grid-template-columns: 1fr 340px; gap: 10px; min-height: 0; }
  .list { overflow: auto; display: flex; flex-direction: column; gap: 4px; }
  .empty { padding: 12px; }
  .cue {
    display: grid; grid-template-columns: 118px 1fr; grid-template-rows: auto auto; gap: 0 8px;
    text-align: left; background: var(--bg-raised); border: 1px solid var(--line);
    border-radius: 6px; padding: 6px 8px;
  }
  .cue.selected { border-color: var(--accent); }
  .cue.approved .text { color: var(--text); }
  .cue .text { font-size: 13px; }
  .cue .meta { grid-column: 2; font-size: 10px; color: var(--text-dim); }
  .tc { font: 11px Consolas, monospace; color: var(--accent-3); grid-row: 1 / span 2; }
  .editor { overflow: auto; border-left: 1px solid var(--line); padding-left: 10px; display: flex; flex-direction: column; gap: 6px; }
  .editor h3 { margin: 4px 0 0; font-size: 12px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-dim); }
  .row { display: flex; gap: 6px; flex-wrap: wrap; }
  .row label { flex: 1; min-width: 96px; }
  .small { font-size: 11px; }
  .exports a { color: var(--accent-3); font-size: 12px; }
  textarea { resize: vertical; }
</style>
