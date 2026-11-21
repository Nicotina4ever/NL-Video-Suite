# NL Video Suite 1.0 — specifiche e roadmap unica

Aggiornata il 18 novembre 2026. Questo documento definisce il perimetro completo: gli elenchi sono requisiti, non attestazioni di completamento. Lo stato corrente, le prove e i gate aperti sono in [Stato e roadmap operativa](../../documentazione/STATO_E_ROADMAP_1_0.md). La ripresa pratica è nel [Session Handoff](V1_SESSION_HANDOFF.md). I checkpoint storici non certificano il codice corrente.

## 1. Missione, perimetro e criteri della 1.0

### 1.1 Il prodotto che deve esistere

Un'applicazione **Windows x64, offline-first**, fondata su Rust/Axum, FFmpeg, progetto e timeline condivisi, che consente di:

- **Montare liberamente** foto, video, audio, transizioni, livelli grafici, versioni alternative e produzioni da zero.
- **Lavorare su video lunghi**: ASR, trascrizioni, capitoli, ricerca, rough cut, false partenze e montaggio basato sul testo.
- **Ricavare shorts/reel**: highlight, selezione editoriale, adattamento 9:16, sottotitoli, grafiche, musica, output multipli.
- **Preparare e correggere sottotitoli**, anche plurilingue, con controllo esplicito di tempi, accuratezza, stili e consegne SRT/VTT/ASS/video.
- **Comporre motion graphics native** a livelli editabili, con parametri e keyframe utilizzabili in monitor ed export.
- **Creare montaggi narrativi da foto/video/musica**, con assistenza IA facoltativa ma intervento manuale sempre possibile.
- **Sviluppare storie e produrre scene** tramite soggetto, sceneggiatura, personaggi, reference, continuità, storyboard, Ollama/ComfyUI e media selettivamente approvati.
- **Montare e finalizzare audio** con comportamento non distruttivo e controlli adeguati per parlato, musica, effetti e mix.
- **Preparare output social** con formati, safe zone, metadati e qualità controllabili, senza pubblicazione automatica non richiesta.
- **Funzionare senza IA** per operazioni di montaggio, sottotitoli manuali, grafica nativa ed export.

L'ambizione è ampia: non ridurre il lavoro a un prototipo o a una demo. Le capacità devono integrarsi nel progetto e negli strumenti esistenti; non costruire una mini-app per ogni funzione.

### 1.2 Che cosa NON significa “non tarpare le ali”

La visione 1.0 resta ampia, ma non è un obbligo a implementare *qualsiasi* funzione teoricamente possibile. L'agente deve censire tutto ciò che era promesso/operativo nelle applicazioni originali e proporre la destinazione migliore; il proprietario decide se un'importante capacità debba essere esclusa o rinviata.

Sono **fuori dall'obbligo 1.0**, salvo nuova decisione esplicita: clone integrale di Premiere/After Effects; produzione 3D generalista; training di modelli; pubblicazione cloud non autorizzata; server multiutente; infrastruttura NL Core universale; porting nativo Linux/macOS. Funzioni nate per strumenti audio/REAPER, non per il video editor, vanno valutate, non importate per inerzia.

### 1.3 Cinque livelli di completamento (non confonderli)

1. **Identificata**: requisito, fonte legacy, uso e dipendenze accertati.
2. **Integrata**: dati, UX, operazioni e output collegati al progetto/timeline nativi.
3. **PASS tecnico**: esecuzione e regressione con test riproducibili, save/reopen, compatibilità.
4. **PASS UX**: funzione raggiungibile senza percorsi assurdi, comportamento leggibile, controlli professionali.
5. **Accettata per l'uso previsto**: risultato video/audio/testo realmente soddisfacente, oppure limiti dichiarati e fallback professionale scelto dal proprietario.

Un 49/49 non equivale al livello 5.

---



## 5. Registro delle capacità e parità legacy

**Regola:** una capacità entra nel registro solo con **fonte** (codice legacy/documento/esigenza utente), **destinazione nella Suite**, **stato reale**, **prova**, **prossima azione**. Non chiamare assente ciò che è soltanto nascosto; non chiamare completo ciò che è soltanto importabile.

**Stati consentiti:** `DA_AUDITARE` · `PRESENTE_VERIFICATO` · `PRESENTE_NASCOSTO` · `PARZIALE` · `MANCANTE` · `IN_CORSO` · `PASS_TECNICO` · `PASS_UX` · `ACCETTATO` · `LIMITAZIONE_DICHIARATA` · `RINVIATO_CON_DECISIONE`.

**Priorità:** `CORE_1.0` (necessaria per l'esperienza prevista), `IMPORTANTE_1.0` (fa parte della completezza/qualità del prodotto), `FACOLTATIVA` (non blocca), `FUORI_PERIMETRO`. Non assegnare la priorità per difficoltà stimata, ma per valore e dipendenze; non retrocedere funzioni centrali senza decisione del proprietario.

| Area da mappare | Fonti da ispezionare | Capacità da confrontare | Prima verifica |
|---|---|---|---|
| Montaggio base/avanzato | Regista `timeline_engine.py` + Suite NLE | Clip, tracce, trasformazioni, keyframe, fade, opacità, versioni, trim/split/undo, time mapping | Parità proprietà e flusso UI/export |
| Audio e finalizzazione | ZP `Stable` e `prova` + Suite audio | Mixer, routing, livelli, fades, automazioni, ducking *effettivo*, loudness opzionale | Effetto misurato sul PCM, no claim da sola UI |
| Shorts editoriali | ZP `Stable`/`prova`/altre versioni disponibili + ClipsAI | Candidati, approvazioni, exclusion map, revisioni, batch, hook, montaggi, social | Originale→scelta→export→reopen |
| Produzione media | Regista e workflow ComfyUI | Image/video/audio generation, reference, prompt, workflow doctor, varianti | Output reale e revisione, no provider finti |
| Continuità narrativa | Calliope + Suite | Storie, scene, shot, registro, reference personaggi/ambienti, memory, proposta creativa | Persistenza e cambi controllati |
| Montaggio fotografico | Regista / Suite Creative Montage | Pan/zoom, composizione, musica, beat, titoli, planner, audio originale, varianti | Video completo rivedibile e editabile |
| Longform | ClipsAI + Suite Longform | ASR/aligner, capitoli, false partenze, rough cut, testo-timeline, shot map | Test vero e QA umano |
| Subtitles | Suite/ZP dove pertinenti | Formati, correzioni, stili, trascrizione multipla, QA, approvazione | Confronto lingue/tempi reali |
| Grafica | Regista / Suite Motion | Layers/alpha/maschere/gruppi/path/keyframes/presets | Preview/export e file nativi |
| Export/social | ZP/Regista/Suite | Formati, qualità, preset, audio, reel, thumbnail, versioni | Media originale, qualità, metadata, reopen |
| Navigazione e sistema | Suite attuale | Quattro workspaces, strumenti contestuali, single media library, jobs/providers/diagnostica | Task-based UX a più risoluzioni |

**Audit legacy obbligatorio:** confronto anche della variante ZP `prova`, non soltanto `Stable`; separare *codice che esegue* da *UI che dichiara*. Non modificare o cancellare gli originali. Compilare/aggiornare `V1_LEGACY_PARITY.md` con percorsi, funzioni, frammenti o simboli verificabili e decisioni.

---



## 6. Roadmap tecnica per workstream

**Le aree seguenti sono una mappa di dipendenze, non una sequenza rigida di giorni o numeri fissi di ticket.** L'agente seleziona il prossimo lavoro in base a valore, blocchi e rischio, mantiene autonomia per scoprire lacune e può proseguire su altre aree quando un gate richiede feedback umano.

### W0 — Stabilizzazione immediata della nuova UI (prima di nuove feature importanti)


- Esaminare i 51 file modificati durante l'interruzione; verificare sintassi/build e impatti su progetto attivo.
- Confermare quattro aree con mappatura completa di ogni strumento preesistente e nessuna funzione perduta.
- Testare nuovo selettore di workspaces, barra contestuale, libreria condivisa, accesso a export, job/settings.
- Controllare a 1920×1080, 1366×768, zoom/DPI e viewport ridotte, senza pannelli fondamentali invisibili.
- Controllare che cambio progetto/workspace non lasci media o transcript del progetto precedente (race già osservata storicamente).
- Registrare il nuovo risultato di regressione; non chiamarlo 49/49 se il codice è cambiato.

**Gate W0:** si completa un edit semplice e si raggiungono tutti gli strumenti avanzati, senza perdita di stato; i report verificano le nuove modifiche.

### W1 — Architettura delle operazioni e UX professionale

**Nuovo requisito:** tema applicazione Scuro / Chiaro / Sistema, derivato dal vero `LogoMio.svg` con token viola `#7531FE`, magenta `#E4469B` e lilla `#AD85C7` (le sfumature del logo non sono una palette obbligatoria per ogni controllo). Preferenza persistente, auto-follow del tema di Windows e compatibilità leggibilità/contrasto; identità della UI separata da rendering e stili del cliente.

- Valutare i quattro workspace in termini di *azioni utente*, non originari nomi dei motori.
- Percorsi prevedibili: Nuovo progetto → Import → Montaggio → Sottotitoli/Grafiche/Audio → Export.
- Inspector contestuale basato sull'oggetto selezionato; funzioni avanzate disponibili senza sovraffollare la vista principale.
- Comandi, shortcut, undo/redo e menu contestuali coerenti; scelte esplicite per AI vs manuale.
- Aiuti contestuali mirati e ricerca comandi/azioni se utile; non moltiplicare wizard obbligatori.
- Mostrare stato di generazione, provenienza e approvazione senza imporre dieci schermate.
- Verificare su almeno un progetto che tutte le quattro aree cooperino sulla **stessa** timeline.

**Gate W1:** test d'uso per le sette attività frequenti (montaggio libero, video lungo, short, foto+musica, sottotitoli, storia, export) senza caccia ai menu.

### W2 — NLE, versioni, transizioni e compositing

- **Versioni nominate** come snapshot revisionati di timeline, audio, grafica, caption, scelte export e riferimenti; zero copie massive dei media. 
- Creare, duplicare, nominare, confrontare, ripristinare e riaprire una versione; il ripristino è consapevole e non distrugge le altre. 
- Audit delle proprietà Regista originali e dei rispettivi mapping; nessuna trasformazione persa silenziosamente.
- Tracce, mix clip, trim, ripple/insert/overwrite se previsti, split, fades, speed dove supportato, effetti, keyframe e collisioni.
- Preview/export temporalmente allineati, media/source timing indipendenti dal punto sulla timeline.
- Gestione master/proxy e cache senza perdita di qualità alla consegna.

**Gate W2:** due versioni di un progetto con differenze vere sono confrontabili e ripristinabili, mantenendo asset/ID e metadati corretti.

### W3 — Audio editoriale/mixer (ZP + Regista + Suite)

- Audit ZP Stable/prova: mixer e finalizzazione realmente implementati, output e GUI; distinguere desideri legacy non realizzati.
- Livelli separati voce, musica, effetti, clip; mute/solo e fades modificabili, automazioni essenziali.
- **Ducking** facoltativo e parametrico realmente eseguito nel motore comune: attacco, rilascio, profondità, sensibilità ed envelope; confronto misurato.
- Parlato originale preservato per default; bypass e routing modificabili, nessun riconoscimento delle sorgenti presunto.
- LUFS/true peak misurati sul PCM e AAC consegnato; normalizzazione e limiter opzionali con target specifico.
- Cache/recovery/riapertura/versioni, confronto preview/export e clock verificati su clip ed estratti autentici.

**Evidenza tecnica W3:** completo000058Z e audio-master00-06-18; voce/musica autentiche8s, FX sintetico dichiarato. PCM−18LUFS/−3,3dBTP, MP4AAC−18/−3,2 (ceiling di prova−3), correlazione1, ~1,75ms clock. Non è un target universale né accettazione d'ascolto umana; quest'ultima rimane aperta. Nessuna promozione1.0.

**Gate W3:** progetto con parlato+musica+FX renderizzato con mix riconoscibile e controllabile; nessuna trasformazione audio nascosta.

### W4 — Longform/ClipsAI e Shorts/ZP

- Audit funzioni storiche, selezioni/esclusioni, short candidate, template, versioni/varianti ZP.
- ASR multi-backend da asset reali, cache/raw/corrected, playback sincronizzato e gestione parole senza timing.
- Ricerca testuale, capitoli, rough cut, false partenze/pausa; indicazioni IA revisionabili.
- Da video lungo a shorts: più candidati, motivazioni fondate, crop/reframe, CC, grafiche, audio, batch.
- Shot map e smart reframing utilizzabili; *non* continuare a rifinire i medesimi sei secondi già approvati come direzione.
- Consegne multiple con la stessa analisi, senza rifare costose inferenze.

**Gate W4:** un video autentico lungo esistente alimenta un longform revisionabile e diversi shorts, con cache intatta e senza perdita audio/testo.

### W5 — Subtitles, lingue e QA editoriale

**Nuovo requisito:** selezione e cambio dei backend di trascrizione, con varianti/modelli solo quando verificati; mantenere versioni e provenienza delle trascrizioni raw/editoriali, possibilità di confronto senza sovrascrivere correzioni approvate. Vedere la specifica 6A.2.

- Gestione separata raw ASR, testo corretto, traduzione, adattamento e cue finali.
- IT/EN misto, ripetizioni, raccordi fra chunk, token non collocati, tempo parola/cue; segnalazione senza inventare timestamp.
- Modalità di revisione umana semplice, confronto WER/CER **solo** con testo di riferimento realmente indipendente.
- Caption, ASS native, SRT/VTT, safe zones, effetti WYSIWYG e collisioni con Motion.
- Audio e sottotitoli coerenti dopo trim, spostamenti, versioni ed export multiplo.

**Gate W5:** lo stesso progetto attraversa revisione IT/EN e consegne subtitle/video senza alterazioni non approvate.

### W6 — Motion Graphics e qualità visuale

- Libreria livelli/preset/motion completa e accessibile dagli strumenti contestuali.
- Grafica sempre editabile (testo non forzato in PNG), gerarchie tipografiche ragionevoli, stili indipendenti dal brand NL del cliente.
- Gruppi/crop/mask/path, alpha, timing, keyframe, link font/linee; verificare soltanto i subset SVG che il motore promette.
- Le proposte AI non inventano numeri/citazioni o si sovrappongono in modo aggressivo a volto/sottotitoli.
- Verifica playback, non soltanto tre screenshot statici; output finali con qualità/codec configurabili.

**Gate W6:** tre composizioni originali, modificabili e realmente adatte ai media producono output coerenti e vengono approvate per i relativi casi d'uso.

### W7 — Foto, musica e AI Montage Planner

**Nuovo requisito:** progetto narrativo foto + clip + audio + reference + asset, anche con grande quantità di materiale, conservando controlli manuali e possibilità di costruire una storia prima della timeline; non ridurre a uno slideshow con transizioni casuali. Lungo/lunghissimo: chunking, cache, preview leggera, seek, salvataggio/ripresa, no limiti di durata arbitrari.

- Montaggio multi-foto/video con media originali, filtri duplicate/similarity quando necessari, ordine coerente, pan/zoom/fade/transizioni.
- Beat e struttura musicale come segnali, non griglia obbligatoria; scelta tra movimento, quiete, ritmo ed emozione.
- Planner produce decisioni strutturate fondate sui materiali, non inventa persone o eventi; testo riassuntivo derivato dal piano effettivo.
- Approvazione e rigenerazione selettive con preservazione di versioni e modifiche manuali.
- Parlato originale protetto, narrazione/musica opzionali, cue e grafiche presenti e controllabili.
- Esperienza manuale funziona anche senza LLM; proposta automatica accettabile o limiti esplicitamente indicati.

**Gate W7:** un video foto+musica con tema produce sequenza narrativa coerente, editabile, riapribile, esportata con qualità soddisfacente e validata umanamente.

### W8 — Storie, Regista AI, Calliope e ComfyUI

**Scenario obbligatorio prioritario:** trailer audio-first da quattro segmenti audio con personaggi, character sheet, ambienti, asset, sound design e prompt di produzione per singola scena/shot. Il rilevamento dei workflow deve esaminare schema/nodi reali e distinguere semplice capacità di caricare file dalla reale semantica di reference; vedere 6A.3-6A.4 e scenario A13.

- Idea/materiali/voce → soggetto → sceneggiatura → scene/shot → storyboard → produzione → timeline.
- Reference per personaggi, ambienti, abiti e asset con registro di continuità; provenienza e revisioni.
- Assistente capace di proporre completamenti/alternative localizzati, senza cancellare scelte confermate.
- Workflow ComfyUI immagine/video/audio con preflight di nodi/modelli reali, reference esplicite, prompt positivi/negativi preservati, job idempotenti.
- Produzione solo di media scelti/approvati; media in libreria con confronto e rientro nella timeline.
- Dove il modello non mantiene consistenza visiva, offrire controllo delle reference e fallback umano invece di promettere perfezione.
- Parità Regista/Calliope per capacità utili verificate, non per nomi dei vecchi pannelli.

**Gate W8:** una storia breve realmente sviluppata e montata attraversa il ciclo completo con revisioni e continuità di reference.

### W9 — IA, risorse, scheduler, cache e affidabilità

**Nuovo requisito:** Model Manager da Impostazioni, inventario byte/dipendenze/provenienza, selezione provider ASR, scansione solo lettura, pulizia mirata solo dei checkpoint gestiti esclusivamente dalla Suite con conferma; per modelli esterni/condivisi soltanto scollegamento salvo esplicita operazione avanzata fuori dal normale flusso. Vedere 6A.2.

- Niente cloud o download di modelli senza autorizzazione; provider locali esistenti e fallback non-IA.
- Job con stati e progress veritieri; crash, API 503, riavvio, RAM insufficiente, coda condivisa, recovery e non-doppio prompt Comfy.
- Non terminare/unload processi Ollama/ComfyUI appartenenti a progetti esterni. Controllare `ollama ps` se una prova vera è davvero necessaria.
- Preservare risultati costosi: Gemini2 intero, Clown Motel 60s, modelli Qwen, shorts, shot map, rendering precedenti.
- Nessuna duplicazione preventiva di runtime/ checkpoint; pulizie solo di file chiaramente temporanei del progetto e autorizzate quando hanno impatto significativo.
- Test prestazioni pertinenti (editor 1–2 h, video realmente densi, formati/fps/HEVC/4K quando disponibili), senza universalizzare benchmark sintetici.

**Gate W9:** nessuna perdita di lavoro, nessun retry duplicato e comportamento prevedibile con risorse scarse o servizi indisponibili.

### W10 — Social, export, UI e qualità di consegna

- Profili 16:9/9:16/1:1/4:5 e custom, codifiche/qualità/risoluzione/FPS/audio espliciti e conservati.
- Testo/thumbnail/chapters/titoli/descrizioni/hashtags come proposte revisionabili; niente upload automatico.
- Output finale da sorgenti corrette, non proxy a bassa risoluzione; avvertire limiti della qualità sorgente.
- Opzioni H.264/HEVC/AV1 quando effettivamente disponibili; encode hardware con verifica e nessun fallback silenzioso.
- Safe zone e QA dei titoli/CC; output pronti per destinazione scelta, audio non alterato inaspettatamente.
- Chiarezza UX: export accessibile dal Montaggio e dai flussi pertinenti.

**Gate W10:** consegna multipiattaforma riproducibile, editabile e con impostazioni comprensibili, con QA audio/video reale.

### W11 — Conformità, portable e distribuzione

- Audit dipendenze: licenze e testi di pacchetti Python, binari, modelli, workflow, asset, font e componenti optional.
- FFmpeg: attribuzioni e sorgenti corrispondenti **della build distribuita**, non soltanto link generici.
- Manifest/versionamento/checksum completo, perimetro delle risorse opzionali dichiarato, assenza dati personali nel pacchetto.
- Evitare copie integrali da ~10,57GB durante normali verifiche. Un solo pacchetto finale quando i gate sono chiusi.
- Test ricollocazione/path Unicode/spazi/PATH ridotto offline. Prova sul Windows pulito/VM usa-e-getta quando possibile, senza snapshot o backup della VM non richiesti.
- EXE build stable promossa soltanto dopo esiti completi e decisione esplicita del proprietario.

**Gate W11:** RC funzionalmente completa, legalmente documentata per la distribuzione prevista e testata su ambiente pulito nei limiti dichiarati.

---


## 6A. Specifiche integrate da logo e requisiti audio-first del 28 ottobre

**Origine:** richieste esplicite del proprietario durante il lavoro sul candidato 1.0. Non sono nuove app né un rinvio alla 1.1. I punti qui descritti integrano W1, W4, W5, W7, W8 e W9, evitando una seconda roadmap o un engine parallelo. Stato iniziale di ciascun elemento: **DA VERIFICARE NEL REPOSITORY**, non "implementato" per il solo fatto di apparire qui.

### 6A.1 — Identità visiva e tre modalità di tema

- Ispezionare l'asset `LogoMio.svg` fornito dall'utente; registrare i design tokens reali primari: viola `#7531FE`, magenta `#E4469B`, lilla `#AD85C7`; gli altri toni del logo sono gradienti/ombre, non colori da assegnare automaticamente a ogni campo.
- Preferenze `Scuro`, `Chiaro`, `Sistema`: `Sistema` segue la preferenza OS/Windows e reagisce ai cambiamenti; opzione salvata tra sessioni, senza restart o perdita di layout.
- Completa copertura di pannelli, popup, tooltips, monitor, inspector, dialoghi, timeline, editor sottotitoli, combinazioni hover/focus, warning e disabled. Fare test di contrasto e leggibilità; niente bagliore diffuso sul testo.
- Branding app indipendente dal design dei video. L'utente può applicare qualsiasi palette e font al progetto cliente; mai watermark/branding NL imposto ai render.
- Non introdurre ulteriori schermate o menu dedicati: Aspetto in Impostazioni, eventualmente selettore rapido discreto.

**Gate:** tutti e tre i temi, screenshot e navigazione reale su desktop/viewport stretta, preferenza preservata, output video identico passando da un tema all'altro.

### 6A.2 — ASR Model & Storage Manager, senza eliminazioni pericolose

**Obiettivo:** dalle Impostazioni scegliere backend e variante di trascrizione, controllare compatibilità e RAM/VRAM, vedere spazio occupato, rimuovere i modelli non utilizzati e realmente gestiti dalla suite. Non limitarsi a una lista di nomi.

- Inventario runtime/modelli **effettivamente installati**: Qwen3-ASR (varianti rilevate, runner compatibile), forced aligner (componente distinto), Whisper/faster-whisper, WhisperX, Vosk se esiste un backend effettivo; altri plugin solo dopo adapter funzionante. Esclusa l'invenzione di opzioni "small/large" senza checkpoint/runner reali.
- Registry: `model_id`, famiglia, variante, formato/quantizzazione, ruolo ASR/aligner/VAD/diarizer, backend/runtime, version/hash, percorso canonico, dimensione reale, compatibilità hardware, dati di licenza, sorgente, `ownership` (`suite-managed`/`external-linked`/`shared`/`unknown`), uso attivo e dipendenze. Non confondere peso GGUF con repository HuggingFace, Python wheels, cache o altri modelli.
- Selettore per progetto e default applicazione, con opzioni `Auto` **solo quando** la politica di scelta è verificata, manuale sempre possibile. Il cambio del modello deve preservare raw, testo corretto, trascrizioni pregresse, cue e mappe temporali; nuovi run come revisioni con provenienza, senza reinferenza implicita.
- In caso di modello mancante/offline/incompatibile: disabilitare con ragione, fallback dichiarato (non silenzioso), continuare editing manuale. Download solo dopo selezione esplicita, requisiti/licenza/spazio indicati; non scaricare nulla per i test di questa roadmap.
- **Analizza spazio** in sola lettura: calcolare bytes per checkpoint/cache/temp/runtime senza attribuire falsamente spazio risparmiabile a file hardlink, condivisi o in uso. Mostrare checkpoint inutilizzati, riferimenti da progetti/job e destinazione.
- Pulizia in due livelli: *Scollega* per esterni/condivisi; *Elimina selettivamente* per file esclusivi creati/gestiti dalla Suite, dopo preview con elenco percorsi e conferma. Bloccare operazioni su modello in RAM/VRAM, job attivo, link da altri tool, ownership ambigua o dipendenze condivise. Non eliminare checkpoint fuori dalla cartella controllata tramite percorsi arbitrari, symlink traversal o pattern generici. Nessun backup massivo inventato.
- Cache temporanee e modelli vanno gestiti separatamente; preferenza di cartella cache, percorso trasferito solo per scelta esplicita. Per un modello non utilizzato in mesi, suggerimento sì, cancellazione automatica no.

**Gate:** ASR A→B su mini campione locale (senza reimport, senza perdita revisione), riapertura, inventario bytes attendibile, simulazione di pulizia e cancellazione di un checkpoint fixture **suite-owned**, rifiuto della cancellazione di un checkpoint linked/shared, zero modifiche ai modelli reali dell'utente durante il test.

### 6A.3 — Regia audio-first: trailer segmentato con asset e character sheet

**Caso concreto proprietario:** trailer esistente da completare, diviso in **quattro parti audio** (non assumere che quattro file equivalgano automaticamente a quattro scene). L'utente associa immagini dei personaggi, character sheet, ambienti/sfondi e risorse audio, e desidera una proposta di produzione *shot-by-shot* realizzabile con i workflow effettivamente disponibili.

Flusso in un unico progetto narrativo:

1. **Materiali:** importa audio intero o quattro file/regioni, immagini personaggi, set/ambienti, concept, video, eventuali storyboard esistenti. Riferimenti con ID, provenienza e licenza; non cancellare o modificare gli originali.
2. **Timeline sonora di riferimento:** waveform e segmenti modificabili con in/out e overlap, etichette scena/shot, dialoghi/voiceover, musica, eventi sonori/FX, riverberi, crescendo e pausa. Analisi del sound design in livelli di evidenza: ciò che si sente, ciò che ASR rileva, ciò che il modello ipotizza e ciò che l'utente conferma. Non trasformare ipotesi in verità.
3. **Trascrizione/allineamento** solo quando utile e da motori esistenti, cache riutilizzabile, lingue corrette. Anche senza parole riconoscibili il progetto resta producibile da eventi sonori; conservare battute e voce originali come riferimento temporale.
4. **Scene / storyboard:** suggerisci confini di scena, shot list con durata stimata, obiettivo visivo, movimento camera, raccordo, palette/emozione, personaggi/oggetti, set, indicazioni negative, `audio_segment_id`, `source_time`, motivazione. L'utente può sostituire o bloccare ogni decisione.
5. **Asset/reference binding:** collega ciascuno shot a personaggi/character sheets, pose, abiti, props e ambienti persistenti. Audio reference può essere segmento/sottosequenza e non per forza file intero; registrare start/end e scopo (ritmo, sound design, acting reference, se il workflow lo supporta). Evitare che un character sheet sia interpretato impropriamente come un fotogramma iniziale.
6. **Capability matching:** per ogni shot mostra workflow compatibili con quelle reference, differenza tra image-to-video, start/end frame, multi-image, audio-conditioned, video-to-video e text-only; le associazioni non dimostrate sono esplicitamente *richiedono mappatura*.
7. **Pre-produzione:** selezionare settings consigliati validi per il workflow/modello e la GPU presenti, piano di suddivisione in generazioni brevi/estensione/continuity, costo stimato (se misurato), rischio OOM, seed, durata, FPS, aspect, risoluzione e dipendenze. Preview/placeholder o storyboard statico se generazione non disponibile.
8. **Approvazione e produzione:** mostra piano e risorse per singolo shot prima della coda; invia esclusivamente gli shot approvati; progress, receipt, `/history` e ripresa dopo riavvio senza reinvio duplicato. Revisione output, sostituzione varianti e ritorno alla timeline condivisa.
9. **Montaggio finale:** ricompone sul trailer audio originale, raccordi, sync, motion/title/CC quando richiesti, colori, mixer, output; tutti gli shot sono editabili e collegati alle versioni del montaggio.

**Importante:** ascolto/analisi dell'audio e rilevamento dei livelli non dimostrano automaticamente l'identità dei suoni o la divisione semantica della scena. Quando servono sorgenti separate, proporre separazione, ma non dichiararla infallibile. La generazione o imitazione di voci reali è un workflow distinto con diritti e consenso; non attivarlo implicitamente da una reference vocale.

**Gate:** quattro segmenti audio autentici (o fixture rappresentativa autorizzata), asset + sheet + fondali → timeline audio → proposta scene/shot → correzione/lock → workflow matching e configurazione → preflight → produzione di almeno uno shot locale *solo se provider disponibile e risorse adatte*, oppure stato bloccato corretto → montaggio/reopen/MP4; non serve generare l'intero trailer per verificare l'architettura.

### 6A.4 — ComfyUI Workflow Capability Inspector e limiti di generazione verificati

- Scansione file workflow JSON presenti e nodo/edge graph (non affidarsi ai nomi nel filename). Interrogare il server ComfyUI attivo con `GET /object_info` e `/object_info/{node_class}` per input/output tipizzati realmente disponibili; controllare `GET /system_stats`, modelli/nodi effettivi, e preservare esattamente le scelte del workflow originale. Non alterare server o installazioni di altri tool.
- Classificazione delle capacità reali con prove: prompt testuali; `IMAGE` singola/multipla, character sheet, stile/reference, start/end frame, `VIDEO`, `AUDIO`/waveform e segmenti temporali. Un ingresso `IMAGE` generico **non prova** che il workflow preservi identità del personaggio. Definire capacità semantiche con adapter testato, mappatura manuale degli slot oppure manifest locale di capability basato su evidenze. Se la mappa non esiste, mostra `non verificato` anziché dichiarare supporto.
- Audio reference parziale: quando il workflow accetta audio o feature compatibili, produrre segmento **temporaneo identificabile** da intervalli di clip, mantenendo originale/offset e senza cancellare o copiare necessariamente tutto il file. Se non supportato, non usare input audio in slot immagine o testo per aggirare il problema.
- Proposte settings **dinamiche**: range tipizzati del nodo, modello caricato, dimensioni ammesse, durata/fps/frame_count, step, CFG, seed, checkpoint, formati e disponibilità reale. Profilo hardware locale RTX 4060 Ti 8GB come riferimento **misurato**, non come garanzia universale o hard cap fisso.
- Profili `Bozza`, `Bilanciato`, `Qualità` e `Personalizzato` come punti di partenza, con suggerimento di segmentare in shot brevi, upscaling/correzione in fasi distinte solo se conveniente. Stime di memoria/tempo e warning con provenienza (documentazione del modello/benchmark/euristica). Le soglie di rischio non devono inventare limiti del modello o impedire per principio override manuali realmente supportati.
- Preflight: convalida il grafo **prima** di inviare; riferimenti risolti, input tipizzati corretti, assenza modelli/nodi, VRAM/RAM prevista dove stimabile, permessi e code. Non correggere automaticamente reference mancanti sostituendole con la prima immagine, né scegliere il primo checkpoint a caso.
- Per lunghe scene: genera solo finestre realmente supportate dal modello, mantieni audio master e timestamps, estendi/stitch o crea stacchi secondo piano revisionabile; non promettere video AI lunghissimi in una singola inferenza.

**Gate:** catalogo di workflow reali etichettati per capacità confermate/incerte; user binding di sheet+ambiente+audio slot; riconoscimento di workflow incapace di accettare l'asset e messaggio chiaro; proposte settings tracciabili; preflight con grafi fittizi e reali senza job costosi. Un singolo job reale non costoso solo se disponibile e autorizzato.

### 6A.5 — Video molto lunghi e progetto fotografico narrativo

- Eliminare limiti UI arbitrari di durata o conteggio foto; i vincoli tecnici del filesystem, del container, di FFmpeg e dei numeri temporali restano reali e vanno diagnosticati.
- Media index scalabile: paginazione/virtualizzazione, proxy, thumbnail/on-demand, waveform in chunk, seek con timebase razionali, timeline con zoom e gestione in/out numericamente stabile.
- Analisi in segmenti riavviabili (ASR, VAD, shot map, scene, beat, embeddings opzionali) con cache dipendente da hash asset+modello+parametri, revisioni e confini di chunk; nessuna reinferenza totale per cambiare un titolo o una transizione.
- Foto+asset+storia: ordine suggerito per importanza, protagonisti, contesto, luogo, tempo quando supportato da metadata/utente, emozione, ritmo, scene di raccordo, musica e B-roll; proposta manuale e AI concorrenti sullo stesso progetto, con preservazione delle scelte approvate.
- Rendering streaming/chunk o per segmenti quando appropriato, progress effettivo, recovery su job, spazio temporaneo calcolato e chiara gestione dei fallimenti. Non promettere prestazioni a priori.

**Gate:** riuso delle prove autentiche già svolte sul video ~58m, più stress con media/metadata sintetici se necessario; progetto fotografico e storia completamente riapribili; prove senza render ripetuto dell'intero master.

**Sequenza suggerita di intervento:** completare W3 (mixer) e UX verticale, in parallelo integrare 6A.1/6A.2 come impostazioni. Poi avviare 6A.3+6A.4 come un unico flusso audio-first, consolidare W7 e 6A.5. L'ordine può cambiare per dipendenze reali; non attendere richieste dell'utente fra attività autonome.

---



## 7. Scenari di accettazione end-to-end

Ogni scenario ha un rapporto `scenario`, `input/asset origin`, `azioni`, `output`, `limitazioni`, `PASS tecnico`, `PASS UX`, `ACCETTAZIONE umana` separati. Non attribuire un giudizio creativo all'agente.

| ID | Scenario utente | Prova minima obbligatoria |
|---|---|---|
| A01 | Video lungo YouTube con parlato reale | Import → ASR/cache → capitoli/rough cut → correzione testo → MP4 + capitoli → riapertura |
| A02 | Da lungo a shorts | ≥3 proposte → review → reframe → caption/motion → output verticali, audio e safe zone |
| A03 | Montaggio foto+musica | Fotografie/video multipli → IA facoltativa o manuale → beat/transizioni/titoli → variazione manuale → MP4/reopen |
| A04 | Idea → storia → scene | Prompt/brief → sceneggiatura → reference/storyboard → media scelti → continuità → timeline/export |
| A05 | Subtitles IT/EN | Campione umano indipendente → correzioni/cue → ASS/SRT/VTT/burn-in e sincronismo |
| A06 | Audio professionale | Voce+musica+FX → livelli, fades, ducking optional → confronto ascolto/PCM → export |
| A07 | Versioni alternative | Versione A/B/C con titoli, mix e crop diversi → confronta → ripristina → riapri, senza copie master |
| A08 | Modalità offline senza IA | Import, editing, manual captions, native motion, export, reload senza provider disponibili |
| A09 | Recupero e risorse | Job interrotto, RAM minima, 503/restart e coda → ripresa senza duplicati o alterazioni |
| A10 | Portable distributivo | Pacchetto finale su Windows pulito/path diverso, senza Python/Cargo del developer, media/export, report licenze |
| A11 | UX professionale | Persona trova e svolge sette compiti comuni senza indicazioni dal codice o menu legacy |
| A12 | Qualità grafica e audio | Preview/export e playback reali, testo leggibile, crop accettato, voce/music bilanciate secondo brief |
| A13 | Trailer audio-first con 4 segmenti | Reference personaggi/ambienti/FX + audio diviso → shot plan → workflow capability → configurazione/preflight → almeno uno shot validato o blocker chiaro → timeline/export |
| A14 | Gestore modelli ASR e storage | Switch backend senza perdere edizioni → scansione bytes → rimozione fixture owned → tutela modelli linked/shared → riapertura |
| A15 | Light/Dark/System da logo | Cambio tema e preferenze persistenti; editor/timeline leggibili; output cliente non toccato |
| A16 | Storia fotografica e video molto lungo | Multi-media con storia+audio; cache riusata, seek e modifica, export/reopen senza reprocess di master |

I casi reali già svolti si **riusano**. Si ripetono solo segmenti modificati o gate ancora aperti, con motivazione.

---

## 8. Qualità, test, evidenze e criteri PASS

### Per ogni capacità

- **Test statico e unit**: schema, time-map, convalida input, formattazione e Clippy/Rust dove coinvolti.
- **Test integrato UI**: selezione, modifica, error state, salvataggio, riapertura e passaggio workspace/progetto.
- **Media reale**: FFmpeg, frame/audio sonde, codec/timebase, font/alpha quando necessari.
- **Parità monitor/export**: misurata quando ha senso; differenze da compressione dichiarate. Per tracking, anche playback a 1×.
- **Qualità editoriale**: verifica esplicita da proprietario o revisore, soprattutto testo IT/EN, narrativa, montaggi musicali, grafiche.
- **Regressione generale**: dopo tranche coerente e prima di dichiarare nuova baseline; identificatore report e hash dev EXE.

Se un test fallisce, conservarne la diagnosi; riparare e ripetere il giro interessato, senza sommare mirati e vecchie baseline per produrre un finto PASS.

### Politica di verifica a costo ridotto

1. Leggere report/artefatti prima di avviare un job.
2. Per un nuovo effetto usare clip brevi rappresentativi; non ricalcolare Gemini2 58 minuti o il campione Qwen 60 s.
3. Sviluppare con media sintetici/mock ove sufficiente, ma segnare chiaramente che non provano qualità reale.
4. Test reale mirato una volta, riusare l'output per molte verifiche.
5. Fare full regression prima della nuova dichiarazione di stabilità, evitando full export ripetuti.
6. Conservare la stabilità dell'eseguibile `app/` finché non è autorizzata la promozione.

### Evidenze minime da registrare

`goal_id`, `status`, `input`, `modifiche`, `tests`, `report_path`, `dev_build_sha`, `runtime/provider`, `media_originali_intatti`, `osservazione_umana`, `limiti`, `next_action`.

---

## 9. Distribuzione, licenze e release

La release finale non si riduce a un comando ZIP. Prima della promozione:

- La matrice legacy è stata riconciliata e le funzioni `CORE_1.0` hanno un esito verificato, non il semplice nome di un pannello.
- I workflow reali principali sono utilizzabili e la UX è leggibile.
- Le limitazioni dell'IA sono oneste e l'uso senza IA è funzionale.
- Il portable è completo, testato su ambiente pulito, con manifest e dipendenze giuste, assenza di media/progetti privati.
- Il materiale di terzi incluso è redistribuibile secondo gli obblighi effettivi: testi delle licenze e sorgenti corrispondenti dove richiesti, attribuzioni, uso di font e modelli.
- La versione 1.0 è allineata tra EXE, manifest, UI, docs, launcher, ZIP e note.
- Il proprietario autorizza la promozione della stable; un test PASS non equivale a un'autorizzazione.

---

## 10. Gestione dei cambiamenti e definizione di completamento

### Decisione sul perimetro

Se l'agente trova una funzione legacy mai citata, deve investigare utilità e compatibilità. Se è fondamentale alla visione originale, integrarla o presentare una decisione motivata. Se è un esperimento incompleto, documentarla come tale e non inseguirla soltanto per “parità totale”. **Non rinviare una capacità centrale al dopo-1.0 senza confronto con il proprietario.**

### Cosa significa “chiudere Wn”

- Tutte le lacune scoperte per quella tranche sono classificate.
- Le capacità previste sono integrate e collaudate ai livelli appropriati.
- L'interfaccia è ragionevole e lo stato si conserva.
- Il registro e il checkpoint permettono ripresa esatta.
- Le limitazioni rimaste sono visibili e non bloccano un risultato professionale, oppure sono riportate per decisione.

### No rincorsa infinita

Non investire altre sessioni nello stesso difetto locale quando il proprietario ne ha accettato una soluzione/fallback. Le revisioni visive devono servire un risultato professionale concreto, non produrre decine di varianti senza criterio.

---



## Nuovo requisito CORE1.0 —5 novembre: racconto audiovisivo da album (6A.6 / A17)

Estendere AI Montage Planner e libreria comune, NON un'altra applicazione o renderer. Percorso nei workspace Montaggio e Storie/Regia: importa album misto → analisi individuale recuperabile → scheda protagonista → proposte narrative revisionabili → premontaggio nativo → modifica/export/riapertura/versioni.

- Album grandi senza tetti arbitrari di fotografie: coda paginata, analisi singola perasset/modello/versione del prompt, checkpoint atomici, pausa/ripresa/errori perfile, cache riutilizzabile e nessuna reinferenza per cambiare titolo o ordine. Video/musica/reference opzionali.
- Modelli locali installati: inventario Ollama `/api/tags` + `/api/show`/capabilities e prova immagine reale soltanto con risorse disponibili. Qwen3.5 e Gemma4 vanno verificati per la variante presente, non dalnome. Non caricare/unload modelli mentre altri processi usano il servizio; nessun cloud/download.
- Evidenza per foto strutturata: descrizione, soggetti/oggetti visibili, ambiente/atmosfera, qualità/orientamento/risoluzione, tempi EXIF quando presenti con provenienza/incertezza. Date mancanti restano sconosciute; EXIF non prova eventi biografici. Duplicati esatti e somiglianze proposte con motivi, esclusioni non distruttive; nessuna rimozione automatica degli originali.
- Profilo protagonista da utente: nome/età/carattere/passioni/soprannomi/persone/aneddoti/ricordi/intenzione. Associazioni manuali foto/soggetti e conferma dei raggruppamenti. Vietato inventare identità, parentele, date o episodi; distinguere osservazioni, EXIF, dichiarazioni e interpretazioni creative.
- Sintesi gerarchica dell'intera raccolta nel planner esistente: temi/fasi ipotizzate/atmosfere e immagini forti; commedia/emozionale/epica/documentario ironico/custom. Apertura/sviluppo/momenti forti/pause/conclusione, non cronologia o griglia beat uniforme. Materiali obbligatori/lock/esclusioni/capitoli e rigenerazione parziale conservano scelte approvate.
- Piano approvato → stessa timeline/compositor: durate/ordine, panzoom, composizione sicura per soggetti/volti, raccordi, musica, clip, titoli Motion editabili e copione narrativo collegato alle scene per registrazione/sincronizzazione successiva. Warning di risoluzione/crop, niente immagini originali alterate. Manuale completo senza LLM.

**Gate A17:** raccolta fotografica autentica varia (percorso richiesto al proprietario), evidenze individuali/cache/stop-ripresa/dedup, proposta e copione persistenti, lock/revisione/rigenerazione selettiva, timeline → breve video dimostrativo → versioni/riapertura/modifiche. Conservare output e storia per accettazione umana: test automatici o provider simulati non certificano qualità artistica. Dipendenze: consolidare6A.2 e il codice audio-first già in corso; non annullare quelle modifiche.



## Integrazione prodotto — 6 novembre 2026: compleanno e dossier aperto

Il percorso Album/Montaggio già operativo deve rendere utilizzabile subito il lavoro reale con molte fotografie, musica, narrazione e tre capitoli stilisticamente distinti, senza attendere la promozione1.0. Usare profilo confermato, cue per foto, capitoli, timeline/Motion/mixer/versioni comuni. Nessuna identificazione automatica della protagonista; Jennifer è committente. I tre trattamenti automatici completi non sono ancora certificati: offrire la rifinitura manuale effettiva.

Dossier di produzione integrato nello stesso progetto: copione/timecode, titoli, storyboard/shotlist, configurazioni native e provenienza, media opzionali con inventario/hash, prompt/binding/grafi workflow associati e indicazioni After Effects. Formati aperti HTML/TXT/CSV/JSON/ZIP, nessun AEP dichiarato, nessun secondo motore. Distinguere ispezione statica del grafo da preflight live e supporto semantico delle reference. Riferimenti senza copie per default; inclusione esplicita dei soli media del progetto, mai checkpoint/runtime. Nuovo job nella coda esistente; nessuna generazione aggiuntiva per esportare il dossier. Stato tecnico ed evidenze aggiornati nell’handoff, accettazione artistica separata.

Esito della tranche6novembre: generale010748Z CONCLUSO60/60PASS158Rust657,84s sul candidato1B4DA452…578627A. W3storico preservato/escluso; nessuna produzione successiva e stable non promossa. Percorso pratico compleanno e dossier collaudati; accettazione artistica/ascolto ancora aperta. Prima delle reference temporali/trailer completo, estendere le versioni nominate esistenti al sidecar narrative.json: il collaudo attuale verifica creazione/presenza versione e riapertura, non il ripristino di quel piano. Handoff aggiornato con evidenze e dipendenze concrete.

## Precisazione vincolante — sistema comune di preproduzione e regia (6 novembre 2026)

Il compleanno è un progetto reale di collaudo, NON il perimetro della capacità. Album AI, regia audio-first, assistente/continuità e dossier sono ingressi e viste dello stesso sistema audiovisivo: compleanni, matrimoni, documentari, trailer, cortometraggi, spot, videoclip, aziendale, social e progetti liberi. Nessuna pipeline/workspace separata per categoria. Tipologie come preset facoltativi e liberamente adattabili, senza imporre struttura o stile. La stessa produzione combina foto, filmati, audio, musica, narrazione, generazioni e lavoro manuale, con fonti e decisioni tracciabili.

Dossier generico, revisionabile e riutilizzabile: esportare elementi pertinenti, copioni/titoli/capitoli, storyboard/shot list, reference, prompt/configurazioni e asset disponibili; evidenziare esplicitamente elementi mancanti, non generati o non approvati. Istruzioni After Effects e titoli non devono imporre il compleanno né tre capitoli. Nessun AEP o compatibilità ComfyUI non dimostrata. Tutto sui motori/timeline/versioni comuni, retrocompatibilità preservata.

Ordine operativo: (1) versionamento del piano narrativo e delle decisioni di regia con backup/ripristino coerente; (2) reference audio temporali e binding dei workflow reali; (3) dossier end-to-end su più tipologie. Poi gate generali1.0. Riutilizzare test/media/inferenze; non riaprire capacità concluse.

Checkpoint tecnico6novembre: priorità1–3 avanzate nello stesso motore: versioni narrative schema2 con backup/recovery e compatibilitàv1; binding referenceaudio a intervalli con cache/validazione; dossier generico per album, audio-first e timeline manuali miste, media pertinenti/mancanti e cueaudio, istruzioniAE senza vincolo compleanno/3capitoli. Verifica finale mirata6/6(023137Z),160unitRustPASS nel precedente giro della tranche; non una nuova regressione generale. Nessuna nuova inferenza o renderlungo. Il compleanno rimane collaudo, non prodotto separato. Preset facoltativi comuni, regia/audiosemantica reale e QAartistico restano da completare prima dei gatefinali. Dettagli effettivi e conservazione nell’handoff.
