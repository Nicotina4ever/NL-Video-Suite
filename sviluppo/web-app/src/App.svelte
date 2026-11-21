<script lang="ts">
  import { get, post, put, api, type ProjectSummary } from './lib/api';
  import {
    commands,
    formatShortcut,
    WORKSPACES,
    loadLayout,
    saveLayout,
    currentProjectId,
    rememberProject,
    pickProjectSummary,
  } from './lib/shell';
  import Splitter from './lib/Splitter.svelte';

  interface Health {
    version: string;
    ffmpeg: boolean;
    comfyui: boolean;
    ollama: boolean;
    data_warnings: string[];
  }

  let health = $state<Health | null>(null);
  let projects = $state<ProjectSummary[]>([]);
  let project = $state<ProjectSummary | null>(null);
  let layout = $state(loadLayout());
  let status = $state('Pronto.');
  let paletteOpen = $state(false);
  let paletteQuery = $state('');
  let renameValue = $state('');

  $effect(() => saveLayout(layout));

  async function refreshHealth() {
    try {
      health = await get<Health>('/api/health');
    } catch {
      health = null;
      status = 'Backend non raggiungibile: avvia la suite o il proxy di sviluppo.';
    }
  }

  async function refreshProjects() {
    try {
      projects = await get<ProjectSummary[]>('/api/projects');
      project = pickProjectSummary(projects, currentProjectId());
    } catch (error) {
      status = (error as Error).message;
    }
  }

  function selectProject(id: string) {
    rememberProject(id || null);
    project = pickProjectSummary(projects, id || null);
    status = project ? `Progetto aperto: ${project.title}.` : 'Nessun progetto selezionato.';
  }

  async function renameProject() {
    if (!project) return;
    const title = renameValue.trim();
    if (!title || title === project.title) return;
    try {
      const full = await get<{ updated_at: string }>(`/api/projects/${project.id}`);
      const result = await put<{ folder_renamed: string | null }>(`/api/projects/${project.title ? project.id : project.id}/title`, {
        title,
        expected_updated_at: full.updated_at,
      });
      await refreshProjects();
      status = result.folder_renamed
        ? `Progetto rinominato; cartella ora «${result.folder_renamed}».`
        : 'Titolo aggiornato; la cartella è rimasta stabile.';
    } catch (error) {
      status = (error as Error).message;
    }
  }

  const filtered = $derived(
    commands
      .list()
      .filter((c) => c.label.toLowerCase().includes(paletteQuery.toLowerCase()))
      .slice(0, 12),
  );

  commands.register({
    id: 'project.refresh',
    label: 'Aggiorna elenco progetti',
    shortcut: 'Ctrl+Shift+R',
    group: 'Progetti',
    run: async () => {
      await refreshProjects();
      status = 'Elenco progetti aggiornato.';
    },
    enabled: () => true,
  });
  commands.register({
    id: 'workspace.materials',
    label: 'Vai a Progetti e materiali',
    shortcut: 'Alt+1',
    group: 'Workspace',
    run: () => { layout.workspace = 'materials'; },
    enabled: () => true,
  });
  commands.register({
    id: 'workspace.edit',
    label: 'Vai a Montaggio',
    shortcut: 'Alt+2',
    group: 'Workspace',
    run: () => { layout.workspace = 'edit'; },
    enabled: () => true,
  });
  commands.register({
    id: 'workspace.social',
    label: 'Vai a Shorts e Social',
    shortcut: 'Alt+3',
    group: 'Workspace',
    run: () => { layout.workspace = 'social'; },
    enabled: () => true,
  });
  commands.register({
    id: 'workspace.story',
    label: 'Vai a Storie e Regia AI',
    shortcut: 'Alt+4',
    group: 'Workspace',
    run: () => { layout.workspace = 'story'; },
    enabled: () => true,
  });
  commands.register({
    id: 'view.theme',
    label: 'Passa al tema chiaro/scuro',
    shortcut: 'Ctrl+Shift+D',
    group: 'Vista',
    run: () => {
      const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
      document.documentElement.dataset.theme = next;
      localStorage.setItem('nl-theme', next);
      status = `Tema: ${next === 'dark' ? 'scuro' : 'chiaro'}.`;
    },
    enabled: () => true,
  });
  commands.register({
    id: 'palette.open',
    label: 'Apri riga di comando…',
    shortcut: 'Ctrl+Shift+P',
    group: 'Shell',
    run: () => {
      paletteQuery = '';
      paletteOpen = true;
    },
    enabled: () => true,
  });

  function onWindowKeydown(event: KeyboardEvent) {
    if ((event.target as HTMLElement)?.closest('input,textarea,select')) return;
    const combo = formatShortcut(event);
    if (!combo) return;
    const id = commands.shortcutKeys().get(combo);
    if (id) {
      event.preventDefault();
      commands.run(id);
    }
  }

  $effect(() => {
    refreshHealth();
    refreshProjects();
    const healthTimer = setInterval(refreshHealth, 15000);
    window.addEventListener('keydown', onWindowKeydown);
    return () => {
      clearInterval(healthTimer);
      window.removeEventListener('keydown', onWindowKeydown);
    };
  });
</script>

<div class="shell">
  <header class="topbar">
    <div class="brand">NL <span>VIDEO SUITE</span></div>
    <label class="project-picker">
      <span class="muted">Progetto</span>
      <select value={project?.id ?? ''} onchange={(e) => selectProject((e.target as HTMLSelectElement).value)}>
        <option value="">Scegli un progetto</option>
        {#each projects as p (p.id)}
          <option value={p.id}>{p.title}</option>
        {/each}
      </select>
    </label>
    {#if project}
      <div class="rename">
        <input aria-label="Nuovo nome progetto" bind:value={renameValue} placeholder={project.title} maxlength="100" />
        <button onclick={renameProject} disabled={!renameValue.trim() || renameValue.trim() === project.title}>
          Rinomina
        </button>
      </div>
    {/if}
    <div class="spacer"></div>
    <button class="ghost" onclick={() => commands.run('palette.open')}>Comandi</button>
    <div class="services" title="Servizi rilevati dal backend">
      <span class:ok={health?.ffmpeg} class:bad={!health?.ffmpeg}>FFmpeg</span>
      <span class:ok={health?.comfyui} class:bad={!health?.comfyui}>ComfyUI</span>
      <span class:ok={health?.ollama} class:bad={!health?.ollama}>Ollama</span>
    </div>
    {#if health}
      <span class="muted version">v{health.version}</span>
    {/if}
  </header>

  <div class="body">
    <nav class="rail" aria-label="Workspace">
      {#each WORKSPACES as w}
        <button
          class:active={layout.workspace === w.id}
          onclick={() => (layout.workspace = w.id)}
          title={w.shortcut}
        >
          {w.label}
        </button>
      {/each}
      <div class="spacer"></div>
      <a class="legacy" href="http://127.0.0.1:8099/" target="_blank" rel="noreferrer">
        Interfaccia completa attuale
      </a>
    </nav>

    <main class="workspace">
      {#if layout.workspace === 'materials'}
        <section class="panel">
          <h2>Progetti e materiali</h2>
          {#if project}
            <p>«{project.title}» · {project.assets.length} media.</p>
            <p class="muted">
              Importazione, trascrizione e montaggio completi restano disponibili nell'interfaccia attuale;
              questa shell li ospiterà pannello per pannello.
            </p>
          {:else}
            <p class="muted">Scegli un progetto dalla barra in alto.</p>
          {/if}
        </section>
      {:else if layout.workspace === 'edit'}
        <div class="edit-grid" style="--inspector:{100 - layout.monitorSplit * 100 * (1 - layout.inspectorWidth)}%;--timeline:{layout.timelineHeight * 100}%">
          <section class="panel monitors">
            <div class="monitor"><h3>Sorgente</h3><p class="muted">Il monitor sorgente usa gli stessi proxy H.264 del motore attuale.</p></div>
            <Splitter
              axis="x"
              fraction={layout.monitorSplit}
              label="Dividi area programma / ispettore"
              onchange={(v) => (layout.inspectorWidth = 1 - v)}
            />
            <div class="monitor program"><h3>Programma</h3><p class="muted">Composizione live della timeline.</p></div>
          </section>
          <Splitter axis="y" fraction={layout.timelineHeight} label="Altezza timeline" onchange={(v) => (layout.timelineHeight = v)} />
          <section class="panel timeline">
            <h3>Timeline</h3>
            <p class="muted">
              {#if project}
                «{project.title}» · il porting della timeline canvas (filmstrip, waveform, trim, snapping)
                è il prossimo passo della shell.
              {:else}
                Nessuno progetto aperto.
              {/if}
            </p>
          </section>
        </div>
      {:else if layout.workspace === 'social'}
        <section class="panel"><h2>Shorts e Social</h2><p class="muted">Candidati, copertine e consegne: pannello in arrivo dalla UI attuale.</p></section>
      {:else}
        <section class="panel"><h2>Storie e Regia AI</h2><p class="muted">Storia, reference e generazione: pannello in arrivo dalla UI attuale.</p></section>
      {/if}
    </main>
  </div>

  <footer class="statusbar">
    <span>{status}</span>
    {#if health?.data_warnings?.length}
      <span class="bad">{health.data_warnings.length} avvisi dati</span>
    {/if}
  </footer>
</div>

{#if paletteOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="palette-scrim" role="presentation" onclick={() => (paletteOpen = false)}>
    <div class="palette" role="dialog" aria-label="Riga di comando" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_autofocus -->
      <!-- the dialog itself owns Escape through the search input -->
      <input
        autofocus
        bind:value={paletteQuery}
        placeholder="Digita un comando…"
        onkeydown={(e) => {
          if (e.key === 'Escape') paletteOpen = false;
          if (e.key === 'Enter') {
            const first = filtered[0];
            if (first) { paletteOpen = false; commands.run(first.id); }
          }
        }}
      />
      {#each filtered as command (command.id)}
        <button
          onclick={() => {
            paletteOpen = false;
            commands.run(command.id);
          }}
        >
          <span>{command.label}</span>
          {#if command.shortcut}<kbd>{command.shortcut}</kbd>{/if}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .shell { display: grid; grid-template-rows: 52px 1fr 28px; height: 100vh; }
  .topbar {
    display: flex; align-items: center; gap: 12px; padding: 0 14px;
    background: var(--bg-raised); border-bottom: 1px solid var(--line);
  }
  .brand { font-weight: 700; letter-spacing: .06em; }
  .brand span { color: var(--accent-2); }
  .project-picker { display: flex; align-items: center; gap: 6px; }
  .rename { display: flex; gap: 6px; }
  .rename input { width: 190px; }
  .spacer { flex: 1; }
  .ghost { background: transparent; }
  .services { display: flex; gap: 8px; font-size: 12px; }
  .services span { color: var(--text-dim); }
  .services span.ok { color: var(--ok); }
  .services span.bad { color: var(--bad); }
  .version { font-size: 12px; }
  .body { display: flex; min-height: 0; }
  .rail {
    width: 168px; display: flex; flex-direction: column; gap: 4px; padding: 10px 8px;
    background: var(--bg-raised); border-right: 1px solid var(--line);
  }
  .rail button { text-align: left; background: transparent; border-color: transparent; }
  .rail button.active { background: var(--bg-panel); border-color: var(--accent); }
  .rail .spacer { flex: 1; }
  .legacy { color: var(--text-dim); font-size: 12px; padding: 6px; text-decoration: none; }
  .legacy:hover { color: var(--accent); }
  .workspace { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 10px; gap: 10px; }
  .panel {
    background: var(--bg-panel); border: 1px solid var(--line); border-radius: 10px; padding: 12px;
    min-height: 0; overflow: auto;
  }
  .panel h2, .panel h3 { margin: 0 0 8px; font-size: 14px; }
  .edit-grid { flex: 1; display: flex; flex-direction: column; min-height: 0; gap: 0; }
  .monitors { flex: 1; display: flex; gap: 8px; }
  .monitor { flex: 1; border: 1px dashed var(--line-strong); border-radius: 8px; padding: 10px; min-width: 0; }
  .timeline { height: var(--timeline); flex: none; }
  .statusbar {
    display: flex; gap: 14px; align-items: center; padding: 0 14px;
    background: var(--bg-raised); border-top: 1px solid var(--line); font-size: 12px;
  }
  .palette-scrim {
    position: fixed; inset: 0; background: rgb(0 0 0 / .55);
    display: flex; justify-content: center; align-items: flex-start; padding-top: 12vh;
  }
  .palette {
    width: min(560px, 92vw); background: var(--bg-panel); border: 1px solid var(--line-strong);
    border-radius: 12px; padding: 10px; display: flex; flex-direction: column; gap: 4px;
  }
  .palette input { width: 100%; margin-bottom: 6px; }
  .palette button { display: flex; justify-content: space-between; gap: 10px; }
  kbd { color: var(--text-dim); font: 11px 'Consolas', monospace; }
</style>
