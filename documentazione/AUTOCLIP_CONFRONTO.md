# AutoClip vs il clip di NL Video Suite

Data: 2026-11-21. Riferimento studiato: `reference_software/autoclip` (MIT).
Scopo: capire che cosa vale la pena portare nel nostro flusso clip, senza copiare codice o
wording di terzi (regola clean-room di FilmCraft) e senza sostituire ciò che già funziona.

## 1. Che cosa fa AutoClip

Pipeline completa, locale o con provider LLM:

1. **Ingest** da file o URL YouTube (`yt-dlp`).
2. **Trascrizione** word-level con faster-whisper; rilevamento silenzi con ffmpeg.
3. **Selezione momenti** con LLM a finestre sovrapposte (8 min, 60 s di overlap), citazioni per
   indice di parola, deduplica per IoU, refinement dei confini su frase + silenzio.
4. **Reframe speaker-aware**: FaceLandmarker, tracce volto, mouth-aspect ratio, diarizzazione,
   assegnazione speaker→volto, strategie per inquadratura (`track` / `wide` / `general`),
   sfondo sfocato quando il crop non è sicuro.
5. **Sottotitoli ASS** con 4 preset e animazione per parola.
6. **Export** 9:16, 1:1, 16:9 con fit/blur, loudness.
7. **Review UI** con trim, modifica didascalie, ranking.
8. **Ripresa dei job** per stage, con artefatti su disco.

## 2. Che cosa ha già NL Video Suite

| Area | Stato NL | File |
|---|---|---|
| Selezione momenti | ClipsAI (WhisperX + Ollama) produce `shorts`/`moments` con titolo, hook e score; in alternativa highlight/capitoli longform e finestre offline | `engine/clipsai/shorts.py`, `sviluppo/src/shorts_studio.rs` |
| Punteggio | Euristico trasparente, pesi modificabili, warning | `sviluppo/src/shorts_studio.rs` |
| Deduplica | Sovrapposizione temporale > 0.8 o Jaccard token > 0.92 | `shorts_studio.rs::dedup` |
| Reframe | Camera CPU v3: Haar, tagli di scena, dead-zone, limiti di velocità/accelerazione, modalità composta, fallback sfondo sfocato, cache | `sviluppo/bridge/camera_path.py`, `sviluppo/src/reframe.rs` |
| Sottotitoli | libass, unità ancorate alla sorgente, stili, `plain` / `word_highlight` / `reveal`, export VTT/SRT, proposta AI di stile | `sviluppo/src/subtitle_studio.rs`, `compositor.rs` |
| Export | 9:16 e social, fit/blur, loudness, coda | `sviluppo/src/compositor.rs`, `social.rs`, `audio_master.rs` |
| Review | Timeline, Text, Project, Markers di FilmCraft | fork `filmcraft-nl` |

Quindi non mancano le funzioni di base: mancano soprattutto **precisione dei confini** e
**leggibilità delle motivazioni** della selezione.

## 3. Da prendere (in ordine di valore)

### P0 — Refinement dei confini su parola e silenzio
AutoClip non taglia al timestamp grezzo dell'LLM: aggancia inizio/fine alla frase, corregge la
durata e sposta il taglio dentro un silenzio misurato (±0,75 s), con lead/tail brevi.
Da noi i momenti ClipsAI arrivano già come secondi e possono cadere a metà parola o dentro un
respiro.

**Da fare in Rust**, in `shorts_studio.rs` prima di creare i `Candidate`:
- agganciare `start`/`end` alla parola più vicina nella trascrizione attiva;
- cercare un intervallo di silenzio entro ±750 ms, riusando `silencedetect` (già usato dal bridge
  ASR) e mettendolo in cache per asset;
- rispettare `min_seconds`/`max_seconds` senza allungare per forza;
- conservare i range originali come evidenza per la revisione.

**Criterio di accettazione:** su GTA6 nessun candidato inizia o finisce a metà parola; i tagli
distano almeno 40 ms dal bordo del silenzio; i test coprono silenzio assente, silenzio parziale e
clamp di durata.

### P0 — Hook verificabile e motivazione
Il punto forte del prompt AutoClip è chiedere un hook **verbatim** e un motivo concreto
(hook → payoff), poi verificare che l'hook esista davvero nella trascrizione.
Da noi `Candidate` ha `title`, `evidence`, `producer`, `scores`, `warnings`, ma non un hook
verificato.

**Da fare:**
- aggiungere `hook` e `reason` a `Candidate` (campi opzionali, compatibili con i progetti salvati);
- verificare l'hook contro il testo del candidato (normalizzazione spazi/punteggiatura); se non
  combacia, warning visibile e nessuna approvazione automatica;
- mostrare hook, score e reason nel pannello di revisione (Project/Text) senza nuove finestre.

**Criterio di accettazione:** un hook inventato produce un warning e non viene presentato come
verificato; un hook reale è evidenziato come tale.

### P1 — Indici di parola stabili
Usare `start_word`/`end_word` come identità del candidato (oltre al tempo) rende deduplica,
modifica e round-trip più stabili quando la trascrizione viene corretta.
**Da fare:** salvare gli indici nel `Candidate` e ricalcolarli se la trascrizione cambia; usare
l'IoU sulle parole per la deduplica, mantenendo l'attuale controllo sui tempi.

### P1 — Preset didascalia "bold pop" e "boxed"
Noi abbiamo `plain`, `word_highlight`, `reveal`. AutoClip mostra che due preset in più coprono
bene l'uso social: parola attiva ingrandita e blocco pieno ad alto contrasto.
**Da fare:** aggiungere i preset nel modello `Style`/UI di `subtitle_studio`, senza cambiare il
renderer libass. I colori restano quelli NL (viola/magenta/ciano).

### P1 — Ripresa dei job per stage
Il pattern di AutoClip (artefatti per stage, retry che riparte dallo stage fallito) è utile per
ASR, analisi e render lunghi. Da noi un job fallito tende a ripartire da capo.
**Da fare:** checkpoint per stage nel job runner NL, con ricevuta e nessun doppio addebito di
tempo. F8, non F4.

### P2 — Reframe speaker-aware
AutoClip correla mouth-aspect ratio e diarizzazione per scegliere il volto che parla. La nostra
camera v3 è più prudente (preferisce blur quando il crop non è certificato), ma non sa *chi*
parla.
**Da fare solo se** l'ASR attivo espone già gli speaker (WhisperX/diarizzazione): in quel caso
mappare speaker→traccia volto una volta per clip, senza decidere frame per frame. Se gli speaker
non ci sono, non aggiungere MediaPipe solo per questo.

### P2 — Ingest da URL
AutoClip scarica da YouTube con `yt-dlp`. Per NL è opzionale: aggiunge una dipendenza esterna e
problemi di termini d'uso. Rinviato; prima finire il flusso file locale.

## 4. Da non prendere

- **Stack Python/MediaPipe/Whisper/provider**: abbiamo già ASR, Ollama e camera CPU.
- **Frontend web**: la UI è FilmCraft.
- **Codice o prompt copiati**: si reimplementano i comportamenti, non il testo.
- **Provider cloud con API key**: NL resta locale.
- **Quattro preset così come sono**: i nostri colori e il nostro renderer restano la fonte.

## 5. Ordine di lavoro consigliato

1. F4: refinement confini + hook verificabile + hook/reason in revisione.
2. F3/F5: preset didascalia NL e verifica export 1:1.
3. F8: ripresa job per stage.
4. Solo dopo: speaker-aware reframe e ingest URL.

Il confronto non cambia l'architettura: il motore clip resta quello NL, con FilmCraft come NLE.
