# NL Video Suite · sperimentale avanzata verso 1.0

Suite video locale in italiano: montaggio multitraccia, video lunghi→shorts, sottotitoli, motion graphics editabili, foto/video/musica, copioni e regia assistita con Ollama/API locali e ComfyUI. Quattro workspace condividono libreria, timeline, mixer, job ed export. Manuale utilizzabile senza IA; nessun modello scaricato automaticamente.

**Non è ancora la release1.0 stabile.** Il candidato sperimentale mantiene internamente versione0.7.1; stato e limitazioni sono documentati, senza attribuire vecchie regressioni al nuovo codice.

- [Scarica la prerelease Windows x64](https://github.com/Nicotina4ever/NL-Video-Suite/releases).
- [Avvio, dipendenze e uso](documentazione/USO_E_INSTALLAZIONE.md).
- [Stato verificato e cosa manca per1.0](documentazione/STATO_E_ROADMAP_1_0.md).
- [Architettura e sviluppo](documentazione/ARCHITETTURA_E_SVILUPPO.md).
- [Specifiche complete1.0](sviluppo/roadmap/V1_MASTER_ROADMAP.md) · [Ripresa dal checkpoint](sviluppo/roadmap/V1_SESSION_HANDOFF.md).

## Avvio

Estrai il pacchetto **Experimental Core**, configura FFmpeg se non già disponibile con `Configura_PC.cmd`, poi fai doppio clic su `Avvia_NL_Video_Suite.cmd`. Il Core contiene l'EXE avanzato e la GUI; runtime Python, FFmpeg e checkpoint non sono inclusi. Riusa componenti già installati oppure installali esplicitamente secondo la guida. Non è presentato come portable completo con ASR già pronto.

Nell'installazione di sviluppo esistente lo stesso launcher permette Stabile/Anteprima aggiornata con dati separati. Il pacchetto pubblico avvia soltanto la sperimentale isolata: non promuove né sovrascrive una stabile.

## Percorsi frequenti

- Foto→copione: Montaggio→Scrivi dalle foto. Foto/video/musica→sequenza: Crea montaggio.
- Un'idea animata, non un singolo template: Montaggio→+Aggiungi→Grafica o animazione→proposta→revisione→livelli nativi→approvazione/export.
- Clip/copertine: Shorts e Social; trascrivi e continua dove serve, o intervalli manuali.
- Storyboard, personaggi, reference e audio-first: Storie e Regia AI. Import JSON API e Doctor usano la stessa Generazione.
- Rimozione completa dei file propri di un progetto: Progetti e materiali→Elimina progetto…→inventario→conferma.

## Sorgenti e licenze

Rust per il server/editor, interfaccia incorporata, FFmpeg per i media, un bridge Python condiviso dove necessario. Il clone non contiene dati personali, grandi asset di test, cache, modelli o runtime. Compila con `cargo build --release --manifest-path sviluppo/Cargo.toml` dopo aver installato Rust/build tools. Il build iniziale può richiedere le dipendenze Cargo, non checkpoint IA.

Il codice della suite è distribuito con [licenza MIT](LICENSE); componenti terzi, modelli e font conservano condizioni proprie. [Distinta componenti](licenses/THIRD_PARTY.md). La redistribuzione completa dei runtime/FFmpeg/modelli richiede verifiche ulteriori ed è un gate aperto della1.0. Il Core non li ridistribuisce.
