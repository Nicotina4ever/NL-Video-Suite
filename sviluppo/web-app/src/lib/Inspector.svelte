<script lang="ts">
  import { get, put } from './api';
  import type { Clip, ProjectFull } from './types';

  interface Draft {
    id: string;
    asset_id: string;
    track_id: string;
    label: string;
    start_ms: number;
    trim_in_ms: number;
    duration_ms: number;
    volume: number;
    opacity: number;
    fit: string;
    fade_in_ms: number;
    fade_out_ms: number;
  }

  let {
    project,
    selectedId,
    playheadMs,
    onstatus,
    onupdate,
    onrazor,
  }: {
    project: ProjectFull;
    selectedId: string | null;
    playheadMs: number;
    onstatus: (message: string) => void;
    onupdate: (project: ProjectFull) => void;
    onrazor?: () => void;
  } = $props();

  let draft = $state<Draft | null>(null);
  let saving = $state(false);

  $effect(() => {
    if (!selectedId || !project) {
      draft = null;
      return;
    }
    const clip = project.timeline.clips.find((c) => c.id === selectedId);
    if (!clip) {
      draft = null;
      return;
    }
    draft = {
      id: clip.id,
      asset_id: clip.asset_id,
      track_id: clip.track_id,
      label: clip.label ?? '',
      start_ms: clip.start_ms,
      trim_in_ms: clip.trim_in_ms,
      duration_ms: clip.duration_ms,
      volume: clip.volume ?? 1,
      opacity: clip.opacity ?? 1,
      fit: clip.transform?.fit ?? 'auto',
      fade_in_ms: clip.transform?.fade_in_ms ?? 0,
      fade_out_ms: clip.transform?.fade_out_ms ?? 0,
    };
  });

  const asset = $derived(draft && project?.assets.find((a) => a.id === draft?.asset_id));

  async function commitClips(clips: Clip[], message: string) {
    saving = true;
    try {
      const saved = await put<ProjectFull>(`/api/projects/${project.id}/timeline`, {
        timeline: { ...project.timeline, clips },
        expected_updated_at: project.updated_at,
      });
      onupdate(saved);
      onstatus(message);
    } catch (error) {
      onstatus(`Salvataggio rifiutato: ${(error as Error).message}`);
      try {
        onupdate(await get<ProjectFull>(`/api/projects/${project.id}`));
      } catch {
        /* the next refresh retries */
      }
    } finally {
      saving = false;
    }
  }

  async function saveFields() {
    if (!draft) return;
    const target = project.timeline.clips.find((c) => c.id === draft?.id);
    if (!target) return;
    const clean: Clip = {
      ...target,
      label: draft.label.trim() || null,
      start_ms: Math.max(0, Math.round(draft.start_ms || 0)),
      trim_in_ms: Math.max(0, Math.round(draft.trim_in_ms || 0)),
      duration_ms: Math.max(1, Math.round(draft.duration_ms || 1)),
      volume: Math.min(4, Math.max(0, draft.volume)),
      opacity: Math.min(1, Math.max(0, draft.opacity)),
      transform: {
        ...(target.transform ?? {}),
        fit: draft.fit,
        fade_in_ms: Math.max(0, Math.round(draft.fade_in_ms || 0)),
        fade_out_ms: Math.max(0, Math.round(draft.fade_out_ms || 0)),
      },
    };
    await commitClips(
      project.timeline.clips.map((c) => (c.id === clean.id ? clean : c)),
      'Clip aggiornata.',
    );
  }

  async function removeClip() {
    if (!draft || !project) return;
    if (
      !confirm(
        `Rimuovere la clip «${draft.label || asset?.name || draft.id.slice(0, 6)}» dal montaggio? I file del progetto restano intatti.`,
      )
    )
      return;
    await commitClips(
      project.timeline.clips.filter((c) => c.id !== draft?.id),
      'Clip rimossa dal montaggio (nessun file eliminato).',
    );
  }

  function nudgeDuration(ms: number) {
    if (draft) draft.duration_ms = Math.max(1, (draft.duration_ms || 1) + ms);
  }
</script>

{#if draft}
  <div class="inspector">
    <h3>Ispettore clip</h3>
    <p class="muted">
      {asset?.name ?? draft.asset_id} · traccia
      {project.timeline.tracks.find((t) => t.id === draft?.track_id)?.name ?? '?'}
    </p>
    <label class="row"><span class="muted">Etichetta</span><input bind:value={draft.label} maxlength="200" /></label>
    <div class="grid">
      <label><span>Inizio timeline (ms)</span><input type="number" min="0" step="1" bind:value={draft.start_ms} /></label>
      <label><span>Ingresso sorgente (ms)</span><input type="number" min="0" step="1" bind:value={draft.trim_in_ms} /></label>
      <label><span>Durata (ms)</span><input type="number" min="1" step="1" bind:value={draft.duration_ms} /></label>
      <label><span>Volume (0–4)</span><input type="number" min="0" max="4" step="0.05" bind:value={draft.volume} /></label>
      <label><span>Opacità (0–1)</span><input type="number" min="0" max="1" step="0.05" bind:value={draft.opacity} /></label>
      <label><span>Fade ingresso (ms)</span><input type="number" min="0" step="10" bind:value={draft.fade_in_ms} /></label>
      <label><span>Fade uscita (ms)</span><input type="number" min="0" step="10" bind:value={draft.fade_out_ms} /></label>
      <label><span>Inquadratura</span><select bind:value={draft.fit}>
          <option value="auto">Automatica</option>
          <option value="contain">Contieni</option>
          <option value="cover">Riempi (crop)</option>
          <option value="blur">Adatta · sfondo sfocato</option>
        </select>
      </label>
    </div>
    <div class="actions">
      <button class="primary" onclick={saveFields} disabled={saving}>Salva clip</button>
      <button class="ghost" onclick={() => nudgeDuration(-1000)} disabled={saving}>−1 s</button>
      <button class="ghost" onclick={() => nudgeDuration(1000)} disabled={saving}>+1 s</button>
      {#if onrazor}
        <button class="ghost" onclick={onrazor} disabled={saving}>
          Dividi alla playhead ({Math.round(playheadMs)} ms)
        </button>
      {/if}
      <button class="ghost danger" onclick={removeClip} disabled={saving}>Rimuovi dal montaggio</button>
    </div>
    <p class="muted">
      Questi campi scrivono la timeline con la stessa guardia di revisione della canvas; il rifilo
      ripple/roll multi-clip arriva con l'algebra di montaggio (goal tracker).
    </p>
  </div>
{:else}
  <p class="muted pad">Clicca una clip nella timeline per ispezionarla e modificarla.</p>
{/if}

<style>
  .inspector { display: flex; flex-direction: column; gap: 8px; padding: 4px; overflow: auto; }
  .inspector h3 { margin: 0; }
  .row { display: flex; gap: 8px; align-items: center; }
  .row input { flex: 1; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 8px; }
  .grid label { display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-dim); }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .danger { border-color: var(--bad); color: var(--bad); }
  .pad { padding: 8px; }
  .ghost { background: transparent; }
</style>
