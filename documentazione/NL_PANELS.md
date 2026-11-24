# NL Video Suite — pannelli e contratto comandi (24/11/2026)

Questo file è la fonte unica per «tutti i pannelli NL sono attivi, collegati ai comandi e
interagiscono con la sequenza aperta». Ogni pannello ha:

- un corpo in `filmcraft-nl/crates/ui-egui/src/panels/nl.rs` (`body(panel)`), quindi non può cadere
  nel placeholder generico;
- una riga nel workspace **NL Video Suite** (menu Window ▸ Workspaces e pulsante Workspaces);
- la riga di contesto `nl.context`: progetto, sequenza attiva con risoluzione/fps, playhead,
  selezione, trascrizione/cue, provider ASR e endpoint ComfyUI;
- comandi primari registrati nel motore, controllati dal test
  `panels::nl::panel_contract_commands_are_registered`;
- una prova di runtime nel test `workspaces_ui::every_nl_panel_renders_context_and_no_placeholder`
  e nello script `documentazione/prove/p19-nl-audit/audit.py`.

## I 29 pannelli

| # | Pannello | Scopo | Comandi primari | Contesto / parametri usati |
|---|---|---|---|---|
| 1 | **NL Home** | Dashboard con tutte le funzioni, stato sessione e pulsante «Apri layout NL completo» | — (pulsanti che aprono i pannelli) | progetto, sequenza, parole, cue, scene storia, job attivi |
| 2 | **NL Generate** | Immagini/video da prompt con ComfyUI locale; dimensione dalla sequenza | `nl.models.workflows`, `graphics.generateComfy`, `nl.generate.text`, `nl.generate.extend`, `nl.prompt.enhance`, `nl.prompt.h3`, `nl.timeline.analyze` | workflow, prompt, seed, override, dimensione 1×/2× |
| 3 | **NL Music** | Musica con workflow locale | `nl.models.workflows`, `audio.generateMusic` | workflow, stile/testo, durata |
| 4 | **NL ASR** | Manager ASR: modelli, provider/device, trascrizione, import SRT/VTT, traduzione | `nl.asr.models`, `transcript.transcribeNl`, `captions.import`, `nl.translate.captions`, `nl.longform.analyze` | lingua, device, provider, modello, selezione/media |
| 5 | **NL Clips AI** | Candidati e sequenze reframed con sottotitoli; reframe automatico | `nl.clipping.propose`, `nl.clipping.render`, `nl.clipping.sequences`, `nl.reframe.sequence`, `nl.subtitles.srt` | target 9:16/16:9, focus auto/manuale, stabilità, reattività, comportamento, sottotitoli |
| 6 | **NL Sgrossa** | Piano tagli (silenzi, respiri, take) e render | `nl.sgrossa.analyze`, `nl.sgrossa.apply`, `nl.sgrossa.report` | selezione/media, lingua, pezzi scelti |
| 7 | **NL Analyze** | Capitoli, ricerca, traduzioni, assistente | `nl.longform.analyze`, `nl.search.index`, `nl.search.query`, `nl.translate.captions`, `nl.assistant.ask` | trascrizione della sequenza, query, lingua |
| 8 | **NL Transcript** | Testo della sequenza: ASR, SRT/VTT, sottotitoli, traduzione, capitoli | `transcript.transcribeNl`, `captions.import`, `transcript.createCaptions`, `nl.translate.captions`, `nl.longform.analyze` | parole/cue, lingua di destinazione, file SRT/VTT |
| 9 | **NL Longform** | Capitoli, marker, export, rough cut | `nl.longform.analyze`, `nl.longform.get`, `nl.longform.markers`, `nl.longform.export`, `nl.sgrossa.analyze` | trascrizione, file di export, piano cleanup |
| 10 | **NL Search** | Indice e ricerca del progetto | `nl.search.index`, `nl.search.query` | query, limite, progetto |
| 11 | **NL Shot Map** | Cambi scena con il motore della timeline | `clip.sceneEditDetection` | clip selezionati, sensibilità, durata minima, marker |
| 12 | **NL Social** | Profili social e controlli di consegna | `nl.delivery.social` | piattaforma, sequenza attiva (formato, durata, cue) |
| 13 | **NL Studio** | Hub finitura con stato della sequenza | — (pulsanti verso i pannelli) | clip video/audio, cue, durata |
| 14 | **NL Subtitles** | Cue list, split/merge/delete, import/export SRT/VTT | `captions.list`, `captions.setText`, `captions.split`, `captions.merge`, `captions.delete`, `captions.import`, `captions.export` | traccia, cue, playhead |
| 15 | **NL Titles** | Template MoGRT con controlli e place | `graphics.template.list`, `nl.titles.add` | template, controlli, sequenza |
| 16 | **NL Graphics** | Grafici nativi (rettangolo/ellisse/testo) | `nl.graphics.add` | sequenza, posizione, durata |
| 17 | **NL Audio** | Essential Sound, mix e link a Mixer | `essentialSound.inspect`, `essentialSound.set`, `mixer.inspect` | clip audio selezionati, strip |
| 18 | **NL Delivery** | QA, pacchetto produzione, QA social, storage | `nl.delivery.qa`, `nl.delivery.package`, `nl.delivery.social`, `nl.storage.cleanup`, `nl.storage.status` | sequenza attiva, cartella, piattaforma |
| 19 | **NL Album** | Album e inserimento in timeline | `nl.album.list`, `nl.album.insert` | progetto, sequenza, media |
| 20 | **NL Versions** | Versioni di montaggio (save/list/restore/delete) | `nl.versions.list`, `nl.versions.save`, `nl.versions.restore`, `nl.versions.delete` | progetto, nome versione |
| 21 | **NL Montage** | Piano di montaggio e applicazione | `nl.montage.plan`, `nl.montage.apply` | sequenza, target/ritmo/BPM |
| 22 | **NL Bridge** | Ponte tra due clip (ultimo/primo frame) | `nl.bridge.plan`, `nl.bridge.generate`, `nl.bridge.place` | clip uscente/entrante, gap, transizione |
| 23 | **NL Story** | Storia, scene, reference, keyframe, video, placement | `nl.story.get`, `nl.story.draft`, `nl.story.update`, `nl.story.generate`, `nl.story.place`, `nl.story.placeAll`, `nl.story.shotmap`, `nl.story.extendAudio`, `nl.story.placeAudio` | storia del progetto, scena, prompt, sequenza |
| 24 | **NL Queue** | Coda varianti: add/list/prepare/send/remove/free | `nl.queue.list`, `nl.queue.add`, `nl.queue.prepare`, `nl.queue.send`, `nl.queue.remove`, `nl.queue.free` | prompt, seed, conteggio, conferma |
| 25 | **NL Settings** | Valori NL: endpoint, modelli, risoluzioni, ASR | `nl.asr.models`, `nl.size.preview` | preferenze `nl.*` |
| 26 | **NL Cover** | Copertina AI dal frame | `nl.cover.prompt`, `nl.cover.set` | playhead, workflow, seed |
| 27 | **NL Workflows** | Libreria workflow, verifica, doctor, parametri, preset | `nl.workflows.list`, `nl.workflows.verify`, `nl.workflows.diagnose`, `nl.workflows.fix`, `nl.workflows.parameters`, `nl.workflows.import`, `nl.workflows.presets` | workflow, ComfyUI, override |
| 28 | **NL Services** | Servizi, modelli, spazio, progetto | `nl.services.status`, `nl.models.llm`, `nl.storage.status`, `project.inspect` | progetto, sequenza, endpoint |
| 29 | **NL Speech** | Scoperta modelli e trascrizione di un file | `nl.asr.models`, `nl.asr.transcribe` | file, provider |

## Come si verifica

1. **Statico (Rust)**: `cargo test -p filmcraft-ui-egui --lib panels::nl::` esegue
   `every_nl_panel_has_a_body`, `home_links_every_nl_panel_once`,
   `panel_contract_commands_are_registered`; `cargo test -p filmcraft-ui-egui --lib dock::tests`
   verifica che `NL_PANELS` sia nel layout e che il workspace sia built-in.
2. **Runtime headless**: `cargo test -p filmcraft-ui-egui --test it workspaces_ui::` apre ogni
   pannello, controlla `nl.context`, l'assenza di `panel.placeholder` e i pulsanti di NL Home.
3. **Runtime GUI**: `documentazione/prove/p19-nl-audit/audit.py` avvia l'exe, apre la copia GTA6,
   attiva `window.workspace.nlvideosuite`, apre ogni pannello, salva uno screenshot e un comando
   sicuro per pannello in `audit.json`.

## Limiti dichiarati (non «in arrivo»)

- L'avvio/arresto di ComfyUI e Ollama non è un'azione della suite: si usano i loro launcher. Il
  pannello Services mostra lo stato reale e non avvia nulla.
- Il reframe automatico richiede il runtime della suite (`runtime/python` + OpenCV) e i modelli
  locali; nel bundle portabile senza `runtime/` fa fallback statico e lo dichiara nel risultato.
- Topics longform, creative montage, narrative assistant/store, ASR catalog/store e service
  start/stop restano nel gap register `NL_PARITA_2026-11-24.md`, con stato e motivo. Non sono
  promessi nella UI.
