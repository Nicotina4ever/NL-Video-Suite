# Session Handoff — 18 novembre 2026

## Stato da cui riprendere

Leggi prima questo file e [Stato e roadmap operativa](../../documentazione/STATO_E_ROADMAP_1_0.md). La specifica completa unica è [V1_MASTER_ROADMAP.md](V1_MASTER_ROADMAP.md). Non ricominciare audit o pipeline già realizzati.

Candidato `sviluppo/target/release/nl-video-suite-ideas.exe`, SHA256 `c0c5a609464712c4032fb175509d9ad9f695167ec843420ab556e332b14cbf5c`; 6/6mirati +1GUI aggiuntiva sullo stesso hash e4unit MotionIdea PASS,169filtrati. Versione0.7.1, NON1.0. Non esiste una nuova regressione generale. Prova nuovo LLM simulata, exportMP4 reale. Progetti personali/stabile/originali intatti.

## Lavoro corrente

Pulizia completata delle11cartelle temporanee workspace-flow: tests46→circa4,00GiB logici, spostamento in da_eliminare. Altre47cartelle obsolete(384MiB) spostate; deduplicata copia Gemini2identica con timestamp preservato(1,368GiB liberabili dopo svuotamento quarantena). Corretto cleanup standalone del test, anche link master su errore; niente nuova regressione, inferenza o renderlungo per la pulizia. Cache e inferenze autentiche indispensabili conservate. Non ricreare Debug o copie massicce.

Documentazione consolidata in README + tre guide (uso, stato, architettura) + specifica completa + questo handoff. I vecchi rapporti non certificano nuovo codice; archivio locale privato in `sviluppo/archivio/documentazione_20261118`, non richiesto per compilare. Report privati in tests/artifacts non inclusi nel clone; non rigenerarli per ricostruire lo stato.

**Pubblicazione sperimentale completata:** sorgenti su https://github.com/Nicotina4ever/NL-Video-Suite, storia remota preservata (primo push55af9bb→ad8fb0c). Prerelease `experimental-2026.11.18` con CoreWindowsx64, checksum e verificaJSON. Ricevuta completa `V1_PUBLICATION.json`: ZIP8.313.084byte, SHA256 `a1936ad4d08c73949e16ad88d705f95e5ddf74073e6c9806ce85019d3943eb04`, download PUBBLICO riletto e verificato. Nessun dato personale/Python/checkpoint caricato.

CoreQA PASS `core-portable/2026-11-18T20-33-15-380Z`: manifest, nientePython/modelli, servizi offline, FFmpeg esterno scelto, GUI→MP4reale1,2s→stop proprietario→spostamento con spazi→riapertura. Packager2unit/sintassiPS PASS; profili browser esclusi dagli archivi personali. Ultimo aggiornamento ZIP aggiunge guide/avvisi Rust(151pacchetti+standard library), i19file EXE/PS/Python sono byte-identici al pacchetto GUI collaudato: niente rerender/inferenza per i testi. Prime prove fallite per attesa pipe del launcher sincrono e namespace Windows nel test, corrette; non chiamarle difetti dell'app. La GUI export ricostruisce i link da basename, non dichiarare persi i file dopo ricollocazione dal solo output_paths assoluto.

Avvio Core in finestra app Edge già installato, nessun Electron o runtime nuovo. Finestra desktop interattiva/Windows pulito ancora da accettare, contenutoGUI testato Edge headless. Core senza FFmpeg/Python/pesi: Media richiede FFmpeg; per tutte le risorse sul proprio altroPC affiancare runtime/models/modelliAST gia disponibili e conservare l'EXE avanzato del Core. Non confondere packager completo(default app/stabile) con Core sperimentale. Redistribuzione full e promozione1.0 restano bloccate dai gate. Stabile localeSHA61442816 invariata.

Pulizia finale: tests circa4.001GiB logici rispetto ai46iniziali;11cartelle workspace-flow +47cartelle obsolete +2CoreQA falliti spostati in da_eliminare. DedupGemini1,368GiB con SHA/timestamp conservati; inferenze/raw/proxy/exportlungo e fixture necessarie intatti. In quarantena circa1.568GiB effettivamente liberabili dopo svuotamento MANUALE; non dire gia liberati. Debug assente. Prossima sessione NON rifare pulizia, pubblicazione, CoreQA o inferenze esistenti per ricostruire lo stato.

## Dopo la pubblicazione

1. Accettazione UX e prova semantica reale del nuovo assistente grafico con modello già installato quando risorse libere. Non scaricare o fermare altri processi.
2. Audio-first: reference/master e28shot già pianificati; preflight→un solo shot reale compatibile→revisione→timeline/export. Non generare tutti gli shot.
3. QA audiovisiva/narrativa/ITEN indipendente, parità residuale e recovery/performance, licenze e portable completo; poi congelamento e regressione finale prima di promozione autorizzata.

Gemini2~58min già trascritto6143parole135blocchi, exportlungo+3short e shotmap63candidati/14finestre disponibili localmente. Album8foto reali/demo33,7s e121asset sintetici già collaudati. W3 tecnico chiuso, ascolto umano aperto. Non ripetere tali elaborazioni o le inferenze precedenti.
