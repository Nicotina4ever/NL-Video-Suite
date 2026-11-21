# Architettura e sviluppo

Riferimento tecnico consolidato, 18 novembre 2026. Contratti e spiegazioni di implementazione restano consultabili qui; lo stato corrente prevalente è in [STATO_E_ROADMAP_1_0.md](STATO_E_ROADMAP_1_0.md). I riferimenti ai collaudi storici non costituiscono una regressione del candidato attuale.



## Finestra desktop e qualità dell'editor

Il Core può aprire Edge già installato in modalità app, senza schede o barra degli indirizzi, con profilo per installazione e cache richieste32MiB disco/32MiB media. Riusa lo stesso Rust/Axum e la stessa UI/compositor: non è un secondo motore, non introduce Electron o un altro Python. Il contenuto GUI è stato collaudato con Edge headless; la finestra desktop interattiva resta da validare nell'uso quotidiano. Il backend viene fermato dall'azione proprietaria Ferma, mai terminando servizi esterni.

Un eventuale host WebView2 nativo può riutilizzare la UI e i contratti esistenti; richiede un gate separato su file picker, ciclo vita, distribuzione e GPU. Non viene aggiunto ora come dipendenza per mascherare problemi di layout: ergonomia di monitor/timeline/inspector, playback e qualità artistica restano requisiti W1/W2/W6. Per1.0 nessuna riscrittura del renderer né duplicazione delle pipeline.



---

# Architettura

Il core Rust/Axum gestisce progetto, asset, timeline, coda e compositor FFmpeg. Un’unica UI legge questi dati; i motori specialistici vengono invocati tramite processi isolati o API locali.

```text
Video lungo → copia nel progetto → WhisperX → proposte ClipsAI → revisione
                                                              ↓
                                                     timeline canonica
                                                              ↓
                                            parlato delle clip selezionate
                                                              ↓
                                      Ollama: argomento + prompt + posizioni
                                                              ↓
                                           ComfyUI: immagini contestuali
                                                              ↓
                                         overlay agganciati al clip sorgente
                                                              ↓
                                      compositor FFmpeg → MP4 e audio mixato
```

## Contratti

- Tempi canonici in millisecondi interi; le conversioni dai secondi dei motori avvengono al confine.
- Gli asset appartengono al progetto. I percorsi originali sono provenienza, non sorgenti di render.
- Il compositor usa `start_ms`, `trim_in_ms`, `duration_ms`, ordine tracce, visibilità, volume e opacità. Gli intervalli vuoti rimangono neri/silenziosi. Le tracce audio partecipano alla durata finale.
- I salvataggi della timeline controllano la revisione per evitare di sovrascrivere aggiornamenti concorrenti.
- La coda serializza i lavori della suite. Non assume il controllo delle code esterne né invia stop globali.
- Le generazioni ComfyUI restano in esecuzione fino alla storia completata; solo allora scaricano e registrano i file. Il polling di `/history` ha un timeout dedicato e una finestra di tolleranza per gli errori transitori; il completamento è riconosciuto anche quando una build non espone `status.completed`. I media vengono registrati prima del placement: un placement rifiutato non lascia file orfani. Le grafiche seguono l’ID del clip sorgente: se viene spostato durante la generazione, il posizionamento segue il nuovo inizio.
- Le immagini generate usano 1,5× del video di riferimento, con tetto di 4096 px sul lato lungo e allineamento a 16. Un full HD diventa 2880×1616 (1620 non è allineato); un verticale 1616×2880. I workflow video seguono solo il formato di consegna, l’audio non viene ridimensionato e gli override espliciti di width/height vincono sempre.
- I prompt delle grafiche usano la trascrizione della parte di sorgente effettivamente mantenuta dal trim. Il patching del workflow individua i nodi tramite tag o connessioni positive; non sostituisce modelli o encoder.
- Workflow Doctor confronta il grafo con lo schema del ComfyUI attivo, inclusi tipi dinamici V3; propone riparazioni deterministiche applicabili a copie con audit. Un nuovo preflight precede ogni invio al server e segue gli upload. La validazione finale del backend rimane autorevole.
- La revisione grafica è abilitata di default nella UI: output importato, placement pendente nel risultato del job, approvazione esplicita e idempotente. Non c'è una scelta automatica del modello o una certificazione automatica della qualità artistica.
- `bridge/` contiene adattatori della suite. I motori già integrati sono snapshot autonomi in `engines/`, con provenienza e SHA-256 in `runtime/snapshot-manifest.json`; l'interprete è unico in `runtime/python/`. Non si importano moduli Python dalle quattro applicazioni originali. Le configurazioni precedenti migrano verso questi default senza fallback nascosto al vecchio interprete.
- I workflow inclusi sono copie in `workflows/`: il catalogo preesistente viene riallacciato alle copie interne all'avvio. I pesi AI e i servizi Ollama/ComfyUI sono un livello separato: non sono incorporati nell'eseguibile Rust né duplicati insieme a ogni progetto.

La UI è inclusa nell’eseguibile tramite `include_str!`. Un eventuale shell desktop è un livello di confezionamento successivo. La creazione di scene 3D è fuori perimetro.


---

# Contratti Media Core v1 — aggiornati 26 ottobre 2026

### Motion SVG/ibridi e miniatura (build di sviluppo, non release)

- `motion.layers[].kind="svg"`, `svg.commands[]:{op:M/L/C/Z,points:[[x,y],...]}`: curve numeriche massimo4096/livello;max20livelli/composizione. Frontend converteSVG controllati concurveQ/T/S,H/V eaffini. XMLnon persiste e non viene eseguito;DTD/entità/istruzioni/script/link/immagini/filter/gradienti/stroke non espansi/archi non supportati sono rifiutati. Metadata`data-*` inerti ignorati. Il tracciamento denso del logoSVG reale(3764forme) supera il limite:errore esplicito,non importazione incompleta. Il PNG principale è supportato come asset.
- `kind="image",asset_id:UUID`: immagine già nella libreria interna,non percorso esterno. Testi/SVG e raster restano livelli indipendenti,gruppi/crop/keyframe ereditati. PassASS/rasterordinati comuni aMP4/PNG;preview alpha usa gli stessi poseFPS. Immagini elaborate solo nella bounding union animata intersecata al quadro. Glow/bordo tipografico non applicato alle immagini:usare forme separate.
- JSONpreset conserva riferimentiasset,non media/font. Su altro progetto riassegnare l’immagine nell’inspector;salvataggio blocca riferimentiassenti senza cercare fonti esterne.
- `social.thumbnail`: frame,dimensioni,titolo/font/colore/geometria/fondo persistenti. `POST /social/thumbnail` congela lo snapshot,quantizza il frame e produce PNG RGB +JSON inexports;nessuna catturaCanvas/ASR/inferenza. `GET /social/thumbnail/{key}` restituisce il PNG;recupero dell’ultimo output dai jobs. Finestretransienti conservano faseMotion e fade/slideclip,non vengono serializzate.
- `GET /social/qa`:reportregolelocali,lingue,avvisi/evidenze,limitiespliciti. `.qa.json` accompagna gli exportSocial. Non WER/CER,nonfact-checking,nonpunteggio virale o certificazioneartistica;nessuna correzioneautonoma.

API `/api/v1/capabilities`: capacità effettive e limiti di consegna. Il compositor comune e le motion seguono gli FPS razionali del profilo Social selezionato (legacy senza selezione:30fps). Il contratto temporale non è certificazione creativa o QA di tutti gli FPS/risoluzioni.

## Tempo e mapping

`TimeRange { start_ms, end_ms }` usa interi, intervalli semiaperti e limite di 24 ore. `Mapping { clip_id, asset_id, track_id, source, timeline, visible }` è una relazione plurale: un passaggio della sorgente può comparire più volte nel montaggio. L'API `/api/v1/projects/{id}/time-map` accetta un range e opzionalmente un asset: con asset proietta source→timeline, senza asset restituisce timeline→source. Nessuna scelta nascosta della prima occorrenza nel contratto.

## ASR, parole e correzioni

WhisperX conserva la sua pipeline e l'allineatore italiano locale. Qwen GGUF è un adapter sperimentale CPU per il runner Windows già presente; percorsi esterni salvati per PC in `data/asr_providers.json`, non incorporati nel pacchetto pulito. Configurazione verificata per header GGUF, file e limiti; le capacità vengono provate con inferenza reale, non dedotte dal nome del modello.

`Word { id, transcript_id, asset_id, raw_text, corrected_text?, start_ms, end_ms, speaker_id?, language, origin_engine, align_status, timing_unit }`. L'ID deriva da transcript UUID, indice, testo raw e tempi. Le correzioni risiedono in `project.longform.corrections`; l'ASR originale resta immutato. Un testo modificato è marcato `edited_needs_review`, non falsamente riallineato. Se non esistono timestamp parola, i segmenti restano `segment_only`. Speaker e confidence sconosciuti restano null/unknown. Una cache hit Qwen conserva UUID e correzioni.

L'adapter Qwen analizza le pause con FFmpeg, usa core disgiunti con contesto sovrapposto, carica al massimo un blocco configurabile nel runner, converte gli offset in tempi assoluti e preserva ripetizioni autentiche. Cache per hash di sorgente/runner/modelli, parametri e versione adapter. I checkpoint GGUF sono referenziati, mai copiati.

## Editing testo e cue

`EditState { schema_version, revision, corrections, decisions }` conserva keep/exclude/review/highlight e motivazione. Una proposta è separata dalla timeline; applicarla richiede entrambe le revisioni correnti. I clip generati sono riconoscibili tramite `clip_origins` e aggiornati idempotentemente. Gli altri clip e gli asset non vengono sostituiti. Un Keep successivo può ripristinare parte di un'esclusione.

`Cue { id, clip_id, asset_id, source, timeline, word_ids, text, raw_text, requires_review, timing_unit }` proietta le parole mantenute sulla timeline, anche dopo trim e split. Non costruisce timestamp fittizi per parole senza allineamento. SRT/VTT trasportano testo e tempi, non stile/animazioni; burn-in e PNG monitor usano lo stesso ASS/libass. StudioM4 conserva unità sorgente, offset di presentazione CC separati, lingue e undo strutturale. Cue manuali senzaASR hanno transcript nil, zero parole e timing editoriale esplicito, non inventato dall'ASR.

## Motion, identità personali e compositing

`Project.motion { schema_version:1, revision, graphics[], style_presets[], templates[] }`: livelli nativi text/rect/ellipse/path/group/svg/image, ID/keyframe, anchor clip opzionale, approvazione distinta. Tracciati numerici a2–64punti normalizzati, stroke_progress0–1; SVG numerici e immagini interne secondo il subset dichiarato sopra, mai XML eseguibile o URL esterni. SVG universale non supportato.

`Layer.parent_id` riferisce un gruppo nella stessa composizione, senza cicli, massimo4 antenati e scala cumulata≤4×. Il gruppo non disegna pixel; trasformazioni e visibilità si ereditano, l'opacità moltiplica ciascun figlio (non una precomp isolata). Ordine forest/preorder con figli contigui, copy/import rigenerano parent e link. Il gruppo non vuoto non viene cancellato implicitamente; spostare prima i figli. Forma collegata e testo devono avere lo stesso parent. Tutte le proprietà restano editabili.

`Layer.crop? {left,top,right,bottom}` è un box locale0–1 non vuoto, normalizzato alla larghezza/altezza del livello. Viene trasformato in poligonoASSclip nello spazio finale, segue scala/rotazione/parent e ritaglia fill/outline/glow senza cambiare i dati originali. Non è una maschera di gruppo o un path libero animato. La misura dei glifi per width_link ignora il crop.

`Layer.active_range? {start_ms,end_ms}` usa tempi locali della composizione, semiaperti,entro la durata; assente=intera composizione. Il range di un gruppo limita i figli; il ritaglio non riscrive i keyframe. Timeline locale offre drag key/bordi,seek,numerici,tastiera eundo; campionamento finale agli FPS di progetto. Crop/range sono conservati nei presetJSON/copy e hanno default retrocompatibili. TestMotion UI→MP4 miratoPASS,regressione successiva in corso.

Le finestre shorts/A-B portano una `timeline_origin_ms` **transiente,non serializzata** nel documento motion per le grafiche libere. Il renderer sottrae l'origine preservando tempi negativi e fase già iniziata,anziché ricominciare l'animazione da0. Gli anchor ai clip usano già i clip rebased. La proprietà non modifica i tempi del progetto originale. Il batchshorts rifiuta clip di propria provenienza che contengono range appena esclusi o fuori dal candidato;il rimontaggio è esplicito perché sostituisce le modifiche manuali di quei clip.

Gli stili di testo e le composizioni personali sono dati del progetto. LibraryJSON `{schema_version:1, templates[], style_presets[]}` non contiene font/media/progetti, va validata su `/api/v1/motion/library/validate`; importazione/istanza rigenerano ID e riferimenti interni, mai trasferita l'approvazione. NL è preset opzionale, non watermark né vincolo di palette.

`Layer.width_link { layer_id, padding }` collega la forma al testo della stessa composizione: misura ink e wrapping con libass a risoluzione di consegna, cache rigenerabile `.cache/text-metrics`, X/scala/rotazione seguono il testo. Larghezza manuale salvata rimane fallback quando si scollega; il documento non rasterizza il testo per conservarlo. Helper usa frame grigio di misura, non nuove dipendenze/modelli.

Monitor ed export usano RGB per il lettering/motion, ASS `YCbCr Matrix: None`, z-order composizioni contigue e CC sopra motion. Hold statici sono eventi accorpati, non nuovi frame duplicati. Per alpha corretto: RGB su nero + copertura bianca, alpha_mode premultiplied dichiarato prima di unpremultiply (negoziazione del FFmpeg9 incluso), AVTB per tempi subsecondo, WebMVP9alpha420 standard. Rendering motion alla dimensione di consegna prima del downscale, senza imporre codec sperimentali. [Riferimento implementativo primario](https://github.com/FFmpeg/FFmpeg/blob/master/libavfilter/vf_premultiply.c).

`GET /api/v1/fonts` legge famiglie Windows di sistema/per utente. Nessun font installato/copiato/incluso; se manca una famiglia si conserva il riferimento e si dichiara fallbackArial. Le licenze restano da verificare separatamente. L'assistente stile usa il provider/modello del ruolo graphics e ritorna una proposta validata in un job: nessuna modifica automatica a testo, keyframe, approvazione o timeline.

La cache browser dei PNG CC èLRU a64MiB stimati diRGBA decodificato,max60immagini,2richieste in-flight,timeout15s. Immagini stantie non vengono inserite dopo cambio progetto/revisione;errore per identità viene memorizzato fino a retry esplicito o nuova revisione. È una politica sui riferimenti trattenuti,non limite garantito delRSS/GPU del browser. Test4K reale:2immagini/66.355.200byte stimati;503simulato non crea loop di retry.

Ultima regressione completa **31/31 PASS,92Rust** (`20261026T090151Z`), successiva anche alla cacheCC e al fix overridecolori. Motion/Subtitle/typography con vere PNG/WebM/MP4;overlapCC/motionMAE6.76495<12,linea255px suink255.5px,presetJSON/gruppi/crop/range/undo/reload/mobile,cache/retry eguardiaShorts verificati. Otto formati con reviewtiming sintetici esplicitaUI,highlight/reveal effettiviASS:globale nativeRGBMAE1.4824–4.8387<12. Il confronto colore aggiuntivo converte la reference nella stessa pipeline delivery yuv420p→RGB (MAE2.4648–3.0199<12),conservando separatamente errori native-edge/solid:non pixelidentici. Provider simulati dichiarati,test8B reale precedente separato. Il precedente30/31FAIL resta nei rapporti. I P0 del MasterPlan ancora aperti impediscono di dichiarare pronta1.0.

## Persistenza e compatibilità

Waveform `GET/POST /api/v1/projects/{id}/assets/{aid}/peaks`: POST mette in coda la lettura streaming PCM mono48k/s16, GET restituisce picchi firmati min/max ogni100ms (normalizzare per32768), campioni effettivamente decodificati e durata. Limite sorgente6ore noto e dichiarato; non è analisi ASR, shot detection o misura audio professionale. Cache in `data/previews`, identità UUID+dimensione+mtime/versione, non un hash SHA di contenuto. Overview NLE SVG numerico generato dai medesimi picchi: nessuna esecuzione di XML importato. Ruler/zoom/seek Longform usano i tempi sorgente; il seek viene differito finché i metadata appartengono alla sorgente giusta.

Qwen: `metadata.alignment_diagnostics` conserva `unplaced_words` con testo e punto indicativo se disponibile, **mai durate parola inventate**, e `raw_owned_chunk_texts`. La sidecar versionata può essere costruita da chunk vecchi senza inferenza e senza cambiare result.json v1 o ID/parole già salvati. `alignment_completed=true` certifica che il processo di allineamento ha concluso, **non** che ogni token abbia durata valida o che l'ASR sia accurata. Longform e Advisor segnalano i token non collocati; resta necessaria revisione dei raccordi e del parlato misto.

Primo sync CC sceglie una lingua realmente presente se il documento è vuoto e il default non trova cue. Documenti esistenti non cambiano lingua di nascosto: il filtro vuoto offre pulsanti espliciti per mostrarli e salvare. `unknown` non significa italiano né traduzione, e non approva automaticamente i cue.

Thumbnail `include_motion` e `include_captions` sono false di default: una miniatura pulita non impone i lower-third del video; le opzioni sono persistenti e non modificano la timeline originale. I media già sulle tracce video rimangono visibili. PNG RGB dal compositor, non screenshot del monitor; titolo editabile nel documento Social.

La preparazione Motion ritorna `geometry:[{id,width,link_offset_x}]` a risoluzione di consegna anche per bozze, senza approvare il documento salvato. Le maniglie width-link utilizzano queste metriche; un cambio del testo/font/link invalida la guida, un cambio dei soli punti no. Stima preventiva ASS limitata a64MiB per respingere animazioni SVG dense prima di allocarle; non garantisce il RSS globale del processo.

Campi nuovi del progetto hanno default compatibili con 0.7.1. Le mutazioni lavorano su una copia, salvano atomicamente il record e pubblicano la nuova versione in memoria soltanto dopo il successo. Le correzioni riferite a trascrizioni sostituite restano conservate ma non vengono trasferite arbitrariamente a parole differenti.

## Provenienza ASR, diagnostici e shot map (tranche 26 ottobre)

Longform distingue i metadata della trascrizione salvata dai selettori della prossima ASR. Lingua richiesta e lingua dichiarata possono differire; `unknown` non viene trasformato in italiano. Il pannello mostra modello/hash/engine solo se attestati, altrimenti «non dichiarato».

`POST /api/v1/projects/{id}/subtitles/manual` accetta un riferimento opzionale `diagnostic:{transcript_id,index}`. Il server verifica ownership, indice e duplicati, conserva il raw del diagnostico e la provenienza nel cue. Il range è una scelta editoriale esplicita, inizialmente vuota in UI: `word_timing_verified:false`, zero parole e approvazione false. Non corregge i tempi canonici ASR. Raw per blocco e confini sono consultabili/ascoltabili; durate parola oltre2s sono indizi, non errori dimostrati. Nessun WER/CER senza testo umano indipendente.

`GET/POST /api/v1/projects/{id}/assets/{aid}/shots`: candidati FFmpeg scdet in CPU2thread su tutti i frame ridotti320px. Riusa un proxy esistente con durata/stat compatibili, senza generarne altri. Cache per identità media SHA256, parametri e firma del binario; size/mtime evitano hash ripetuti. Checkpoint periodico con ripresa a1s prima dell'ultimo tempo e deduplica dei frame già letti. È un rilevatore di differenze brusche: dissolvenze possono mancare, flash/movimenti/sovrimpressioni possono essere falsi rispetto alle inquadrature desiderate.

`longform.shot_parameters` e `shot_decisions` conservano configurazione, conferma/esclusione, motivo e marker espliciti `manual:true`. La revisione richiede versione progetto/revisione Longform e identità della mappa corrente. Un marker manuale non finge di essere rilevato. Cambio input/parametri non cancella decisioni vecchie né le riapplica ad altra identità. Nessun confine modifica automaticamente la timeline.

`POST .../shots/inspect {center_ms,params?}` legge esclusivamente una finestra locale≤1,6s, campionata80×46 grigio/10fps, massimo17frame. Può funzionare senza scansione completa. Cache separata versionata, nessuna mutazione progetto/decisioni. Mostra distribuzione temporale delle differenze, frazione di tile cambiati e riduzione dopo piccole traslazioni, con indizi non esclusivi: cambio brusco, variazione graduale, movimento, cambio localizzato, flash o ambiguo. NON identifica semanticamente una sovrimpressione a schermo intero né certifica uno stacco. Tempi campionati non sostituiscono il controllo sul player. `analysis_seconds` è tempo trascorso della lettura/elaborazione CPU, non somma di core-secondi né stima VRAM.

Corretto inoltre il caricamento globale: risposte di progetti precedenti non possono sostituire la selezione corrente. I test ritardano deliberatamente la risposta precedente per verificare trascrizione, media e provenienza coerenti.

## Esito ComfyUI incerto e layout verticali (accettazione26ottobre)

Prima di `/prompt`, la suite salva `generated/{job_id}/prompt-intent.json` con workflow effettivo, server/progetto e placement. Invia marker `extra_data.nl_suite_job_id` e `client_id` propri; un errore di trasporto non causa reinvio automatico. Se manca la ricevuta, il recupero esplicito di un job failed cerca una corrispondenza unica di job/client/workflow in queue e ultimi200history, con risposte limitate16MiB. Zero o più corrispondenze lasciano l'intenzione intatta e producono errore esplicito. Una corrispondenza salva la ricevuta e usa l'import idempotente esistente, senza rigenerare o approvare il placement. Grafi riscritti dal server o risultati più vecchi della finestra possono richiedere controllo manuale: non si attribuiscono output per somiglianza o nomefile. Nessuna API interrupt/clear/unload globale. [Schema server primario](https://github.com/comfyanonymous/ComfyUI/blob/master/server.py).

I nuovi preset verticali Pulse/Prisma/Signal/Neon riservano una banda alta e dimensioni/font coerenti con il formato; non riposizionano composizioni già salvate o template personali. Non è face detection: controllare il quadro reale e la safe-zone. Testi ancora uguali ai segnaposto dei preset producono un avviso, non vengono cancellati automaticamente. Il selettore globale progetto impedisce modifiche tramite gli handler comuni finché non corrisponde al progetto caricato.

## Riferimenti di protocollo consultati

- Runner C++ e JSON word timestamps: https://github.com/predict-woo/qwen3-asr.cpp
- Famiglia ASR/forced aligner: https://github.com/QwenLM/Qwen3-ASR
- Capitoli YouTube, riferimento per M2: https://support.google.com/youtube/answer/9884579?hl=it (consultato 25 ottobre 2026).


---

# AI Montage Planner — guida della build di sviluppo

Non è un secondo editor: il risultato è una normale timeline NL con clip, movimenti, audio e titoli Motion modificabili. Le proposte non approvano asset generativi e non avviano ComfyUI.

## Percorso pratico

1. In **Montaggio → Foto / video / musica**, importa o aggiungi i materiali del progetto. Descrivi soggetto, inquadratura, momento importante e intenzione nelle note. Non usare il nome del file come descrizione del contenuto.
2. Scegli musica e volume; facoltativamente aggiungi una narrazione audio e le sue indicazioni. L'analisi ritmica CPU già presente propone attacchi energetici correggibili, non beat obbligatori o riconoscimento del ritornello.
3. **Proteggi scelta manuale / approvata** conserva gli elementi durante la pianificazione. Le modifiche manuali alla sequenza li proteggono automaticamente: sblocca esplicitamente solo quelli che vuoi riproporre. Spunta gli elementi per rigenerare una parte, senza toccare gli altri.
4. Facoltativo: spunta foto/video e premi **Analizza foto / frame spuntati**. Richiede un modello visivo già assegnato in Sistema. Per un video viene analizzato soltanto il frame al trim, non il filmato intero né il suo audio. Risposte/cache sono incrementali; verifica e approva ogni descrizione prima dell'uso nel planner.
5. Nel planner scegli **IA** (provider/modello del ruolo Storia) oppure **Offline**. Scrivi obiettivo artistico, atmosfera e vincoli; la continuità di Storia è opt-in, non obbligatoria per un album fotografico.
6. Scegli la politica audio: **Conserva originale** (predefinita), **Solo musica** (esclusione esplicita), oppure **Mantieni le regolazioni manuali**. I nuovi video entrano con audio originale attivo; caricare un vecchio progetto non ne modifica i volumi. Il modello non può autonomamente silenziare una sorgente protetta.
7. Confronta il riepilogo **Ordine effettivo**, calcolato dai dati, con le schede. Il commento libero del modello è conservato in una sezione separata, non descrive necessariamente quanto sarà montato. Motivazioni, titolo, durate/trim, raccordi, movimenti e volumi sono modificabili. Il titolo iniziale è richiesto per default nella UI (disattivabile); motivazioni IA o titolo richiesto mancanti impediscono l'accettazione. Nessun testo viene inventato per mascherare un campo vuoto.
8. Nulla è preselezionato. Correggi i campi oppure rigenera soltanto gli elementi spuntati e sbloccati, scegliendo **Cosa cambiare** (tono, tempi, ordine, stile, titoli o audio). I campi fuori ambito e le annotazioni originali sono preservati dal server. **Accetta selezionate nella sequenza** chiede conferma; una variazione manuale di volume rispetto alla protezione originale richiede consenso esplicito. Una ricevuta persistente distingue le scelte accettate dalla proposta originale.
9. **Revisiona proposta → Approva e monta** mostra prima l'assemblaggio effettivo. L'accettazione del piano non aveva ancora cambiato la timeline. L'ordine negli slot effettivamente selezionati può differire dall'elenco completo delle proposte: verifica anche la ricevuta di accettazione.
10. Rifinisci i clip nell'inspector e i titoli nel **Motion Studio**: testo/font/colore/animazione rimangono livelli nativi. Esporta dalla timeline normale e riapri il progetto per verificare il risultato.

## Cosa viene preservato

- Solo gli slot selezionati vengono riordinati; elementi non scelti o bloccati mantengono la posizione. Nuovi materiali selezionati vengono aggiunti in fondo.
- Omettere un elemento dal piano **non lo cancella** dalla sequenza. Per escluderlo, usa la rimozione esplicita del sequencer: non elimina il file originale o l'asset del progetto.
- Un'approvazione protegge le scelte accettate. Rigenerarle richiede lo sblocco esplicito.
- La protezione timeline confronta i clip con l'ultimo montaggio: conserva trasformazioni/tempi/audio modificati e rimozioni manuali, anche dopo più riapplicazioni. Non rende automaticamente perfette eventuali sovrapposizioni: revisionale nel monitor.
- Titoli già modificati in Motion non vengono sovrascritti. Undo della timeline non elimina le grafiche: rimangono ancorate ai rispettivi clip e non vengono rasterizzate.
- Proposte riferite a materiali, note o continuità superati vengono rifiutate; nessun cambio viene applicato su uno stato diverso senza revisione.

## Limiti espliciti

L'offline è una bozza variabile basata sui tipi di media/sezioni/durate: **non comprende il contenuto**. L'IA usa note, descrizioni visive approvate e ASR già disponibile; non vede ciò che non è stato analizzato. I segmenti ASR sono limitati esplicitamente all'intervallo sorgente selezionato, senza inventare tempi o reinviare migliaia di parole inutilmente.

Un riepilogo eloquente non certifica il JSON: il riepilogo autorevole è ora costruito deterministicamente da ordine, durate/sovrapposizioni, audio e titoli effettivi. I controlli segnalano anche elementi omessi, sezioni apertura/finale fuori posizione e segnaposto ricopiati; mancanze obbligatorie bloccano il merge anche via API. Non sono un giudice artistico, né un confronto semantico infallibile del testo libero. Note dell'utente, metadati misurati, descrizioni IA fallibili e intenzioni creative restano categorie distinte. Verifica in particolare parole tagliate, mix, crop dei volti, ripetizioni, leggibilità e continuità emotiva.

Avvisi su estratti audio, trim delle fotografie e titoli oltre l'apertura sono richiami alla revisione, non divieti automatici. Una frase del modello che garantisce «nessuna parola tagliata» non prova l'ascolto o l'allineamento. Gli avvisi liberi del modello sono conservati nella sezione non verificata, separati dai controlli tecnici dell'app. Gli avvisi sul piano iniziale possono riferirsi a valori che hai poi corretto: confrontali con le schede aggiornate prima di accettare.

Risorse mancanti sono **proposte non prodotte**, separate dai media disponibili: completale tramite Storia/reference o Generazione, revisionale e aggiungile soltanto dopo. Nessun modello, checkpoint o servizio viene installato o avviato automaticamente. Il limite di contesto è64kbyte e la selezione massima80elementi; selezioni molto lunghe possono richiedere sottogruppi revisionati per evitare output troncati, che vengono rifiutati anziché certificati.

La musica dei pacchetti di collaudo è sintetica e già esistente, non una colonna sonora definitiva. Il montaggio dimostrativo è da sottoporre a valutazione umana, non dichiarato pronto per una campagna.


---

# Workflow Doctor e controllo creativo — 0.3

Il riparatore migliora l'affidabilità tecnica senza decidere al posto dell'utente quale immagine sia professionale. Un workflow eseguibile può ancora produrre artefatti, risultati incoerenti o errori di memoria: non viene etichettato "qualità garantita".

## Uso

1. In **Generazione**, seleziona il workflow e modifica gli input/reference dal progetto.
2. Premi **Diagnostica e proponi riparazioni**. La diagnosi usa `/object_info` del ComfyUI effettivamente attivo, non un elenco teorico di modelli.
3. Leggi gli errori e le modifiche esatte. Per valori non disponibili il menu mostra le scelte reali: la sostituzione è una scelta esplicita dell'utente, non una dichiarazione di equivalenza.
4. Applica le correzioni selezionate a una **copia**. Si seleziona il nuovo preset `--riparato`; il file originale resta invariato. Rapporto con prima/dopo in `data/workflows/repaired/<id>/`.
5. Genera. Il preflight viene ripetuto prima di inviare `/prompt` e dopo eventuali upload reference.

I controlli comprendono nodi mancanti, input obbligatori, scelte/model file non disponibili, encoder mancanti, collegamenti/porte/tipi, cicli e dimensioni anomale. Riconoscono schemi V3 MatchType, DynamicCombo e Autogrow. Il controllo è preliminare: la validazione e l'esecuzione finale di ComfyUI restano autorevoli, soprattutto per custom node dinamici.

## Correzioni deterministiche attuali

- Migrazione di ingressi Autogrow legacy (`a` → `variables.a`) quando lo schema attivo descrive quel contenitore. Il collegamento/valore viene conservato.
- Bypass del separatore LTX AV su un ramo riconoscibile come solo video, esclusivamente se nessuno usa la sua porta audio.
- Normalizzazione dei separatori di un percorso soltanto quando l'intero percorso relativo corrisponde a un'unica scelta disponibile. Non cambia file, famiglia, precisione o encoder.
- Rimozione facoltativa di nodi scollegati dagli output, soltanto sulla copia.

Nessuna sostituzione per somiglianza del nome, scelta del primo file immagine/audio, installazione di nodi, download di pesi o modifica della configurazione ComfyUI. Un downscale viene segnalato, non annullato indiscriminatamente. Dimensioni irragionevoli vengono bloccate: non si converte arbitrariamente il formato in 1024×1024. I nodi dichiarati API esterne vengono bloccati nel flusso locale.

## Produzione promozionale

In **Grafiche contestuali** non c'è un modello scelto automaticamente. Il flusso automatico supporta text-to-image compatibili; workflow di editing/reference restano disponibili nel pannello Generazione, con selezione degli asset autentici.

La direzione visiva permette di specificare palette, luce, composizione e vincoli. I prompt mantengono separati positivo, negativo e istruzioni di sistema; supportano anche BasicGuider e stringhe User Prompt collegate. I testi/caption restano separati dall'immagine. Non viene inventata documentazione di lavori, crediti, testimonial o identità del doppiatore.

Il piano può anche proporre zero immagini: non si impone una grafica quando è preferibile lasciare spazio alla performance. Non ci sono cicli automatici che producono varianti fino a dichiararne una "perfetta".

La revisione delle immagini è attiva per default nell'interfaccia: la generazione importa l'asset ma non modifica la timeline. Dopo l'approvazione l'immagine viene inserita rispetto alla posizione attuale del clip sorgente. La doppia approvazione non duplica il clip; sorgente rimossa o traccia bloccata producono un errore esplicito. Disattivando volontariamente la revisione si mantiene l'inserimento automatico.

Non esiste ancora un benchmark creativo sul materiale autentico dell'attività, né una garanzia "impeccabile". Prima di pubblicare controllare pertinenza, anatomia/oggetti, eventuali scritte, coerenza cromatica, sincronizzazione e integrità della performance vocale. La suite non pubblica automaticamente sui social.

## Verifiche

- Unit test: prompt positivo/negativo/sistema, costanti dimensioni, tipi V3, migrazione Autogrow, LTX con/senza lettori audio, cicli e link mancanti.
- Smoke: preflight impedisce realmente l'invio del prompt guasto, riparazione su copia con hash originale invariato, revisione e approvazione idempotente.
- Browser: diagnosi, selezione delle correzioni e copia riparata, revisione attiva e nessun modello imposto.
- `tests/audit_workflows.py`: interrogazione in sola lettura dei nodi reali per gli attuali preset. Report `tests/artifacts/workflow-audit.json`, senza campionamento GPU o certificazione artistica.
- `tests/live.py --comfy-only`: upload/reference, preflight e round-trip SaveImage reali, senza campionamento neurale.

Protocollo: [route del server ComfyUI](https://docs.comfy.org/development/comfyui-server/comms_routes). Schemi V3 confrontati anche con `K:/AI/ComfyUI/ComfyUI/comfy_api/latest/_io.py` e il [sorgente ufficiale](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_api/latest/_io.py).


---

# Un solo progetto, quattro aree

Questa guida riguarda il **candidato di sviluppo**, `sviluppo/target/release/nl-video-suite.exe`. Il launcher `Avvia_NL_Video_Suite.cmd` avvia ancora la stable in `app/`, volutamente non sostituita. Per una prova manuale isolata, da PowerShell nella cartella della suite:

```powershell
& .\sviluppo\target\release\nl-video-suite.exe --port 8091 --data-dir .\sviluppo\prove-utente\data
```

Apri `http://127.0.0.1:8091` dopo l'avvio. Usa una porta libera e un progetto di prova; non avviare due istanze sullo stesso data-dir. Il comando non avvia automaticamente Ollama/ComfyUI né importa i progetti personali.

Le aree cambiano gli strumenti visibili, non creano timeline o librerie separate.

| Area | Strumenti |
|---|---|
| Progetti e materiali | Importa copie, sfoglia, apri nel monitor, aggiungi alla timeline |
| Montaggio | Timeline; testo e tagli; sottotitoli; titoli e motion; grafiche dal parlato; audio e musica; pipeline ClipsAI avanzata |
| Shorts e Social | Candidati, montaggio, copertine, metadati e consegne |
| Storie e Regia AI | Storia, reference e scene; workflow e riparazioni |

«Studio e strumenti» raccoglie provider, modelli, lavori/recupero e pulizia. Alt+1…4 apre le quattro aree. I ritorni alla timeline sono contestuali; le modifiche non salvate nei vari editor rimangono nei loro strumenti passando tra aree. Cambiare progetto richiede invece di salvare o gestire esplicitamente le bozze.

«Esporta…» nel Montaggio apre gli stessi profili di consegna del Social, senza saltare pagina. Seleziona una destinazione, imposta qualità e risoluzione, quindi applica o esporta. Chiudere non equivale a salvare. Il motore usa le copie degli originali nel progetto, non i proxy di anteprima. Nessuna normalizzazione audio implicita.

## Versioni del montaggio

Dal Montaggio: **Versioni…**, nella barra «Spazio di lavoro». Non è un altro workspace.

1. Salva le modifiche nei rispettivi strumenti; assegna un nome e scegli «Salva versione corrente».
2. Seleziona una versione e confrontala con il lavoro corrente: formato, timeline/mix, sottotitoli, motion, consegna e decisioni editoriali. Le differenze espandibili mostrano i dati esatti. Puoi rinominare o duplicare una versione.
3. «Riapri questa versione…» richiede conferma. Prima viene registrato automaticamente il lavoro corrente come versione di recupero. Poi l'interfaccia si ricarica, con Undo nuovo e versioni precedenti ancora disponibili.
4. Per tornare al lavoro successivo, confronta e riapri la corrispondente versione «Prima del ripristino».

Gli snapshot sono JSON in `data/projects/<id>/edit-versions/`. Condividono gli asset della libreria; non duplicano MP4, modelli, runtime o trascrizioni raw. Comprendono crop/fade/movimenti, livelli dei clip, oggetti grafici, decisioni Longform/Shorts, caption, motion, montaggio creativo e impostazioni Social/export. Non riavvolgono titolo del progetto, storia/produzione, libreria, lavori o inferenze.

Media mancanti, dati danneggiati, versione estranea, trascrizioni sorgenti modificate o conflitti con modifiche più recenti impediscono il ripristino. I file rimangono conservati. Aggiungere trascrizioni di altri materiali non invalida quelle già usate. Durante lavori del progetto attivi si attende: nessun ripristino su una generazione in corso. Limite per snapshot: 16 MiB. Il controllo d'integrità rileva alterazioni accidentali; non è una firma autenticata per file di provenienza sconosciuta. Non modificare manualmente questi JSON.

Le versioni condividono i media: non sono un backup dei file originali. Non cancellare gli asset pensando che ciascuna versione ne conservi una copia.

## Mixer e aspetto del candidato aggiornato

**Mixer…** nella barra del Montaggio apre Voce, Musica, Effetti e Master. Attivalo esplicitamente e salva: i progetti precedenti conservano il bypass. I bus aggiungono i propri livelli ai volumi e alle dissolvenze dei clip; una traccia spenta resta esclusa. Il routing è per traccia, modificabile: senza assegnazione esplicita video/grafiche vanno a Voce e tracce audio a Musica. Puoi cambiare il routing quando una traccia video contiene musica o una traccia audio contiene parlato. Solo isola i bus selezionati, Mute prevale; Mute Master silenzia tutto. Non si tratta di separazione delle sorgenti audio.

Le automazioni si scrivono come `secondi ; dB`, una coppia per riga, con tempi crescenti e unici. Tra i punti il livello varia linearmente in dB, mantenendo il primo/ultimo valore fuori dall'intervallo. Una curva presente sostituisce il livello statico del suo canale. Il tempo segue la timeline, anche dopo lo spostamento dei clip. Anteprima WebAudio ed export FFmpeg implementano lo stesso contratto. Non ci sono normalizzazione, ducking o limiter impliciti; somme o livelli elevati possono saturare. Il ducking Voce→Musica è opzionale: regola soglia, rapporto, attacco, rilascio e attenuazione massima. La profondità si automatizza con coppie secondi ; dB negative, fino a 0 per bypassare un tratto. Controlla il routing manualmente: non è riconoscimento delle sorgenti. La finalizzazione offre profili modificabili, target LUFS, gain e limiter esplicito. “Prepara e misura” conserva un master PCM e un ascolto AAC condivisi da Programma ed export. Le misure del file consegnato sono in encoding.json: silenzio e target non raggiunti non sono certificati. La prima preparazione richiede CPU e spazio temporaneo, senza modificare originali o modelli.

Il mixer viene salvato nei nuovi snapshot nominati; le versioni storiche prive del mixer ripristinano il bypass, non il mix del lavoro corrente. Undo della timeline non è la cronologia delle impostazioni audio: per confrontarle salva versioni nominate.

**Studio e strumenti → Sistema → Aspetto** offre Scuro, Chiaro e Sistema (segue Windows). La preferenza resta nel browser; il logo sorgente ha fornito i token #7531FE, #E4469B e #AD85C7. Non si applicano branding o colori ai video. Sotto1250px il monitor predefinito è Programma singolo, salvo scelta esplicita diversa: Sorgente e dual rimangono disponibili. I pannelli e la timeline scorrono indipendentemente; i separatori sono ancora regolabili con mouse e frecce.

## Stato del collaudo aggiornato

Versioni collaudate in server isolato con fixture autentica cached: UI → confronto → ripristino → export MP4 breve → riavvio; test per riferimenti mancanti, dati alterati, progetto estraneo e revisione concorrente. Nessuna inferenza. La regressione generale corrente è documentata nei V1, separata dall'accettazione UX/artistica dell'utente. Nessuna promozione della stable.

Il completo202955Z52/52/136Rust rimane storico. Ducking e finalizzazione sono ora operativi e misurati sul PCM e AAC; lo stato della nuova regressione è nei documenti V1. Ascolto umano, altri gate e promozione1.0 restano separati.

## Modelli e raw ASR

Il catalogo locale in Provider e modelli collega risorse esistenti senza copiarle o scaricarle. Default applicazione e scelta per progetto riguardano la prossima ASR, non attestano quale motore abbia prodotto il raw salvato. L'inventario riporta byte logici e conteggi eventualmente incompleti, non spazio sicuramente recuperabile. Nessun modello preesistente è considerato esclusivo soltanto perché si trova nella suite; proprietà sconosciuta, condivisa ed esterna bloccano la cancellazione. Scollegamento di profili referenziati/default o durante lavori attivi bloccato. La pulizia di checkpoint esclusivi con prova di proprietà è ancora un gate aperto, non una funzione implicitamente disponibile.

Nuove trascrizioni e importazioni conservano le precedenti. La selezione raw esplicita in Testo e tagli cambia la base per nuove analisi/shorts/grafiche; cue approvati e correzioni conservano i riferimenti originali. Il confronto mostra i raw, non un WER o giudizio di accuratezza. Versioni nominate preservano scelta raw e profilo; il ripristino non cancella raw aggiunti successivamente. Vecchi progetti senza selezione mantengono il primo raw. Cambiare modello non avvia elaborazioni; una nuova ASR richiede il comando esplicito.


---

# Distribuzione autonoma e consolidamento

La suite non deve essere un semplice pannello che smette di funzionare quando si archiviano Calliope, Regista, ClipsAI o ZP. I componenti integrati vengono conservati dentro la suite, non collegati alle cartelle originali.

## Architettura scelta

- Rust: UI locale, persistenza, asset, timeline, job, compositing FFmpeg, API Ollama/ComfyUI.
- `engines/`: copie selezionate dei motori Python già collaudati, senza le vecchie interfacce/server e senza ambienti duplicati per ogni tool.
- `runtime/python/`: un interprete gestito dalla suite per WhisperX e import; `requirements-installed.txt` ne registra le versioni effettive.
- `workflows/`: preset inclusi, indipendenti dai vecchi percorsi.
- Pesi AI condivisi fuori dai progetti. Non duplicare i pesi Ollama/ComfyUI in ogni applicazione.

Esegui `Prepara_Runtime.cmd` per preparare le copie locali senza download o aggiornamenti pip. Per ora si conserva intero l'ambiente ClipsAI noto funzionante: rimuovere pacchetti non verificati rischierebbe regressioni. La preparazione non sposta/cancella nulla e non converte ambienti virtuali in-place. Non lanciare nuovamente la copia mentre la suite sta elaborando lavori.

## Cosa non significa "un'app"

Un solo avvio e una UI non implicano un solo linguaggio o un file senza dipendenze. ComfyUI è Python/PyTorch e i suoi custom node possono richiedere versioni incompatibili con WhisperX o TTS. Non vengono copiati arbitrariamente nel runtime condiviso: prima occorre un test completo. Ollama è un servizio locale separato. La suite attuale non incorpora questi servizi e non ne gestisce ancora l'avvio.

Eliminare del tutto Python è possibile solo sostituendo uno per uno i backend con motori nativi compatibili, dimostrando qualità/parità. Cambiare Rust con C++ o C# non converte automaticamente i modelli, PyTorch o i nodi ComfyUI.

## Prima di archiviare gli originali

1. Migrare anche le funzioni avanzate e completare i video narrativi da zero.
2. Inventariare progetti, voci/reference, modelli, cache, custom node e relativi percorsi.
3. Collaudare ogni funzione con percorsi legacy non disponibili, inclusa inferenza reale e riavvio.
4. Salvare un archivio recuperabile; solo dopo verifica manuale rimuovere gli ambienti ridondanti.

`/api/runtime` è una diagnostica, non un'autorizzazione alla cancellazione. Oggi non c'è ancora parità completa. Il test smoke imposta appositamente tutti i percorsi legacy su cartelle inesistenti, senza rinominare o alterare i progetti dell'utente.

Riferimento: [installazione ufficiale ComfyUI](https://docs.comfy.org/installation/manual_install), in particolare isolamento delle dipendenze e percorsi condivisi per i modelli.


---

# NL Video Suite — manuale tecnico

Versione stabile in `app`: 0.7.1, preservata. Lo sviluppo1.0 è separato in `sviluppo`; il binario dev mantiene ancora il numero0.7.1 ed è identificato per SHA nei documentiV1. Ultimo giro completo26ottobre:40/40PASS,103Rust, non accettazione finale1.0. La guida utente stabile è `../LEGGIMI.html`; stato corrente e punto di ripresa sono in `../sviluppo/roadmap/V1_PROGRESS.md` e `V1_NEXT_ACTION.md`. Il pacchetto finale non è creato/promosso. I collaudi0.6 sotto sono cronologia; per ricollocazione completa e limiti vedere `PORTABLE.md`.

Suite locale con core Rust per ricavare video brevi da video lunghi, rifinire il parlato, aggiungere grafiche coerenti con l’argomento e montare il risultato. Interfaccia in italiano, servita su `127.0.0.1`.

La creazione di scene 3D è esclusa dal prodotto, su indicazione dell’utente. La parte narrativa di Calliope è riusata per storyboard e organizzazione dei contenuti.

Novità 0.3: [Workflow Doctor](WORKFLOW_DOCTOR.md), preflight contro il ComfyUI reale, riparazioni tracciate su copie e revisione delle grafiche prima della timeline. Nessun modello è scelto automaticamente per le grafiche; la qualità creativa va collaudata sui propri materiali.

## Avvio

Doppio clic su **Avvia_NL_Video_Suite.cmd** nella root, poi apri <http://127.0.0.1:8088>. Il launcher usa l'eseguibile incluso in `app`, avvia il server nascosto e apre il browser. Riusa soltanto un'istanza appartenente alla stessa cartella, senza compilazioni automatiche.

Per arrestare il server usa **Ferma_NL_Video_Suite.cmd**. Ferma solo il server della suite, non ricorsivamente i processi figli; ComfyUI e Ollama restano attivi. Se ci sono lavori in corso, il launcher chiede di attenderne la conclusione.

Oppure, da terminale:

```powershell
cargo run --manifest-path sviluppo/Cargo.toml --release -- --port 8088
```

Requisiti: FFmpeg/ffprobe su PATH; Rust serve per compilare, non per usare un eseguibile già compilato. Le analisi usano `runtime/python/python.exe` e i motori interni in `engines/`. La preparazione iniziale è disponibile con **Prepara_Runtime.cmd**; non sposta o elimina gli originali. ComfyUI e Ollama devono essere attivi per le rispettive funzioni. Non vengono scaricati automaticamente modelli AI.

L'obiettivo è poter archiviare i vecchi progetti, non dipendere da essi. Il runtime delle funzioni già integrate è stato separato dalle loro cartelle; **non c'è ancora parità completa e non vanno ancora cancellati**. Vedi [RUNTIME.md](RUNTIME.md) per ambienti condivisi, pesi, limiti di ComfyUI/TTS e verifiche necessarie.

## Flusso principale

1. Crea un progetto e importa una copia del video lungo dalla libreria.
2. In **Analisi e clipping**, seleziona il media e scegli trascrizione, sgrossatura, clip social, reel o shorts.
3. Revisiona le proposte: puoi deselezionarle e correggere gli intervalli. Aggiungi quelle scelte alla timeline oppure renderizzale con i sottotitoli originali ClipsAI.
4. In **Grafiche contestuali**, scegli esplicitamente un workflow text-to-image compatibile e indica la direzione visiva. **Analizza e genera** ricava argomento e prompt dal parlato. La revisione è attiva per default: l'asset viene importato ma entra in timeline soltanto dopo la tua approvazione, nel punto previsto del clip sorgente. Puoi anche chiedere solo suggerimenti, correggere i prompt e generare una selezione; l'inserimento automatico resta disponibile disattivando volontariamente la revisione.
5. In **Montaggio**, modifica inizio, trim, durata, volume, opacità e traccia; usa divisione, rimozione, mute, blocco e annulla/ripeti. **Esporta timeline** produce un MP4 con audio e overlay.

Il monitor Programma riproduce la composizione live della timeline; il monitor Sorgente mostra il media selezionato. Sul desktop possono essere affiancati. HEVC e altri formati non universali usano una cache H.264/AAC per il playback; l'export usa gli originali. I test NLE confrontano anche pixel e audio dell'export FFmpeg.

## Integrazioni disponibili

| Applicazione | Integrazione attuale |
| --- | --- |
| ClipsAI | Chiamata diretta ai motori originali: WhisperX, sgrossatura, clip, reel, shorts, render selettivo, SRT/ASS e verticale. Non richiede il vecchio server Flask. |
| Regista AI | Import della timeline con media, tempi, trim, volume e opacità; catalogo dei workflow API e generazione via ComfyUI. Trasformazioni legacy avanzate non supportate bloccano l’import con errore esplicito. |
| Calliope | Import SQLite in sola lettura di storia, personaggi, luoghi e shot già generati; storyboard Ollama e workflow ComfyUI nello stesso progetto. Scene 3D escluse. |
| ZP Shorts Studio | Import dei candidati approvati usando il parser e la matematica KEEP originali, comprese le esclusioni interne. |

Gli import si trovano in **Sistema**. Per Regista indica la cartella progetto; per Calliope il database e l’ID progetto; per ZP il `candidates.json` e il media selezionato in Analisi. Per preservare una timeline importata scegli un progetto di destinazione vuoto. Lo snapshot legacy è conservato nel progetto.

## Dati e servizi

`data/settings.json` contiene URL e percorsi dei motori. Il modello locale predefinito è `qwen2.5:14b-instruct`, già installato. Chiudi l’app prima di modificare le impostazioni. Le richieste native Ollama rilasciano il proprio modello alla fine; il bridge ClipsAI lo mantiene tra i passaggi della stessa analisi e lo rilascia al termine.

I progetti sono in `data/projects/<uuid>/`: `project.json`, `assets/`, `analysis/`, `jobs/`, `exports/`. I master restano al loro posto. I job sono serializzati e salvati in `data/jobs.json`. Dopo un riavvio i lavori incompleti vengono marcati interrotti e devono essere rilanciati; non c’è ancora ripresa automatica dal punto d’interruzione. Un lock impedisce a due processi di scrivere sulle stesse cartelle dati.

## Verifica

```powershell
cargo fmt --manifest-path sviluppo/Cargo.toml --check
cargo clippy --manifest-path sviluppo/Cargo.toml --offline --all-targets -- -D warnings
cargo test --manifest-path sviluppo/Cargo.toml --offline
cargo build --manifest-path sviluppo/Cargo.toml --offline --release
& '.\runtime\python\python.exe' sviluppo/tests/smoke.py
node sviluppo/tests/nle_browser.cjs sviluppo/target/release/nl-video-suite.exe
node sviluppo/tests/narrative_browser.cjs sviluppo/target/release/nl-video-suite.exe
```

I test browser richiedono Node, Playwright già disponibile in `sviluppo/tests/node_modules` ed Edge; non sono dipendenze dell'applicazione finale. I test HTTP e FFmpeg creano video sintetici temporanei. Verificano trim, spazi vuoti, overlay, audio, range HTTP, SRT, import legacy, coda, grafica contestuale e immutabilità dei master. Nei test di regressione i servizi AI sono simulati; i motori ClipsAI e il parser ZP sono copie interne reali. I percorsi delle vecchie applicazioni sono impostati su cartelle inesistenti. Il test browser usa Edge headless.

`tests/live.py` è una verifica esplicita con servizi reali: round-trip PNG ComfyUI, ASR WhisperX su voce sintetica italiana e storyboard Ollama. `tests/live_graphics.py` esegue anche generazione immagine e inserimento automatico: richiede ComfyUI libero e conserva il risultato in `tests/artifacts/live-graphics/`.

## Stato del lavoro

Questa versione copre i percorsi di montaggio e produzione per scene descritti nella guida; non rappresenta ancora la parità completa con tutte le quattro applicazioni. Restano l’assistente agentico completo, le riparazioni oltre i casi deterministici coperti, le versioni/compilazioni avanzate ZP, keyframe/effetti avanzati e l’annullamento dei lavori già in esecuzione. Il dettaglio e le verifiche sono in [STATUS.md](STATUS.md).


---

## Parità legacy: evidenze dei sorgenti

La matrice seguente conserva il confronto storico e i simboli verificati. Per le integrazioni successive e le lacune attuali usare lo stato operativo; non interpretare i vecchi «assente» come fotografia del codice aggiornato.

# Parità creativa legacy — audit dei sorgenti, 27 ottobre 2026

## 6novembre — checkpoint010748Z,60/60PASS158Rust

Regista/Calliope audio-first recuperato operativamente per master indipendente, segmenti/range, shot collegati, continuità/assistente revisionabile, generazione preesistente e timeline comune.4estratti autentici/8shot fino a MP4/reopen verificati; proposta assistenteSIMULATA, nessuna qualità trailer reale certificata. Direct→Master evita ducking/classificazione nascosta del mix. Rimangono eventi sonori/shotplanning completo, referenceComfy temporali e trailer autentico completo. Versioni timeline esistenti non includono ancora narrative.json: estendere lo snapshot comune, non creare altra cronologia.

Dossier aperto aggiunge consegna fra applicazioni senza un importer come sostituto della funzione: HTML/TXT/CSV/JSON/ZIP, media opzionali/hash, grafi associati/configurazioni/istruzioniAE. Nessun AEP o equivalenza grafica AE promessa; static graph non è preflightlive né prova semantica. Album/copione/trecapitoli già usabili con rifinitura manuale, guida compleanno pronta. Arte/ascolto/portable finale/licenze restano gate. W3storico non rieseguito, stable intatta; dettagli e prossime dipendenze nell’handoff.

## Copione fotografico — checkpoint220617Z

58/58PASS155Rust sul candidato5666CD04…F453F8DD, W3render preservati. Recupero narrativo Album: cue per immagine/editID, capitoli e tempi attuali, testo/regia, istruzioni/cache/proposte selettive/lock, exportbozza senza accettazione, legacycopione intatto, versioni/restore/reopen e cambio progetto operativo nel motore comune. Nessun nuovo planner/render/runtime.

Qualità autonoma IA NON accettata: provaQwen2cue reale introduce intenzione non dimostrata e testi eccessivi. Otto cue completi ora disponibili come revisione autoriale dell’assistente, non successo automatico del modello o approvazione umana. Audio-firstRegista/Calliope6A.3/6A.4 resta CORE da completare (master/eventi/shot e reference audio a intervalli); licenze e portable non chiusi. Non dichiarare parità totale o1.0pronta.

Checkpoint corrente201044Z **57/57PASS154Rust**, SHA272A9F1D…BAC294A, W3render esclusi/preservati. Grandi album PASS_TECNICO121foto sintetiche, cache/gerarchia/revisione/121clip/versioni/restore/reopen nel motore comune. QA UI autentica separata senza nuove inferenze. Qualità artistica e testo narrativo completo per-foto, associazioni confermate e audio-first6A.3/6A.4 restano CORE aperti; non è parità totale o promozione1.0. I paragrafi “in corso” sotto sono cronologia.

## 6A.6 — nuova tranche grandi album, 5 novembre

La lacuna80/64k è sostituita da gruppi a budget e sintesi gerarchica dello STESSO planner, con riferimenti individuali e cache per gruppo, modalità offline, revisione di capitoli/copione, omissioni recuperabili e selezioni protette. Ricevute foto/epoch compatibili con versioni nominate; non nuovo motore. Mirato121 JPEG sintetiche/provider simulato fino a121 clip/versione/restore/reopen PASS (20-09-44),154unit PASS; nuova regressione57check in corso201044Z. Qualità narrativa reale su raccolte ampie e accettazione artistica ancora aperte. Audio-first6A.3/6A.4 resta parziale: binding reference su file interi, intervallo voce già reale; analisi eventi/segmenti/shot plan e reference audio parziali restano da completare. Nessuna parità totale.

##5novembre — stato successivo, senza dichiarare parità totale

Checkpoint finale094041Z56/56PASS150Rust sul candidato47AF7D94…85A5E10, esclusi W3render precedenti. RealiA→BWhisper/Qwencached ealbumUI→export/reopen verificati; omissioni/duplicati hanno recuperonativorevisionabile, rawconservati. Non chiude audio-firsttrailer/grandialbum/paritàtotale. Ascolto/accettazioneartistica/portablefull restanoaperti.

-6A.2: profilo/default applicati nella pipeline guidata, concatenazione adapterQwen→ClipsAI e riuso rawoffline, dipendenze da raw/job/snapshot/profili ritirati. Deposito privato SHA/ID/hardlink/inuso/preview/conferma; eliminati solo bytefixture. Prova nuova WhisperA→B bloccataRAM, licenze/runtimeinventory completo ancora aperti. Non eliminare installazioni esterne.
- Regista audio-first: intervalli originali editabili per scena, anteprima/assembler/compositor comuni, firma continuità sensibile al cambiointervallo. Mirato autentico2s e compatibilità narrativa PASS. Non equivale al trailer segmenti→sounddesign→shotplan→generazione completa.
-6A.6album CORE: nuova capacità integrata nel MontagePlanner esistente, non duplicazionelegacy. Profilo utente/annotazioni, EXIF/date incerte, SHA/dHash/duplicati, osservazioni per foto/cachemodello/pausa/errori, revisione mandatory/excluded, copione con cue, native timeline/versioni/export/riapertura. Test tecnico8foto autentiche+duplicato PASS; 8inferenze visive REALIqwen3.5:9b concluse,1cachehit sulduplicato,0erroritecnici (non0errorivisivi!). QA ha trovato imprecisioni e conserva raw/correzioni separate. Gemma4solo capabilitiesAPI, nessuna prova di qualità inventata.
- Sintesi gerarchica dei grandi album e narrazione generativa revisionabile restano incomplete; cap200manualerimosso, budget80/64k della singola propostaLLM da superare strutturalmente. Regia artistica e accettazione utente ancora aperte. Non promuovere1.0.

## Checkpoint54/54 —29ottobre

Completo000058Z54/54PASS144Rust595,49s sul candidato5310D014…6FC0991: W3 **PASS_TECNICO/PASS_UX**, ducking reale e finalizzazione opzionale condivisa preview/export, non mera dichiarazionelegacy; ascolto umano aperto.6A.2 **PARZIALE operativo**, catalogo/selector/raw/versioni dallaUI con testSRT/export/grafica/restart, ma cleanupowned/provenienza completa/A→B inferita/defaultguidato ancora mancanti.6A.3/6A.4audio-first CORE successivo, non quattro scene forzate. Nessuna parità totale o promozione1.0. Le vecchie sezioni IN CORSO sotto conservano la cronologia, non prevalgono suquesto checkpoint.

##6A.2 —29 ottobre, capacità reale non parità totale

Catalogo/installati da Impostazioni, default/progetto, inventario byte e protezione modelli incerti ora operativi; scelta raw separata da prossimaASR e versioni preservate. Catalogo deriva dalle configurazioni reali e registrazione esplicita, non inventa varianti o adapterVosk.7unitASR e browserfixture/import/restartPASS, senza inferenze nuove. Cleanupesclusivi/ownership completa e provaA→B inferita ancora mancanti; audio-first6A.3/6A.4 restaCORE successivo. Checkpoint corrente/handoff prevalgono sui vecchi stati sottostanti.

## W3 —29ottobre, confronto effettivo

ZPStable `mix.py` ducking applied=False NON certificava DSP; `final_output.py` misura/calibra realmente AAC, docstring obsoleta. Recuperate capacità nel motore comune Rust/FFmpeg: sidechain opzionale con routing/automazioni, EBU R128 PCM/AAC, master opzionale e limiter esplicito. Mirati23-30 PASS, completo233045Z in corso. Misure e ascolto in V1_TEST_MATRIX; PASS tecnico/UX mirati, NON accettazione umana né parità totale.6A.2 e audio-first ancora successivi, non completati da questo intervento.

## Nuove specifiche integrate —28ottobre, dopo50/50

**Checkpoint concluso successivo:202955Z52/52PASS136Rust571,10s**, dev1A6E7F7D…396AEDAB, nessuna produzione successiva. Questa regressione comprende realmente nuovo layout, temi e mixer. Bus/routing/mute/solo/automazioni native ora PASS_TECNICO end-to-end; NON più mancanti come nelle vecchie righe storiche. Ducking reale e finalizzazione ZP rimangono MANCANTE/PARZIALE, non implicitamente recuperati dai bus. Palette/temi PASS tecnico, QA screenshot senza accettazione umana universale.3D escluso, speed Regista/ASRManager/audio-first/capability/scalabilità e gate finali restano aperti. Evidenze precise nell'handoff e nella matrice test; stable e progetti personali intatti.

Aggiornamento effettivo di questa tranche: palette reale verificata da `K:/ZZSitoWeb/Materiali/LogoMio.svg` (589999B, solo lettura), temi UI Scuro/Chiaro/Sistema implementati, preferenza salvata e output immutato nel test con media autentico. Prima tranche mixer ZP recuperata nativamente: bus Voce/Musica/Effetti/Master, routing, mute/solo e automazioni, stesso contratto dB in WebAudio/FFmpeg; snapshot nuovi e storici compatibili. Test `audio-mixer/2026-10-28T20-19-40-856Z/report.json` PASS, DSP/export differenze misurate sotto0,034dB sul tono di misura, NLE reale collegato. NON significa ducking o finalizzazione migrati: entrambi ancora aperti, così come ascolto artistico, speed/time-map e trailer audio-first. Nuovo completo dopo gli ultimi fix di layout ancora necessario; righe sotto descrivono il punto precedente, non prevalgono su questo aggiornamento.

Fonte: roadmap aggiornata6A.1–6A.5 e richiesta esplicita del proprietario, NON nuove app o rinvii alla1.1. Stati iniziali da confrontare con codice, non promesse di supporto:

| Capacità CORE1.0 | Stato prima della tranche | Dipendenza e prossimo intervento |
|---|---|---|
| Palette LogoMio.svg + Scuro/Chiaro/Sistema | PARZIALE: tema scuro/token precedenti | W1, trovare asset reale; temi solo UI, non video |
| Backend/modelli ASR da Impostazioni | PARZIALE: adapter e provenienza esistenti | Inventario reale, switch senza perdita raw/cue, ownership e pulizia solo fixture/esclusivi verificati; mai cancellare shared/esterni |
| Trailer audio-first, quattro segmenti | PARZIALE: reference/scene/assistente e timeline audio esistenti | W3 comune → segmenti/eventi/evidenze → shot plan revisionabile e binding → Comfy; quattro file non impongono quattro scene |
| Character sheet/ambiente/audio reference con offset | PARZIALE: libreria/binding tipizzati | Distinguere ruolo semantico da semplice IMAGE/AUDIO; mapping dichiarato/adapter testato, nessuna identità garantita |
| Capability/settings Comfy da grafo/nodi/GPU | PARZIALE: controlli/doctor/preflight esistenti | Integrare semantica e range reali senza primo checkpoint casuale, impostazioni consigliate con provenienza e override supportati |
| Foto/video/asset/musica → storia e lunghissimo | PARZIALE: planner/narrativa/cache/baseline1–2h esistenti | Un solo progetto; audit limiti attuali, virtualizzazione/chunk e checkpoint, non nuove inferenze integrali |

Implementazione attiva: ergonomia W1 e mixer W3; gli altri requisiti rimangono aperti e vincolanti. Nessun download o pulizia di modelli reali autorizzata dai test.

## Stato verificato finale della tranche —190108Z

**Nuovo completo50/50PASS132Rust,558,79s**, dev7871C09C…F4D498B; nessuna produzione successiva. Versioni nominate native e navigazione sono PASS tecnico end-to-end, incluso export/restart, non più soltanto strutture previste. Snapshot dati/media condivisi, backup del corrente prima del ripristino e Undo distinto. Mirato finale nel completo: edit-versions/2026-10-28T19-06-14-734Z. Fonte/capacità legacy sotto restano distinte: NON parità totale con i quattro originali.

Conservati completi183516Z48/50FAIL e185028Z49/50FAIL; fix ora verificati nel nuovo giro. Raggiungibilità strumenti e desktop/mobile testati; layout NLE ancora troppo verticale e UX umana da accettare (W1). Bus mixer/master W3, ducking reale facoltativo e speed/time-map W2 ancora lacune, non rinvii approvati o funzioni completate. Origini/progetti/stable invariati. Nessuna promozione1.0.

## Consolidamento effettivo — 28 ottobre, dopo la navigazione

Questa sezione prevale sulle vecchie righe «nessuna produzione successiva». Nuove versioni integrate, NON nuova parità totale. Completo corrente `20261028T183516Z` in esecuzione; 49/49 precedente storico.

| Capacità / fonte verificata in sola lettura | Destinazione e stato effettivo | Prova / lacuna residua |
|---|---|---|
| ZP `src/version/store.py`: VersionStore, save_version_as, salvataggio atomico | **PASS tecnico mirato**: Montaggio → Versioni…, snapshot dati + asset condivisi, confronto/rinomina/copia/ripristino con backup automatico | `edit-versions/2026-10-28T18-34-39-381Z/report.json`: UI, export 2 s e restart; nessun master duplicato. Non importa automaticamente tutti i vecchi VersionDescriptor ZP; versioni native nuove, Undo distinto |
| ZP Stable `src/audio/mix.py`: build_mix_plan, mix_filtergraph, render_mix | **PARZIALE**: Suite ha volume clip 0–4, mute tracce e fade audio reali in preview/compositor comune | `compositor.rs` volume/afade/amix normalize=0; `nle.js` gain WebAudio. Routing Voice/MusicFX/Master con guadagni di bus e solo ancora mancanti: non equivalenti al solo volume clip |
| Ducking ZP | **PIANIFICATO NEL LEGACY, NON OPERATIVO** | `mix.py` relations[].applied=False e processing vuoto; `final_output.py` master_chain ducking.applied=False, OPTIONAL_NOT_INCLUDED include ducking/leveler. Nessuna pretesa di migrazione di una funzione mai applicata. W3 richiede ducking facoltativo reale, da implementare nel motore comune |
| Finalizzazione ZP Stable | **MANCANTE nella Suite** come controllo loudness/true-peak editoriale | La docstring iniziale è obsoleta: `final_output.py` measure_loudness, master_processing, calibrate_master, build_finalize_plan realmente calcolano guadagno, oversampling/limiter e misurano l'AAC consegnato, impostando applied=True quando measure/delivered disponibili. Target legacy −14 LUFS/−1 dBTP non va imposto a tutti. Non confondere bitrate AAC della Suite con finalizzazione loudness |
| ZP prova vs Stable, montaggio/KEEP/proposte | **PRESENTE/PARZIALE**, non due motori da incorporare | 73/71 sorgenti, 17 differenze normalizzando CRLF, 56 identici. Stable-only audio/mix.py e media/final_output.py; altre differenze in renderer, trascrizione, server/draft/operations/resources/timeline/capabilities e frontend. Suite conserva edit/ripple/approvazioni, NLE, candidati e bridge; editing UI di intervalli KEEP multipli da raffinare, non dichiarato equivalente a ogni operazione ZP |
| Regista `_render_timeline_engine` in utility/RegistaAI/server.py | **PARZIALE**: pan/zoom/fade/transform/audio e sequenze comuni presenti | Source speed0,25–4 instradata a setpts/atempo: Suite non ha ancora time mapping a velocità variabile e CC conseguenti. Import blocca esplicitamente trasformazioni non supportate; nessuna perdita silenziosa. Il cablaggio legacy non equivale a nuovo collaudo runtime del legacy |
| Calliope agent/continuity.py / routers/story.py | **PARZIALE**, capacità native utili già integrate | Suite narrative_assistant.rs contiene Continuity/Direction/proposte localizzate e controllo fingerprint; narrative_references.rs libreria owned e link scena. Non è migrazione integrale dell'agente/shot planning/basis_hash Calliope. 3D intenzionalmente escluso dall'utente; non reintrodurlo |
| ClipsAI motore.py / sgrossatore_raw.py / taglia_clip_per_editing.py | **INTEGRATO con percorso avanzato conservato** | Longform/Shorts/subtitles/export nativi + Pipeline ClipsAI · avanzata contestuale nel Montaggio. bad-takes in due fasi e percorsi originali non sono automaticamente identici alle euristiche native. ASR/Gemini2 e risultati esistenti riusati, non rieseguiti |
| Accessibilità delle capacità | **PASS mirato**, nuova regressione in corso | Quattro aree + utility; dieci destinazioni provate con veri click, selezione clip e progetto conservate, libreria DOM unica; export modale usa il medesimo profilo. Shorts ora espone contain/cover/blur/auto già presenti nel compositor |

Prossima lacuna concreta dopo W0/versioni: **W3 audio comune**, mixer e finalizzazione opzionale con misure/ascolto; non avviare un altro prodotto audio. W2 speed e le capacità narrative avanzate richiedono completamento mirato, non una falsa dichiarazione di parità grazie agli importatori. Accettazione UX umana e distribuzione/licenze ancora aperte.

## Ultimo esito — nuovo completo49/49PASS128Rust,28ottobre2026

Baseline151530Z conclusa573,17s, dev9D8D3C42…C5FD0C8; nessuna produzione successiva. La correzione percettiva sotto è ora collaudata tecnicamente con UI reale→export→riapertura. Sfondo sincronizzato/foreground editabile, camera composta/fissa e fallback Auto, tre composizioni native collegate ai nuovi titoli del sequencer; pipeline unica, nessun nuovo modello/agente. Vecchi progetti/manualità preservati.

Confronto finale152225Z: quattro MP46s, due registrazioni Programma1×,180frame MP4/0drop;179frame sorgente per preview, MAE1,695–3,834, vocePCM0,99999048. Quattro progetti riaperti. CONFRONTO.html con baseline contain/titolo ricostruita, stesso cachedCRF20, limiti espliciti. Primo completo150331Z47/49FAIL conservato, due harness corretti prima del nuovo giro intero. Nessuna parità artistica universale dichiarata: l’utente deve valutare visivamente le alternative,1.0 non promossa. Stable/settings/personali hash invariati; nessuna inferenza/download.

## Correzione percettiva —28ottobre2026, non nuovo planner

Utente NON soddisfatto del risultato precedente: il PASS tecnico non è accettazione artistica. Nuovi modi fit contain/cover/blur/auto nell’NLE/compositor esistenti, sfondo sincronizzato dalla stessa sorgente, foreground editabile; Camera composta v3 con lock per shot, dead zone e fallback motivato su perdita/ambiguità. Campionamento con PTS decodificati; nessun nuovo modello/servizio. Vecchi progetti e curve preservati.

Tre composizioni tipografiche native originali, factory deterministica con testo revisionato, gerarchia/contrasto/misure dei glifi/linee proporzionate; montaggio titoli integrato, selettore e protezione di titoli già modificati. Nuovi elementi offrono auto; non migrazione distruttiva. Monitor ingrandito dopo lo screenshot reale.

Mirato145820Z10/10PASS128Rust; quattro MP4 autentici6s, 180frame riprodotti a1× senza drop, WebM del canvas per cover/blur, MAE1,70–3,84 e riapertura vera. Nuovo completo49check150331Z in corso per includere ultime rifiniture: NON certificare con48/48 vecchio. Dettagli/limiti in documentazione/QA_VISUALE_VERTICAL_20261028.md. Accettazione estetica umana ancora necessaria, nessuna parità artistica universale dichiarata.

## Stato verificato finale28ottobre2026 — planner semantico

**48/48PASS125Rust478,53s**,baseline140645Z,devSHAE78AF948…69E496. Recupero tecnico del percorso creativo confermato nel motore unico: proposta revisionabile→ricevuta→timeline/Motion nativi→monitor/export→reopen. Contratto fonti/decisioni separato,riepilogo dai dati,audio originale conservato,titolo/ragioni mancanti bloccati;avvisi su rischi osservabili e commenti modello non presentati come QA. Vecchi progetti compatibili,manualità e continuità opzionale conservate.

Confronti8B/LFM recuperati,UNA nuova prova LFM del contratto72,480sCPU: campi migliorati ma scelte semantiche/artistiche ancora da revisionare. Nessun modello maggiore o nuovo download. Due risposte LFM diverse rigiocate nel completo senza inferenze;contenuto/ordine/titoli/tempi corretti dal revisoreTEST esplicitamente,non attribuiti all'IA. CorrPCM0,9999779,MAE finale4,373/1,725/2,584;progetto e video ispezionabili dal confrontoHTML.

La parità tecnica di questi strumenti NON certifica l'intero agente Calliope/Regista o buona regia automatica universale. Accettazione umana/qualità narrativa e gate finali1.0 restano aperti. Stable/originali/personali/servizi preservati;camera/export non reimplementati. Le note IN CORSO seguenti sono cronologia.

## Aggiornamento planner semantico v3 —28ottobre2026

Ultima correzione: riepilogo/controlli app non includono più warnings liberi del modello. La risposta rimane consultabile con provenienza non verificata. Mirato140449Z7/7PASS125Rust; completo finale48check140645Z in corso. Non usare i giri interrotti come certificazione.

Verifica successiva:47/47PASS124Rust134456Z, poi nuova prova LFM del contratto cambiato72,480sCPU. Ragioni/titolo/audio compilati ma regia non affidabile; nessuna escalation automatica di modello. Aggiunti avvisi deterministici su trim still, titoli multipli e confini audio, non un secondo motore o blocchi estetici arbitrari. Replay nuovo raw140130ZPASS→revisione→timeline→MP4→restart,corr0,9999779;125Rust. Nuovo completo48check in corso,47precedente non copre gli ultimi avvisi. Nessuna parità artistica universale dichiarata.

Il motore resta unico. Recupero risposte8B/LFM già calcolate: inventario più completo ma riepiloghi incompatibili con ordine/audio/titolo e motivazioni mancanti. Nuovo livello di revisione NON è un nuovo agente/motore: canonicalizza il riepilogo dai dati, tutela audio originale, espone campi mancanti e interpretazioni, richiede correzione esplicita prima del merge. Titoli Motion e scelte native restano editabili; ricevuta approvazione serializzata e compatibilità vecchi progetti. Maschere di revisione e continuità opzionale conservate.

ReplayLFM v3 `133347Z` PASS tecnico senza inferenza,PCM0,9999779 e riapertura reale. Non equivale a recupero del giudizio artistico umano o dell’intero agente Calliope. Completo vecchio102218Z46/46PASS120Rust recuperato, nuovo completo dopo queste modifiche ancora da chiudere. Camera/export già collaudati: nessuna reimplementazione in questa tranche.

## Aggiornamento qualità28ottobre2026 — NON rifare l’audit

Camera: aggiunta traiettoria source-time editabile nel compositor/NLE esistenti, CPU Haar/cache e controlli stabilizzazione/reattività; non semplice ease per frame. Vecchi crop invariati. I punti/cut e i diagnostici persistono; nessuna identità ricostruita. Passaggio proposte/adozione/trim/export/riapertura collaudato in reframe_export; stacchi4Hz e soggetti ambigui richiedono revisione, non parità universale con tracker avanzati.

Delivery: profilo Social esteso a qualità/codec CPU e NVENC opzionale, capacità compilate distinte da probe operativo, fallback esplicito, fps razionali/custom e stream misurati. Export dagli asset originali, senza proxy substitution. A/B autentico breve H264/HEVC/AV1; hardware non collaudato sul driver condiviso. Nessun secondo motore o dipendenza.

Planner: contesto v2 separa evidenze/affermazioni/ipotesi, evita reason/emotion offline riciclate e reference non approvate, note utente preservate. Maschera revisioni selettive tono/tempi/ordine/stile/titoli/audio applicata dal server; scelte protette intatte. Analisi causale dettagliata in documentazione/QA_PLANNER_CAMERA_EXPORT_20261028.md. Efficacia artistica della nuova inferenza ancora da provare; non basta il replay.

Nuova regressione46check in corso: primo giro095812Z già FAIL narrativaSRT per sovrascrittura metadati, fix merge implementato ma non ancora certificato. Il44/44 precedente non copre queste modifiche. Fare riferimento agli headerV1 per l’ultimo giro chiuso, non alla cronologia sotto.


## AI Montage Planner — aggiornamento28ottobre2026

Recuperato il passaggio strumenti→proposta creativa revisionabile: src/montage_planner.rs + web/montage_planner.js, integrati nel sequencer e nel compositor esistenti. Note/media/campioni visivi approvati/ASR cached a intervallo + musica/voce/istruzione → arco, motivazioni, emozioni, ordine/durate/trim/movimenti/raccordi, titoli Motion nativi e risorse mancanti distinte. Provider story già configurato; offline esplicitamente non semantico. Continuità narrativa opzionale, dati/reference stantii rifiutati.

Scelte manuali/approvate protette, rigenerazione su selezione, slot non scelti invariati; omissione non significa cancellazione. Baseline server-owned conserva modifiche e rimozioni native per più riapplicazioni, anche audio; titoli editati non sovrascritti. Analisi visiva opzionale max12campioni, cache/raw incrementale e consenso, helper condiviso con Storia; NON riconoscimento dell'intero video, EXIF o clustering automatico. Nessun nuovo modello/runtime o motore di montaggio.

**Evidenza reale e limite qualitativo:** una inferenza llama3.1:8b-instruct-q4_K_M già installato, CPU111,028sUI (110,887sprovider,1612input/465output), raw conservato. Risposta artistica insufficiente:1foto, ma riepilogo afferma video/titolo/9s. Non certificata. Aggiunti controlli locali che espongono durate strutturate/omissioni/assenze e motivazioni offline ricopiate, non un critico semantico infallibile. Replay della risposta salvata con revisione esplicita del test→2foto+video+musica→titolo Motion editato→MP48,6s→reopen: PASS tecnico, **accettazione umana false**, limiti estetici nel visual-review.json del pacchetto.

Mirato con guardia UI save/inference PASS: montage-planner/2026-10-28T09-02-11-067Z/report.json. Il completo precedente084833Z è **43/44FAIL** per la race di salvataggio/narrazione: controlli ora protetti da fieldset nativo e cambio progetto rifiutato mentre occupato; NUOVO completo090335Z **44/44PASS,117Rust,387,79s**, devSHA8C8B0228…C68ABB0; non accettazione artistica. Questa nota non sostituisce i risultati finali nei documentiV1.

## Aggiornamento implementativo — 28 ottobre 2026

La tabella seguente resta l'audit **iniziale**, non lo stato finale. Questa tranche recupera capacità native e un percorso incrementale, NON l'intero agente Calliope o la parità totale.

| Recupero | Codice/UI attuale | Evidenza nuova | Limite residuo |
|---|---|---|---|
| Fade Regista | VisualTransform fade_in/out e audio_fade_in/out; inspector → compositor comune | creative_montage: edit/undo/redo, dissolvenze sovrapposte, PCMexport e restart | Vecchio importatore non assume automaticamente parità di ogni trasformazione Regista |
| Camera fotografica | zoom_start/end e pan_x/y proporzionali; canvas RGBA fisso + affine, non deformazione volti | 5campioni reali MAE0,27–7,20<12; finestra6,9s MAE4,52 | Non tracking/face-aware automatico; preview e output richiedono revisione estetica |
| Sequenze miste | Montaggio → Foto/video/musica; import multiplo, note, ordine, sezioni, durate, trim/movimenti/fit, proposta → commit con undo | 2foto autentiche + video cached6s + musica sintetica → MP47,7s → reopen/mobile | Non ancora planner IA semantico per50foto, EXIF, duplicati/similarità o collage automatici; titoli si aggiungono nel Motion esistente |
| Ritmo audio Regista | montage_audio.rs + montage_rhythm.js; analisi CPU20ms mono8k, ascolto, marcatori correggibili, griglia manuale e snapping | 31attacchi sul test120BPM; cache; reale PCM mix/fade corr0,99998; mixautentico6s nel nuovo completo | Attacchi energetici NON downbeat/sezioni semantiche: metà/doppioBPM e falsi positivi possibili; accuratezza umana null |
| Sviluppo integrativo / chiudi buchi | narrative_assistant.rs/js: stato scene/materiali/bible → patch ristrette → confronto e scelta → salvataggio/revisione/produzione comune | Provider simulato: nuova scena e followup sullo stessoID; reale llama3.1:8b-instruct-q4_K_M CPU126,21s → bozza → mediaesistente → MP4/reopen | Non ciclo agentico generale, gerarchia scene/shot indipendente o critica semantica infallibile. Ogni scena è un'unità produttiva; piùshot richiedono piùscene |
| Continuità Calliope | Bible stile/epoca/fatti/props/camera + act/camera/luce/tempo/abiti/props/stato prima/dopo per scena; reference esistenti | Persistenza, prompt di produzione vincolati, modifiche revocano approvazione visual; contesto stantio rifiutato | Non identità generativa garantita né confronto visuale automatico; il vecchio planner sostitutivo non equivale al nuovo assistente |
| Cache e sicurezza creativa | Raw risposta salvata prima della validazione, chiave contesto/richiesta/provider/modello; job seriale; dati server-owned | creative_cache:1chiamata per2richieste, rawidentico; null-stringnormalizzato, IDestraneo/stale/doppiaapplicazione rifiutati | Cache locale perprogetto, non promessa di recoveryuniversale. Nessuna generazione Comfytestata in questa tranche: serviziooffline |

**QA reale dell'assistente:** PASS tecnico non equivale a buon soggetto. Nel campione reale il modello scambia la reference per un ritratto dentro la storia, scrive i prompt in italiano e propone parlato eccessivo per2s; alternative poco pertinenti. Non applicare automaticamente tutte le proposte. UI ora segnala densità narrativa e campi tempo/durata sospetti tramite indizi locali; non è una critica semantica. Resta revisione umana e ulteriore miglioramento di proposta/shotlist. Nessuna approvazione artistica inventata.

Regressione generale **43/43 PASS,111Rust**,v1-baseline/20261028T002937Z/report.json,349,10s. Primo giro42/43 conservato,non rinominatoPASS; race del testCC corretta prima di nuovo giro intero. Stable/settings/3progetti personali invariati perhash. Coreportable nuovoPASS; il portable COMPLETO precedente NON certifica questi sorgenti. Artefatti ScenariA/B riapribili con piccoli media necessari, non nuova copia runtime/modelli, non accettazione artistica finale.

Stato iniziale della tranche: sviluppo 0.7.1, EXE SHA256 `2F95459818AD42382759081A41B89C0E2C065917FC781AC45234049567D4A40A`, ultima regressione 40/40 e 103 Rust. Non certifica modifiche successive. Originali consultati soltanto in lettura; nessun avvio dei vecchi programmi. Le istruzioni AGENTS dei tre progetti che le possiedono sono state consultate. Nessun AGENTS trovato nella suite o in Calliope.

Percorsi abbreviati: R=`K:/AI/Regista_AI/utility/RegistaAI`; C=`K:/AI/Calliope/calliope-backend/src/calliope`; L=`K:/AI/ClipsAI`; Z=`K:/AI/ZioPaolo Video/ZP-Shorts-Studio-Stable/src`; S=`sviluppo`. È un audit statico dei sorgenti, non un test d'esecuzione dei legacy. Le descrizioni nei README/AGENTS non costituiscono da sole evidenza operativa.

| Capacità | Evidenza originale / funzionamento e dipendenze | Suite / accesso UI effettivo iniziale | Parità iniziale e gap concreto | Verifica end-to-end richiesta |
|---|---|---|---|---|
| Timeline multitraccia | R/timeline_engine.py:38 Clip, :181 TimelineEngine; JSON, trim/split/duplicate/snap, Python stdlib | S/src/editor.rs + web/nle.js; Montaggio, monitor e inspector | Presente con test NLE; non equivalenza totale dei comandi multipli | Import, drag/trim/split, undo, export/reopen |
| Trasformazioni e ordine livelli | R/server.py:132 `_render_timeline_engine`, scala/posizione/opacità e tracce; FFmpeg comune preview/export | S/src/compositor.rs, model::VisualTransform; inspector | Presente; crop/focus/rotazione aggiunti. Velocità legacy non supportata dal contratto della suite | Frame di più livelli, coordinate/alpha, preview/export |
| Fade regolabile per clip | R/server.py:207–222 fade_in_sec/out_sec, R/web/app.js `reelClipFadeFactor` | S/compositor.rs fade fisso 350ms con motion, selettore inspector | **Parziale**: manca durata separata ingresso/uscita; importer rifiuta correttamente dati non rappresentabili | Fade non standard, sovrapposizione, export parziale e riapertura |
| Speed | R/server.py setpts/atempo, Clip.speed | Nessun controllo nativo equivalente, importatore rifiuta | **Assente**, non migrazione nominale | Implementare semantica source-time + CC prima di esporre controllo |
| Asset e reference | R/project_manager.py e server.py, file in input; C/routers/library.py e assets.py | S/narrative_references.rs + web/narrative_library.js; Storia → libreria | Presente: materiale/personaggio/ambiente/stile/voce; note/analisi e scelta input espliciti | Import multiplo, assegnazione/approvazione, slot Comfy, reopen |
| Storyboard revisionabile | R/agents.py:735 StoryboardAgent, :1231 `_build_scene_entries`; preview→apply-scenes | S/narrative.rs plan/save + web/narrative.js | **Parziale**: scene revisionabili, ma rigenerazione sostituisce la bozza, non propone patch contestuali | Nuova scena e sviluppo di scena esistente senza perdita media |
| Chiudi buchi senza sovrascrivere | R/agents.py DirectorFillAgent e merge storyboard, `_has_real_content` | Solo editing manuale / nuovo plan | **Assente come assistente UI**; non basta endpoint story/generate | Proposta contestuale→scelta patch→conferma→scene→render |
| Assistente con strumenti | R/director_conversational.py:497; C/agent/harness/plugins/story.py CRUD beat/reference, script.py CRUD scene | Provider ruoli + plan e produzione, non loop creativo completo | **Parziale**. Recuperare capacità utili con operazioni ristrette e revisionabili; non shell/autonomia distruttiva | Richieste successive, stato progetto reale, alternative, applicazione selettiva |
| Storia/atti/scene/shot | C/authoring/script.py, agent/script_agent.py, plugins/script.py, routers/shots.py; DB SQLite | Story legacy + Production scena=unità produttiva | **Parziale**: manca gerarchia atti/shotlist e sviluppo integrativo accessibile | Idea→scaletta→shotlist editabile→media→montaggio |
| Registro continuità | C/agent/continuity.py:60 PLAN, :90 CRITIC, :490 ensure_continuity_plan, :539 continuity_lock_text; hash board/refs, LLM | S/narrative_references.rs:346 bound_scene_prompt | **Parziale**: reference collegate/validate, manca bible condivisa e raccordo fra scene | Personaggio/abiti/oggetti/luce/fatti persistiti; modifica invalida approvazioni; prompt e confronto |
| Character sheet / sfondi | R/agents.py + timeline_manager.py, workflow scelto secondo reference; C/agent/prompts.py | Libreria narrativa con prompt/workflow/asset_inputs e revisione | Presente tecnicamente, qualità identità non garantita. Niente immagini senza reference spacciate per consistenti | Reference approvata→workflow compatibile→revisione output→scena |
| Produzione sequenziale | R/director_conversational.py RenderQueue raggruppata tipo/workflow; Comfy API | S/narrative.rs generate, generation.rs, job persistenti e recovery | Presente, seriale prudente; non recuperare stop/unload globali legacy | Input reali, output al proprietario, no duplicazione dopo restart |
| Film da audio / ritmo | R/audio_story_pipeline.py:15, analyze_audio; audio_analyzer.py beat/prosodia/ASR→scene | Audio import/ASR/reference esistenti, nessun planner ritmico montaggio | **Parziale/assente** per musica→montaggio; non attribuire semantica musicale a picchi waveform | Analisi beat esplicita o griglia manuale→durate variabili→mix/export |
| Montaggio molte foto/video | R/web/app.js asset gallery e scene dagli asset; timeline scene modificabili | Import multiplo in libreria, inserimento singolo in NLE | **Parziale**: manca sequencer revisionabile, ordinamento, durate/ritmo e duplicati | Scenario A autentico, foto+video+musica→proposta→timeline editabile |
| Pan/zoom fotografico | R/audio_story_pipeline.py default camera/zoom descrive generazione; compositor locale non anima scale nel tempo | VisualTransform statico + slide/fade, Motion separato | **Assente come movimento fotografico nativo**; nuova implementazione comune, non deformazione volti | Campioni inizio/metà/fine, contain/cover, stesso movimento preview/export |
| Collage | R multi overlay/posizioni; non rilevato planner automatico collage | Multitraccia/scale/posizioni NLE + motion ibrido | Manuale presente; **manca proposta collage**. Non dichiarare feature legacy più estesa del codice | Due foto su due tracce, manualità e trasparenza conservate |
| Sgrossatura voce rispettata | L/sgrossatore_raw.py:268, 298, 307, 335; word-safe pad, breaths, filler; AI quote→supervisore RIMUOVI/LASCIA | engines/clipsai + bridge + Longform/Analisi | Presente ma due percorsi editoriali non equivalenti; non chiamare euristiche native supervisione LLM | Riuso ASR, piano motivato, ascolto e approvazione, mantenimento performance |
| Reel multi-segmento | L/taglia_clip_per_editing.py:227/274/379; moments→reels→render; shorts.py:105 grouping | Analisi/ClipsAI bridge e timeline; Shorts Studio candidato/range | Presente bridge; nuovo Shorts non dimostra tutta la parità narrativa | Composizione non contigua, quote e time map, CC+render |
| Eventi audio | L/audio_events.py, motore.py opt-in AST | Snapshot engine conserva modulo; disponibilità UI/core da verificare | **Non certificato**, no download/modello automatico | Capability e cache eventi, no nomi speaker inventati |
| Candidati approvati e KEEP | Z/models/short.py derive_keep_segments, compilation/engine.py:99 | S/legacy.rs import; Shorts Studio ranges separati | Import sì; **parziale** editor KEEP/exclusions per candidati importati | Import approvato→esclusioni→tempi CC→export |
| Compilazioni e fingerprint | Z/compilation/engine.py:68 struttura+KEEP, selection/proposal.py:167 | Timeline comune + Shorts, non proposal-version equivalente | **Parziale**; no falsa equivalenza con scoring editoriale | Cambiare confini e riordinare→cache invalida→export/reopen |
| Versioni editoriali | Z/version/store.py save_version/save_version_as, lock/fingerprint | Undo NLE client, storia CC persistente, progetto JSON | **Parziale**: manca sistema versioni compilazione | Due varianti selezionabili, nessuna perdita approvazioni |
| Workflow Doctor | R/workflow_repairs.py + workflow_engine.py + server endpoints | S/workflow_doctor.rs, UI Generazione su copie | Sostituito intenzionalmente: riparazioni deterministiche senza modelli/encoder scelti di nascosto né installazione nodi | Copia diagnosticata→diff→preflight→invio compatibile |
| Grafica/audio/frame→video | R/asset_suggester.py, server.py reel endpoints | Motion/oggetti IA/music/frame a cursore, shared jobs | Presente tecnico; risultati artistici da revisionare | Asset→revisione→traccia corretta→render e reopen |
| 3D / comandi globali servizio | Calliope blockout/scene 3D, R stop-all/unload | Esclusi intenzionalmente | **Sostituiti/esclusi** su richiesta; servizi condivisi intatti | Nessun download/stop globale, uso app senza IA |

## Ordine di recupero

1. Primitive native mancanti (fade separati e movimento fotografico), con conformità monitor/export e finestre temporali.
2. Sequencer foto/video nel workspace Montaggio: proposta editabile, ordinamento/durate/ritmo manuale trasparente, musica e transizioni sul compositor esistente.
3. Assistente incrementale nel workspace Storia: proposte tipizzate sullo stato salvato, selezione/approvazione, continuità e invalidazione; non riscrittura cieca della produzione.
4. Scenari A/B/C con distinguo mock/inferenza reale/qualità artistica, nuove regressioni e gate ancora aperti. Parità totale NON dichiarata.
