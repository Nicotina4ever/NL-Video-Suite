<script lang="ts">
  import { get, post, put, api, type ProjectSummary } from './lib/api';
  import type { ProjectFull, EditTool } from './lib/types';
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
  import Timeline from './lib/Timeline.svelte';
  import Transcript from './lib/Transcript.svelte';
  import Program from './lib/Program.svelte';
  import Sorgente from './lib/Sorgente.svelte';
  import Inspector from './lib/Inspector.svelte';
  import Shorts from './lib/Shorts.svelte';


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
  let full = $state<ProjectFull | null>(null);
  let timelineApi = $state<{
    seek: (ms: number) => void;
    razor: () => void;
    setTool: (tool: EditTool) => void;
  } | null>(null);
  let programApi = $state<{ toggle: () => void; rate: (value: number) => void } | null>(null);
  let playheadMs = $state(0);
  let timelineTab = $state<'cut' | 'transcript' | 'inspector'>('cut');
  let selectedClipId = $state<string | null>(null);

  async function reloadFull() {
    if (!project) return;
    full = await get<ProjectFull>(`/api/projects/${project.id}`);
  }
  const videoAsset = $derived(full?.assets.find((a) => a.kind === 'video') ?? full?.assets[0] ?? null);
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

  async function selectProject(id: string) {
    rememberProject(id || null);
    project = pickProjectSummary(projects, id || null);
    full = null;
    if (project) {
      try {
        full = await get<ProjectFull>(`/api/projects/${project.id}`);
        status = `Progetto aperto: ${project.title} · ${full.timeline.clips.length} clip.`;
      } catch (error) {
        status = (error as Error).message;
      }
    } else {
      status = 'Nessun progetto selezionato.';
    }
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

  async function releaseResources() {
    if (
      !confirm(
        'Rilascio manuale: i modelli Ollama in memoria vengono scaricati (righe successive: chi li usa li ricaricherà) e, SOLO se la coda ComfyUI è vuota, ComfyUI rilascia pesi e cache. I lavori in corso non vengono interrotti. Procedere?',
      )
    )
      return;
    status = 'Rilascio risorse in corso…';
    try {
      const reply = await post<{ note: string; actions: string[] }>('/api/v1/resources/release', {
        confirmation: 'SVUOTA',
      });
      status = `${reply.note} ${reply.actions.join(' · ')}`;
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
    id: 'playhead.start',
    label: "Vai all'inizio della timeline",
    shortcut: 'Home',
    group: 'Playback',
    run: () => timelineApi?.seek(0),
    enabled: () => !!full?.timeline.clips.length,
  });
  commands.register({
    id: 'clip.razor',
    label: 'Dividi la clip selezionata alla playhead',
    shortcut: 'C',
    group: 'Montaggio',
    run: () => timelineApi?.razor(),
    enabled: () => !!full,
  });
  for (const [toolId, label, shortcut] of [
    ['trim', 'Strumento Taglio', 'V'],
    ['ripple', 'Strumento Ripple', 'N'],
    ['roll', 'Strumento Roll', 'R'],
    ['slip', 'Strumento Slip', 'Y'],
    ['slide', 'Strumento Slide', 'U'],
  ] as [EditTool, string, string][]) {
    commands.register({
      id: `tool.${toolId}`,
      label,
      shortcut,
      group: 'Strumenti',
      run: () => {
        if (!timelineApi) {
          status = 'Apri il workspace Montaggio: gli strumenti agiscono sulla timeline.';
          return;
        }
        timelineApi.setTool(toolId);
        status = `${label}: ${toolId === 'trim' ? 'bordi liberi' : toolId === 'ripple' ? 'il seguito si avvicina' : toolId === 'roll' ? 'taglio condiviso, totale invariato' : toolId === 'slip' ? 'finestra sorgente mobile' : 'clip scorre, vicini assorbono'}.` +
          (toolId === 'slip' || toolId === 'slide' ? ' trascina il corpo del clip.' : ' trascina un bordo.');
      },
      enabled: () => !!full,
    });
  }
  commands.register({
    id: 'play.toggle',
    label: 'Riproduci / pausa il Programma',
    shortcut: 'Space',
    group: 'Playback',
    run: () => programApi?.toggle(),
    enabled: () => !!full,
  });
  commands.register({
    id: 'play.forward',
    label: 'Avanti veloce 2×',
    shortcut: 'L',
    group: 'Playback',
    run: () => programApi?.rate(2),
    enabled: () => !!full,
  });
  commands.register({
    id: 'play.normal',
    label: 'Velocità normale 1×',
    shortcut: 'K',
    group: 'Playback',
    run: () => programApi?.rate(1),
    enabled: () => !!full,
  });
  commands.register({
    id: 'play.back',
    label: 'Riavvolgi di un frame',
    shortcut: 'J',
    group: 'Playback',
    run: () => {
      playheadMs = Math.max(0, playheadMs - 33);
    },
    enabled: () => !!full,
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
    const target: unknown = event.target;
    if (target instanceof Element && target.closest('input,textarea,select')) return;
    const combo = formatShortcut(event);
    if (!combo) return;
    const id = commands.shortcutKeys().get(combo.toUpperCase());
    if (id) {
      event.preventDefault();
      commands.run(id);
    }
  }

  function onSeekEvent(event: Event) {
    const ms = (event as CustomEvent<number>).detail;
    if (Number.isFinite(ms)) playheadMs = Math.max(0, ms);
  }

  $effect(() => {
    void (async () => {
      await refreshHealth();
      await refreshProjects();
      if (project) await selectProject(project.id); // reopen the remembered project, like the classic UI
    })();
    const healthTimer = setInterval(refreshHealth, 15000);
    window.addEventListener('keydown', onWindowKeydown);
    window.addEventListener('nl-seek', onSeekEvent);
    return () => {
      clearInterval(healthTimer);
      window.removeEventListener('keydown', onWindowKeydown);
      window.removeEventListener('nl-seek', onSeekEvent);
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
    <button class="ghost" title="Scarica i modelli Ollama residenti e, se la coda ComfyUI è vuota, rilascia la sua memoria. Nessun processo esterno viene terminato." onclick={releaseResources}>Libera RAM/VRAM</button>
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
            <div class="monitor">
              <h3>Sorgente</h3>
              {#if full}
                <Sorgente
                  project={full}
                  onstatus={(m) => (status = m)}
                  onadded={async () => {
                    await reloadFull();
                    timelineTab = 'cut';
                  }}
                />
              {:else}
                <p class="muted">Apri un progetto per vedere i suoi media.</p>
              {/if}
            </div>
            <Splitter
              axis="x"
              fraction={layout.monitorSplit}
              label="Dividi area programma / ispettore"
              onchange={(v) => (layout.inspectorWidth = 1 - v)}
            />
            <div class="monitor program">
              <h3>Programma</h3>
              {#if full}
                <Program
                  project={full}
                  {playheadMs}
                  onplayhead={(ms) => (playheadMs = ms)}
                  onstatus={(m) => (status = m)}
                  exposed={(api) => (programApi = api)}
                />
              {:else}
                <p class="muted">Composizione live della timeline: apri un progetto.</p>
              {/if}
            </div>
          </section>
          <Splitter axis="y" fraction={layout.timelineHeight} label="Altezza timeline" onchange={(v) => (layout.timelineHeight = v)} />
          <section class="panel timeline">
            <div class="timeline-tabs">
              <button class:active={timelineTab === 'cut'} onclick={() => (timelineTab = 'cut')}>Montaggio</button>
              <button class:active={timelineTab === 'transcript'} onclick={() => (timelineTab = 'transcript')}>
                Trascrizione
              </button>
              <button class:active={timelineTab === 'inspector'} onclick={() => (timelineTab = 'inspector')}>
                Ispettore{selectedClipId ? ' · clip' : ''}
              </button>
              <div class="spacer"></div>
              <span class="muted ticks">{#if full}{full.timeline.clips.length} clip · {videoAsset ? 'parlato indicizzato' : 'nessun media audio'}{/if}</span>
            </div>
            <div class="timeline-host" class:hidden={timelineTab !== 'cut'}>
              {#if full}
                <Timeline
                  project={full}
                  {playheadMs}
                  onplayhead={(ms) => (playheadMs = ms)}
                  onselect={(id) => (selectedClipId = id)}
                  exposed={(api) => (timelineApi = api)}
                  onstatus={(m) => (status = m)}
                  onupdate={(p) => {
                    full = p;
                    status = `Timeline salvata (${p.timeline.clips.length} clip).`;
                  }}
                />
              {:else}
                <p class="muted">Nessun progetto aperto.</p>
              {/if}
            </div>
            <div class="transcript-host" class:hidden={timelineTab !== 'transcript'}>
              {#if full && videoAsset}
                <Transcript projectId={full.id} assetId={videoAsset.id} ontoseek={(ms) => timelineApi?.seek(ms)} onstatus={(m) => (status = m)} />
              {:else}
                <p class="muted">Apri un progetto con un media video o audio.</p>
              {/if}
            </div>
            <div class="transcript-host" class:hidden={timelineTab !== 'inspector'}>
              {#if full}
                <Inspector
                  project={full}
                  selectedId={selectedClipId}
                  {playheadMs}
                  onstatus={(m) => (status = m)}
                  onupdate={(p) => (full = p)}
                  onrazor={() => timelineApi?.razor()}
                />
              {:else}
                <p class="muted">Nessun progetto aperto.</p>
              {/if}
            </div>
          </section>
        </div>
      {:else if layout.workspace === 'social'}
        <section class="panel full">
          <h2>Shorts e Social</h2>
          {#if full}
            <Shorts
              project={full}
              onstatus={(m) => (status = m)}
              onupdate={(p) => (full = p)}
            />
          {:else}
            <p class="muted">Apri un progetto.</p>
          {/if}
        </section>
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
  .panel { background: var(--bg-panel); border: 1px solid var(--line); border-radius: 10px; padding: 12px; min-height: 0; overflow: auto; }
  .panel.full { flex: 1; display: flex; flex-direction: column; }
  .panel h2, .panel h3 { margin: 0 0 8px; font-size: 14px; }
  .edit-grid { flex: 1; display: flex; flex-direction: column; min-height: 0; gap: 0; }
  .monitors { flex: 1; display: flex; gap: 8px; }
  .monitor { flex: 1; border: 1px dashed var(--line-strong); border-radius: 8px; padding: 10px; min-width: 0; }
  .timeline { height: var(--timeline); flex: none; position: relative; }
  .timeline-tabs { display: flex; gap: 6px; align-items: center; }
  .timeline-tabs button { background: transparent; border-color: transparent; padding: 3px 9px; }
  .timeline-tabs button.active { border-color: var(--accent); background: var(--bg-raised); }
  .ticks { font-size: 11px; margin-left: auto; }
  .timeline-host { position: absolute; inset: 38px 12px 12px; }
  .transcript-host { position: absolute; inset: 38px 12px 12px; overflow: auto; }
  .timeline-host.hidden, .transcript-host.hidden { visibility: hidden; pointer-events: none; }
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
