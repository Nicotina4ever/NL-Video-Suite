# Stato e percorso verso NL Video Suite 1.0

> **21 novembre 2026 — restauro in corso.** Fasi chiuse: consolidamento del lavoro 19/11, rinomina progetto con cartella leggibile che segue il titolo, trasparenza durate/trascrizione (verificata sul reale 28 min), indice SQLite FTS derivato per progetto, shell «Premiere-style» (Vite + Svelte) in `sviluppo/web-app`. Punto di ripresa: [Session Handoff](../sviluppo/roadmap/V1_SESSION_HANDOFF.md). Le righe sotto restano la base finché il handoff non le sostituisce esplicitamente.

Aggiornato: **18 novembre 2026**. Canale di pubblicazione: **sperimentale avanzata**, non 1.0 stabile. La versione interna del candidato è ancora 0.7.1. L'eseguibile stabile locale non è stato sostituito.

## Documentazione da usare

- [Uso e installazione](USO_E_INSTALLAZIONE.md): avvio, quattro workspace, dipendenze e lavoro su un altro PC.
- [Architettura e sviluppo](ARCHITETTURA_E_SVILUPPO.md): contratti, motori, tempo, cache, workflow e parità legacy.
- [Specifiche complete 1.0](../sviluppo/roadmap/V1_MASTER_ROADMAP.md): perimetro e scenari obbligatori.
- [Session Handoff](../sviluppo/roadmap/V1_SESSION_HANDOFF.md): prossimo lavoro verificabile. Le copie cronologiche non prevalgono su questi documenti.

## Candidato realmente verificato

`nl-video-suite-ideas.exe`, SHA256 `c0c5a609464712c4032fb175509d9ad9f695167ec843420ab556e332b14cbf5c`.

**6/6 verifiche mirate PASS**: fmt, Clippy su tutti i target, build release, Motion Idea GUI, eliminazione progetto GUI, prerequisiti del workflow. Una seconda prova GUI sullo stesso hash verifica il passaggio del prompt a Generazione, il percorso manuale senza IA e la conservazione delle composizioni approvate. **4 unit Rust mirate PASS, 169 filtrate**: non sono 173 test eseguiti. Nessuna regressione generale del candidato attuale è dichiarata.

Inferenza del nuovo assistente grafico **simulata** nel test; preview libass e MP4 di 2,4 secondi 1920×1080 H.264/AAC 30fps **reali**, progetto riaperto e originale preservato. La qualità artistica del modello non è approvata da questi risultati. Le prove complete storiche appartengono ai rispettivi vecchi hash; non vengono sommate a queste verifiche.

## Capacità presenti e lacune da chiudere

| Area | Attualmente operativo | Ancora necessario per la 1.0 |
|---|---|---|
| W0–W1 · UX | Quattro workspace, libreria comune, strumenti contestuali, avvio Stable/Anteprima, temi Scuro/Chiaro/Sistema; Montaggio semplificato, monitor e timeline ridimensionabili, cursore seguito durante play | Accettazione d'uso sui sette compiti frequenti, schermi piccoli/DPI, meno passaggi e assenza di pannelli ridondanti in tutte le aree, non solo Montaggio |
| W2 · Montaggio | Multitraccia, trim/split, trasformazioni, fade regolabili, pan/zoom, contain/cover/sfondo sfocato, versioni nominate senza duplicare master, originale distinto da proxy | QA percettiva del tracking e delle composizioni verticali; conferma parità residuale delle trasformazioni legacy, inclusi casi di velocità/time mapping non rappresentati |
| W3 · Audio | Mixer/bus/routing, automazioni, ducking Voce→Musica, loudness LUFS/true-peak, finalizzazione e limiter opzionali; cache/versioni e PCM/AAC tecnicamente collaudati | Accettazione d'ascolto umano su parlato/musica/FX, senza cambiare di nascosto i trattamenti. Non riaprire l'implementazione né rifare i render già disponibili senza un difetto concreto |
| W4 · Longform/shorts | ASR, allineamento, capitoli, rough cut, ricerca/testo→clip, candidati, copertine, batch, shot map, cache; prova autentica ~58min: 6.143 parole/135blocchi, export lungo e3short già prodotti; shot map63candidati/14finestre ispezionate | Accettazione narrativa dei tagli, qualità effettiva degli shorts, confronto dei casi KEEP/esclusioni/compilazioni legacy ancora non certificati, scala e reattività su altri materiali |
| W5 · Sottotitoli/ASR | Traccia CC, editor/cue, raw e correzioni separati, provenienza motore/lingua, revisione raccordi senza timestamp inventati, ASS/SRT/VTT e ordine CC/Motion | QA IT/EN con riferimento umano indipendente; niente WER/CER contro il raw del medesimo motore. Verificare raccordi incerti, sincronismo e leggibilità artistica |
| W6 · Grafica | Livelli testo/forme/path/SVG/immagini, gruppi/crop/alpha/keyframe, stili personalizzabili e JSON; idea libera→scene revisionabili→livelli nativi→preview/export/riapertura; preset manuali facoltativi | Prova semantica reale del nuovo assistente con modello locale e risorse libere, tre composizioni artisticamente approvate; nessuna promessa di effetti/3D o reference semantiche non supportati |
| W7/6A.6 · Album e montaggio creativo | Foto/video/musica, ritmo CPU, planner unico, EXIF/dedup, analisi incrementale/cache, protagonista con dati confermati, sintesi gerarchica, copione per foto/capitolo, revisioni e blocchi; 8foto autentiche/demo33,7s e121asset sintetici con versioni/riapertura | Qualità narrativa reale su album esteso, confronto degli stili e copione registrabile; non dedurre identità, parentele o fatti da ipotesi del modello. Non confondere scala sintetica con qualità reale |
| W8/6A.3–4 · Regia e ComfyUI | Storia/materiali/reference→scene/shot, continuità e versioni del piano, audio-first, binding temporali senza doppio trim, import API GUI, Doctor su copie, preflight e dossier aperto | Trailer autentico: quattro master/146,408s/28shot pianificati, ma non28generazioni validate. Revisionare ascolto, identità e reference; risolvere encoder/workflow realmente compatibile e collaudare un solo shot quando la coda è libera. Un loader IMAGE/AUDIO non prova il supporto semantico |
| 6A.2/W9 · Modelli e affidabilità | Provider/modelli per ruolo, backend ASR/profili/raw, inventario e rimozione confermata dei soli modelli privati; gestione RAM/VRAM, job, recovery, ComfyUI offline riconosciuto | Prova finale degli stati inconsistente/crash/503/memoria/coda sul candidato congelato; calibrare stime e scelta dispositivo su altro hardware, senza CPU forzata o unload di servizi altrui |
| W10 · Social/export | Shorts/thumbnail/Advisor, qualità e codec supportati, preset e timebase razionali, originali in export, salvataggio/riapertura | Validazione editoriale/visiva delle consegne effettive, continuità audio/video e contrasto; nessun voto artistico derivato dai soli test pixel |
| W11 · Distribuzione | Sorgenti pubblicati, candidato avviabile, launcher e packager esistenti; pacchetto sperimentale Core separato dai dati locali | Portable completo su Windows pulito, dipendenze e licenze/font/modelli verificate, corrispondenti sorgenti dei binari dove richiesti, allineamento versione e regressione finale. Il Core sperimentale non certifica la distribuzione completa |

## Verifica del Core sperimentale

ZIP Core circa8MiB: manifest/hash verificati, nessun interprete Python o modello incluso, servizi IA offline, FFmpeg esterno scelto esplicitamente. GUI→titolo editabile→approvazione→MP4 reale1,2s1920×1080H264/AAC; stop proprietario, nuova cartella con spazi, avvio e riapertura del progetto/output PASS sulla stessa macchina Windows. Non è prova su Windows pulito né del runtime completo. I metadati job possono conservare percorsi assoluti, ma la GUI degli export ricostruisce i collegamenti dal nome file nel progetto attuale. La prova di ricollocazione non dimostra tutti gli altri casi di recovery/reference: completarli prima della1.0, senza dedurre un difetto dal solo prefisso Windows nel test.

## Dossier e sistema di produzione comune

Album AI, audio-first e Dossier condividono libreria, regia, timeline e compositor. Compleanno, matrimonio, documentario, trailer, corto, spot, videoclip, aziendale, social o progetto libero sono preset facoltativi; non pipeline o menu separati. Dossier esporta copione/titoli/timecode, storyboard/shot, riferimenti asset, prompt e indicazioni di composizione in formati aperti. Le istruzioni per After Effects non sono un file AEP; un asset mancante o una reference non verificata resta esplicito. Nessun media viene dichiarato generato se non esiste un output reale.

## Ordine pratico per chiudere i gate

1. Pubblicazione sperimentale completata: [sorgenti](https://github.com/Nicotina4ever/NL-Video-Suite) e [prerelease Windows x64](https://github.com/Nicotina4ever/NL-Video-Suite/releases/tag/experimental-2026.11.18), download/checksum verificati, binario in ZIPcirca7,93MiB. Stabile e dati personali preservati. La distribuzione completa resta un gate distinto.
2. Provare davvero l'assistente grafico con un modello locale disponibile; continuare la semplificazione sulla base dei compiti dell'utente, non aggiungere menu per ogni esempio.
3. Chiudere il singolo shot audio-first reale attraverso preflight compatibile, reference approvate, generazione, revisione, timeline ed export. Non generare tutti gli shot.
4. Accettazione audiovisiva concatenata, album/copione reale e QA IT/EN indipendente; riusare inferenze, proxy ed export costosi.
5. Riconciliare le lacune legacy concrete; prove scheduler/recovery/performance; verifiche distributive e licenze complete.
6. Congelare il candidato, regressione generale appropriata una volta e prova portable su Windows pulito. La promozione1.0 richiede i gate superati e autorizzazione del proprietario.

## Spazio e conservazione

Oltre a47cartelle obsolete di test(384MiB), il18novembre sono state spostate11cartelle `workspace-flow/*/data`: **41,967GiB logici** in `da_eliminare/test_temporanei_20261118`. `sviluppo/tests` è scesa a **circa4,00GiB logici**; soprattutto collaudo autentico lungo e fixture necessarie. Deduplicata inoltre la copia identica di Gemini2(1,368GiB): SHA256 e timestamp verificati, percorso fixture conservato tramite hardlink al sorgente esistente, raw/proxy/export intatti; la vecchia copia è in quarantena e libererà spazio dopo lo svuotamento.

Il grande video era un hardlink al master personale, verificato ancora presente:41,967GiB non sono spazio fisico appena liberato. Lo svuotamento di `da_eliminare` compete al proprietario. Sono stati preservati raw e risultati unici costosi; Debug assente. I nuovi run sintetici riusciti compattano dati e copie EXE, anche se il test workflow viene avviato singolarmente. Nessuna pulizia giustifica una nuova inferenza o un render lungo.

## Regole di accettazione

PASS tecnico, verifica UX, inferenza reale, ascolto e accettazione artistica sono esiti distinti. Un test simulato prova il contratto, non la qualità del modello. Le funzioni non si dichiarano complete da un pulsante o importatore: vanno dalla GUI all'output e alla riapertura. Nessun limite creativo arbitrario; limiti tecnici reali mostrati e correggibili. Nessun download automatico, cancellazione di modelli condivisi o intervento sui servizi altrui.
