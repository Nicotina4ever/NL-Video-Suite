<script lang="ts">
  import { get, post, put } from './api';
  import type { ProjectFull } from './types';

  interface Range {
    start_ms: number;
    end_ms: number;
  }
  interface Candidate {
    id: string;
    asset_id: string;
    title: string;
    ranges: Range[];
    evidence: string;
    scores: Record<string, number>;
    warnings: string[];
    approved: boolean;
    producer: string;
    duplicate_of?: string | null;
  }
  interface ShortsView {
    schema_version: number;
    auto_approval: boolean;
    project_updated_at: string;
    state: { revision: number; candidates: Candidate[]; weights: Record<string, number> };
  }

  let {
    project,
    onstatus,
    onupdate,
  }: {
    project: ProjectFull;
    onstatus: (message: string) => void;
    onupdate: (project: ProjectFull) => void;
  } = $props();

  let view = $state<ShortsView | null>(null);
  let busy = $state(false);
  let error = $state('');
  let minSeconds = $state(12);
  let maxSeconds = $state(60);
  let manual = $state({ start: 0, end: 30, title: '' });

  const time = (ms: number) =>
    `${Math.floor(ms / 60000)}:${((ms / 1000) % 60).toFixed(2).padStart(5, '0')}`;

  const videoAsset = $derived(project?.assets.find((a) => a.kind === 'video') ?? project?.assets[0] ?? null);

  async function load() {
    try {
      view = await get<ShortsView>(`/api/v1/projects/${project.id}/shorts`);
      error = '';
    } catch (e) {
      error = (e as Error).message;
      view = null;
    }
  }

  $effect(() => {
    if (project?.id) void load();
  });

  async function suggest() {
    if (!videoAsset || !view) return;
    busy = true;
    try {
      const fresh = await post<ProjectFull>(`/api/v1/projects/${project.id}/shorts/suggest`, {
        asset_id: videoAsset.id,
        expected_updated_at: view.project_updated_at,
        analysis_job_id: null,
        min_seconds: minSeconds,
        max_seconds: maxSeconds,
      });
      onupdate(fresh);
      await load();
      onstatus(`Candidati rigenerati dalle decisioni e dai capitoli approvati: ${view?.state.candidates.length ?? 0}.`);
    } catch (e) {
      onstatus((e as Error).message);
    } finally {
      busy = false;
    }
  }

  async function saveState(mutate: (candidates: Candidate[]) => Candidate[], message: string) {
    if (!view) return;
    busy = true;
    try {
      const fresh = await put<ProjectFull>(`/api/v1/projects/${project.id}/shorts`, {
        expected_updated_at: view.project_updated_at,
        state: { ...view.state, candidates: mutate(view.state.candidates) },
      });
      onupdate(fresh);
      await load();
      onstatus(message);
    } catch (e) {
      onstatus(`Salvataggio rifiutato: ${(e as Error).message}`);
      try {
        onupdate(await get<ProjectFull>(`/api/projects/${project.id}`));
      } catch {
        /* retry on refresh */
      }
    } finally {
      busy = false;
    }
  }

  async function toggleApproval(candidate: Candidate) {
    await saveState(
      (list) => list.map((c) => (c.id === candidate.id ? { ...c, approved: !c.approved } : c)),
      candidate.approved ? 'Candidato non approvato.' : 'Candidato approvato (ora partecipa all’assemblaggio).',
    );
  }

  async function addManual() {
    if (!videoAsset || !view) return;
    busy = true;
    try {
      const fresh = await post<ProjectFull>(`/api/v1/projects/${project.id}/shorts/manual`, {
        asset_id: videoAsset.id,
        expected_updated_at: view.project_updated_at,
        start_ms: Math.round(manual.start * 1000),
        end_ms: Math.round(manual.end * 1000),
        title: manual.title.trim() || 'Scelta manuale',
      });
      onupdate(fresh);
      await load();
      onstatus('Candidato manuale aggiunto, non approvato finché non lo selezioni.');
    } catch (e) {
      onstatus((e as Error).message);
    } finally {
      busy = false;
    }
  }

  async function applyApproved() {
    if (!videoAsset || !view) return;
    const approved = view.state.candidates.filter((c) => c.approved);
    if (!approved.length) {
      onstatus('Nessun candidato approvato: nulla da assemblare.');
      return;
    }
    if (
      !confirm(
        `Assemblare ${approved.length} candidati approvati sostituisce il montaggio attuale e porta il progetto a 9:16. Gli altri clip verranno rimossi dal montaggio (i file restano). Procedere?`,
      )
    )
      return;
    busy = true;
    try {
      const fresh = await post<ProjectFull>(`/api/v1/projects/${project.id}/shorts/apply`, {
        asset_id: videoAsset.id,
        expected_updated_at: view.project_updated_at,
        analysis_job_id: null,
        min_seconds: null,
        max_seconds: null,
      });
      onupdate(fresh);
      onstatus('Assemblaggio 9:16 applicato: verifica nel Montaggio e regola le riproduzioni.'+
        ' Ogni clip resta collegato alla sua origine nel candidato.');
    } catch (e) {
      onstatus((e as Error).message);
    } finally {
      busy = false;
    }
  }

  function seek(ms: number) {
    // reuse the shell playhead through a custom event so Shorts stays decoupled
    window.dispatchEvent(new CustomEvent('nl-seek', { detail: ms }));
  }
</script>

<div class="shorts">
  {#if !videoAsset}
    <p class="muted">Importa un video nel progetto per preparare gli shorts.</p>
  {:else if error}
    <p class="bad">{error}</p>
    <button onclick={load}>Riprova</button>
  {:else if view}
    <div class="row">
      <label class="muted">Min s <input class="num" type="number" min="3" max="180" bind:value={minSeconds} /></label>
      <label class="muted">Max s <input class="num" type="number" min="3" max="180" bind:value={maxSeconds} /></label>
      <button onclick={suggest} disabled={busy || !videoAsset}>Riproponi candidati</button>
      <div class="spacer"></div>
      <button class="primary" onclick={applyApproved} disabled={busy || !view.state.candidates.some((c) => c.approved)}>
        Assembla approvati · 9:16
      </button>
    </div>
    <p class="muted">
      {view.state.candidates.length} candidati · ponderazioni {Object.keys(view.state.weights).length}
      · {view.auto_approval ? 'approvazione automatica attiva' : 'nessuna approvazione implicita'}
    </p>
    <div class="cards">
      {#each view.state.candidates as candidate (candidate.id)}
        <article class="card">
          <header>
            <label class="approve">
              <input type="checkbox" checked={candidate.approved} onchange={() => toggleApproval(candidate)} />
              <strong>{candidate.title || 'Senza titolo'}</strong>
            </label>
            <span class="muted">{candidate.producer === 'manual' ? 'scelta manuale' : 'da trascrizione'}</span>
          </header>
          <div class="ranges">
            {#each candidate.ranges as range (range.start_ms)}
              <button class="range" onclick={() => seek(range.start_ms)}>
                {time(range.start_ms)} → {time(range.end_ms)}
              </button>
            {/each}
          </div>
          <p class="evidence">{candidate.evidence.slice(0, 240)}{candidate.evidence.length > 240 ? '…' : ''}</p>
          {#if candidate.duplicate_of}
            <p class="bad">Possibile ripetizione: verifica se è una ripresa intenzionale.</p>
          {/if}
          {#each candidate.warnings as warning (warning)}
            <p class="warn">{warning}</p>
          {/each}
        </article>
      {:else}
        <p class="muted">Nessun candidato: proponi dagli highlight approvati o aggiungi una scelta manuale.</p>
      {/each}
    </div>
    <div class="row manual">
      <span class="muted">Scelta manuale dalla sorgente:</span>
      <input class="num" type="number" min="0" step="0.1" bind:value={manual.start} aria-label="Inizio secondi" />
      <input class="num" type="number" min="0" step="0.1" bind:value={manual.end} aria-label="Fine secondi" />
      <input bind:value={manual.title} placeholder="Titolo" maxlength="180" />
      <button onclick={addManual} disabled={busy || manual.end <= manual.start}>+ Candidato</button>
    </div>
  {/if}
</div>

<style>
  .shorts { display: flex; flex-direction: column; gap: 8px; padding: 4px; min-height: 0; overflow: auto; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .spacer { flex: 1; }
  .num { width: 90px; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 8px; }
  .card { border: 1px solid var(--line); border-radius: 10px; padding: 10px; display: flex; flex-direction: column; gap: 6px; }
  .card header { display: flex; justify-content: space-between; gap: 8px; align-items: baseline; }
  .approve { display: flex; gap: 6px; align-items: baseline; cursor: pointer; }
  .ranges { display: flex; gap: 6px; flex-wrap: wrap; }
  .range { font: 11px Consolas, monospace; background: transparent; }
  .evidence { color: var(--text-dim); font-size: 12px; margin: 0; }
  .warn { color: var(--warn); font-size: 12px; margin: 0; }
  .manual input { flex: 0 1 160px; }
</style>
