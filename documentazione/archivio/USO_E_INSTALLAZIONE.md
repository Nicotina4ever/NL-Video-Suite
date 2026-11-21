# Uso e installazione

## Quale versione avviare

L'installazione di sviluppo locale usa `Avvia_NL_Video_Suite.cmd`: scegli Stabile oppure Anteprima aggiornata. L'Anteprima sceglie soltanto un candidato con hash e prova GUI corrispondenti; non compila all'avvio. Salva il lavoro prima di cambiare istanza. I dati della stabile e dell'anteprima restano separati.

Il pacchetto GitHub **Experimental Core Windows x64** contiene l'eseguibile avanzato, la GUI incorporata, avvio e guide, non Python/Torch, modelli, FFmpeg, Ollama o ComfyUI. È una distribuzione sperimentale leggera con dipendenze esplicite, non il portable completo1.0. Non introduce una seconda versione del motore e non disabilita funzioni nel codice: segnala i componenti mancanti.

Prerequisiti: Windows x64 e runtime Microsoft Visual C++ x64 compatibile, se non gia presente nel sistema. Il binario sperimentale non e firmato: verifica SHA256 del pacchetto dalla release ufficiale; firma/SmartScreen e Windows pulito sono gate ancora aperti.

1. Scarica lo ZIP della prerelease e verifica SHA256, poi estrai in una cartella scrivibile (anche con spazi). Non eseguire l'EXE dentro lo ZIP.
2. Per import/export/anteprime native serve FFmpeg e ffprobe. Se sono nel PATH o in `app/tools`, vengono riusati. Altrimenti `Configura_PC.cmd` permette di scegliere un `ffmpeg.exe` già installato nella stessa cartella di `ffprobe.exe`. Il riferimento è locale al pacchetto, nessun PATH globale viene modificato. Puoi ottenere FFmpeg manualmente dalla [pagina ufficiale](https://ffmpeg.org/download.html); nessun componente viene scaricato dalla suite.
3. Doppio clic su `Avvia_NL_Video_Suite.cmd`. Nel Core si apre direttamente la build sperimentale; non sostituisce alcuna stabile. I progetti nuovi sono in `data/projects`. `Ferma_NL_Video_Suite.cmd` riguarda soltanto questa installazione.
4. Senza servizi IA puoi già modificare manualmente timeline, titoli, Motion e sottotitoli ed esportare con FFmpeg. Se manca FFmpeg l'editor si avvia ma i processi multimediali richiedono di configurarlo.
5. Per IA configura il provider esistente in Sistema e modelli, seleziona modelli realmente installati e assegnali ai ruoli. Ollama è il default; API compatibili possono puntare a llama.cpp o LM Studio locali. Non sono inclusi checkpoint o server; la suite non li scarica.
6. I backend Python/WhisperX, allineamento e analisi locali richiedono anche un runtime compatibile, `engines` e modelli. Riusa la tua installazione configurando i percorsi da Impostazioni/`data/settings.json` ad app chiusa, oppure prepara esplicitamente il pacchetto completo dalla tua installazione con il packager. Il Core non contiene quei9GiB e non promette ASR out-of-the-box. Le dipendenze sono in `sviluppo/bridge/requirements.txt` quando presenti; vedi il riferimento tecnico. Non ricostruire un ambiente aggiornando pacchetti casualmente.

## Quattro aree, un progetto

- **Progetti e materiali:** crea/seleziona progetto, importa copie dei media, consulta la libreria. `Elimina progetto…` mostra i file propri da rimuovere e richiede conferma; originali esterni e modelli restano intatti. Job attivi o riferimenti reali da altri progetti bloccano l'eliminazione.
- **Montaggio:** libreria, monitor, timeline. Seleziona una clip→Regola clip. Trascina la maniglia per estendere le tracce; il cursore viene seguito durante play. `Crea montaggio` riusa il sequenziatore foto/video/musica; `Scrivi dalle foto` lavora sul copione senza imporre prima un video di durata prestabilita. Sottotitoli, grafica e audio sono strumenti contestuali, non progetti separati.
- **Shorts e Social:** scegli il video, rivedi candidati/tagli/crop, crea la copertina e la consegna. Se manca la trascrizione, usa l'azione contestuale Trascrivi e continua; non è obbligatoria per un intervallo manuale.
- **Storie e Regia AI:** materiali, personaggi, ambienti, continuità, copione e shot; regia audio-first per audio master e reference temporali; Dossier produzione e generazione selettiva. Sistema, lavori e pulizia restano accessibili dagli strumenti comuni.

## Dall'idea all'animazione

Montaggio→+Aggiungi→**Grafica o animazione…**. Scrivi cosa deve comunicare la composizione, dati/testi verificati, tono e materiali pertinenti. `Proponi l'animazione` usa il modello del ruolo Grafiche e restituisce scene revisionabili. Puoi cambiare testi, durate, colori, ingressi, ordine e selezionare quali scene inserire. Le composizioni precedenti non vengono sovrascritte.

Le immagini mancanti devono essere collegate dalla libreria prima di inserire i livelli che le usano. La preparazione di una risorsa trasferisce il prompt al generatore comune ma **non** lo invia. Per video si sceglie un workflow compatibile e si incolla il prompt disponibile. Audio/video generati si revisionano nelle rispettive tracce. Una resource request non è un media già prodotto.

`Usa il mio testo senza IA` anima letteralmente le tue parole; non interpreta la richiesta. I preset, classifica compresa, sono sotto **Parti da un preset oppure crea manualmente**. Dopo l'inserimento: prepara preview→rivedi a velocità normale→approva→esporta. Testi, forme, immagini e keyframe restano modificabili.

## Workflow ComfyUI

Storie e Regia AI→Workflow e riparazioni/Generazione→Aggiungi workflow API(.json). Importa il formato API di ComfyUI, non il JSON dell'editor con nodes/links. Nessun workflow è eseguito all'importazione. Seleziona il grafo, associa esplicitamente prompt e ingressi immagine/audio/video; il Doctor propone copie e modifiche deterministiche. Preflight verifica schema reale, modelli/nodi disponibili e collegamenti. Non può certificare la consistenza artistica di una reference da un semplice tipo IMAGE. Non installa nodi o modelli e non interrompe altre code.

## Altri PC, dati e ripresa dello sviluppo

Il clone Git contiene sorgenti e documentazione, non media privati, fixture pesanti, interpreter, modelli o cache. Il codice UI è incorporato nel binario: per modificare JS/CSS/HTML occorre ricompilare. Su Windows x64 con Rust e build tools installati:

```powershell
git clone https://github.com/Nicotina4ever/NL-Video-Suite.git
cd NL-Video-Suite
cargo build --release --manifest-path sviluppo/Cargo.toml
```

Per avviare dal clone, dopo il build esegui `sviluppo/target/release/nl-video-suite.exe --port 8088 --data-dir ./data` dalla root e apri `http://127.0.0.1:8088`. Le cartelle placeholder runtime/workflows rendono individuabile la root senza installare Python o pesi. FFmpeg deve essere nel PATH o configurato.

Il primo build può richiedere le dipendenze Cargo; `--offline` funziona soltanto se sono già disponibili. Non scarica modelli IA. Leggi il Session Handoff e lo stato operativo prima di lavorare. Non usare una vecchia regressione come certificazione delle modifiche nuove. Alcuni collaudi creativi storici richiedono fixture private: non sono falsamente inclusi nel clone e non devono essere rigenerati per ottenere un numero verde.

Trasferire una tua installazione completa è distinto dal Core pubblico: riusa `app/crea_portable.py --verifica` e crea il pacchetto solo su richiesta esplicita. Il pacchetto completo senza `--con-dati` esclude i tuoi progetti. Il trasferimento dei dati è facoltativo e richiede la suite ferma. Dopo ricollocazione riconfigura riferimenti a servizi, modelli e font esterni; non eliminare la vecchia installazione prima di una riapertura/export verificati.

## Trasferire la sperimentale con tutte le risorse gia disponibili

Per il proprio altroPC, estrai il Core e conserva il suo `app/nl-video-suite.exe` avanzato. Affianca una sola copia delle cartelle `runtime`, `models` e dei modelli opzionali `engines/clipsai/models` dalla tua installazione completa; configura i servizi e i percorsi esterni sul PCdi destinazione. I sorgenti dei bridge/motori sono gia nel Core. Importa i workflow API pertinenti dalla GUI, invece di riutilizzare percorsi assoluti della vecchia macchina. Trasferisci `data/projects` soltanto se vuoi anche i tuoi progetti, con la suite ferma. Non sovrascrivere l'EXE avanzato con la vecchia stabile. Nessun modello viene scaricato da questa operazione.

Il packager completo attuale usa per default l'EXE di `app/`, cioe la stabile dell'installazione locale: non scambiarlo per la distribuzione sperimentale avanzata. Il Corepiu risorse esplicite evita tale ambiguita. Verifica riapertura/preview/export prima di eliminare la vecchia copia. La redistribuzione a terzi del pacchetto completo richiede ancora i gate di licenza e packaging della1.0.
