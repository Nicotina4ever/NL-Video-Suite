# TODO — restauro NL Video Suite

Regola: **una voce = un commit**, spuntata solo dopo verifica visibile (test, screenshot o output).
Stato del backend e dei documenti: `STATO.md`, `MANUALE.md`, `ARCHITTETTURA.md`.
UI classica: `sviluppo/web/*.js` (54 moduli, ~671 KB) — fonte di verità per i pannelli.
Shell Premiere-style: `sviluppo/web-app/src/`.

## A. Adapter — tutta la UI classica raggiungibile dalla shell
- [x] Workspace «Classica» nella shell: iframe + selettore di tutte le schede classiche
- [x] Deep-link `?tab=` e `?project=` nella UI classica (necessario per l'iframe)
- [x] Base dell'iframe corretta in sviluppo (proxy vite) e in produzione (binario)

## B. Shell: pannelli mancanti (port dalla classica, uno per commit)
- [x] Project browser con miniature, selezione, «+ Timeline» e drag-to-timeline (`Media.svelte`)
- [x] Sottotitoli (`Subtitles.svelte`: elenco cue, testo, tempi, approvazione, stile, manuale, split/merge, export SRT/VTT/ASS/JSON)
- [ ] Motion studio (`motion_studio.js`, `motion_paths.js`, `motion_typography.js`, `motion_svg.js`, `motion_groups.js`, `motion_idea.js`, `motion_ranking.js`, `motion_presets.js`, `motion_images.js`, `motion_layer_timeline.js`)
- [ ] Mixer audio (`audio_mixer.js`, `audio_master_ui.js`)
- [ ] Regia / narrative (`narrative*.js`, `narrative_audio_first.js`, `narrative_library.js`, `narrative_preflight.js`, `narrative_assistant.js`)
- [ ] Workflow ComfyUI (`workflow_navigation.js`, `flow.js`, `creative_ui.js`, `creative_montage.js`)
- [ ] Pannello effetti / oggetti grafici IA (`object_media.js`, `card_ui.js`, `clip_animation.js`)
- [x] Safe margins nel monitor Programma (action-safe 90%, title-safe 80%, toggle + centro)
- [x] Monitor Programma applica volume, opacità, fade e trasformazione della clip (non solo il decode)
- [ ] Filmstrip per-tempo (frame via `/frame`) accanto alla waveform
- [ ] Drag e rilascio dalla Sorgente alla timeline

## C. UX Premiere
- [x] Timeline: timecode, durata totale, zoom slider, «Adatta», snap on/off, chip tracce con visibile/bloccata persistenti
- [ ] Pannelli dockable / ridimensionabili (oggi solo 2 splitter)
- [ ] Shortcut completi da `keyboard.md` di FilmCraft (143 comandi, adattati al browser)
- [ ] Scrub testuale (hot text) nei monitor
- [x] Marker di timeline: M per aggiungere, chip cliccabili per saltare, rimozione, persistenza per progetto
- [ ] Marker e guidatore nel Programma
- [ ] Selezione progetti con miniature

## D. Verifica
- [ ] `real_playback` a ComfyUI libero (oggi `running=1`: coda rispettata)
- [ ] Regressione completa (193 unit + 8 algebra + baseline 5/5)
- [ ] REA browser su ogni pannello, dati reali GTA6
- [ ] Prova IA reale sulle correzioni (richiede Ollama acceso)

## F. Backend — lacune scoperte durante la verifica
- [ ] Annullare un lavoro dalla coda (endpoint + pulsante): oggi un `prepare_preview` lungo non si può fermare dalla UI

## E. Housekeeping
- [ ] Svuotare `da_eliminare` (decisione del proprietario, ~545 MB)
- [ ] Push di `origin/main` (24+ commit in attesa)
