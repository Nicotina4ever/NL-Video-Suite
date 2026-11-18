# Session Handoff — 18 novembre 2026

## Stato da cui riprendere

Leggi prima questo file e [Stato e roadmap operativa](../../documentazione/STATO_E_ROADMAP_1_0.md). La specifica completa unica è [V1_MASTER_ROADMAP.md](V1_MASTER_ROADMAP.md). Non ricominciare audit o pipeline già realizzati.

Candidato `sviluppo/target/release/nl-video-suite-ideas.exe`, SHA256 `c0c5a609464712c4032fb175509d9ad9f695167ec843420ab556e332b14cbf5c`; 6/6mirati +1GUI aggiuntiva sullo stesso hash e4unit MotionIdea PASS,169filtrati. Versione0.7.1, NON1.0. Non esiste una nuova regressione generale. Prova nuovo LLM simulata, exportMP4 reale. Progetti personali/stabile/originali intatti.

## Lavoro corrente

Pulizia completata delle11cartelle temporanee workspace-flow: tests46→circa4,00GiB logici, spostamento in da_eliminare. Altre47cartelle obsolete(384MiB) spostate; deduplicata copia Gemini2identica con timestamp preservato(1,368GiB liberabili dopo svuotamento quarantena). Corretto cleanup standalone del test, anche link master su errore; niente nuova regressione, inferenza o renderlungo per la pulizia. Cache e inferenze autentiche indispensabili conservate. Non ricreare Debug o copie massicce.

Documentazione consolidata in README + tre guide (uso, stato, architettura) + specifica completa + questo handoff. I vecchi rapporti non certificano nuovo codice; archivio locale privato in `sviluppo/archivio/documentazione_20261118`, non richiesto per compilare. Report privati in tests/artifacts non inclusi nel clone; non rigenerarli per ricostruire lo stato.

**In corso:** prima pubblicazione GitHub della sperimentale con storia remota preservata, sorgenti privi di dati personali, pacchetto Core avviabile e dipendenze esplicite. CoreQA PASS `core-portable/2026-11-18T20-33-15-380Z`: manifest, noPython/modelli/servizi, FFmpeg esterno, GUI→MP4reale1,2s→stop→spostamento con spazi→riapertura. Packager2unit/sintassiPS PASS. Prime prove fallite solo per attesa pipe del launcher sincrono e namespace Windows nel test, corretti; nessuna nuova inferenza. Restano commit/tag/upload e verifica asset realmente scaricabile. Nessuna sostituzione dell'app stabile locale. Il Core non equivale al portable completo1.0; licenze/redistribuzione full e Windows pulito restano gate.

## Dopo la pubblicazione

1. Accettazione UX e prova semantica reale del nuovo assistente grafico con modello già installato quando risorse libere. Non scaricare o fermare altri processi.
2. Audio-first: reference/master e28shot già pianificati; preflight→un solo shot reale compatibile→revisione→timeline/export. Non generare tutti gli shot.
3. QA audiovisiva/narrativa/ITEN indipendente, parità residuale e recovery/performance, licenze e portable completo; poi congelamento e regressione finale prima di promozione autorizzata.

Gemini2~58min già trascritto6143parole135blocchi, exportlungo+3short e shotmap63candidati/14finestre disponibili localmente. Album8foto reali/demo33,7s e121asset sintetici già collaudati. W3 tecnico chiuso, ascolto umano aperto. Non ripetere tali elaborazioni o le inferenze precedenti.
