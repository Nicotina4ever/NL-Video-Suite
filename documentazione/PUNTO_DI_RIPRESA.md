# Punto di ripresa — pausa del 23/11/2026

Stato congelato su richiesta dell'operatore. Tutto è committato: nessuna modifica pendente
(`git status` pulito in `filmcraft-nl` e in `documentazione`), nessun job in esecuzione, nessun
servizio avviato dalla suite.

## Dove siamo arrivati

- **Fasi chiuse**
  - P1 impostazioni/pannelli, P3a clipping+sgrossa, P4b copertine, P8a inventario versione test,
    P8b default rapido, P9a workflow doctor+API+import, P9b Prompt AI/H3/parametri/dry run,
    P8d storia AI con libreria reference e timeline, P9d reference media + video scena + musica,
    P8c longform (capitoli+marcatori) e ricerca progetto.
- **Commit fork principali** (in `filmcraft-nl`, gli ultimi in cima)
  - `208dc2d8` P8c longform + ricerca (pannello **NL Analyze**)
  - `7cd91d24` P8d libreria reference (sheet/plate → keyframe di scena)
  - `7aff8f47` P9b compilatore H3 + chat testuale Ollama
  - `a4abc203` P9b prompt AI + parametri workflow + dry run
  - `826a1d5d` P9d reference media + video da keyframe + musica di scena
  - `f602e3a3` P8d storia AI → scene → keyframe → timeline
  - `660f4ad9` P8b/P9a quick edit + doctor/API; `4927a196` P4b; `c46162f5` P3a; `d9102850` P1
- **Documenti vivi**: `HANDOFF.md`, `PARITA_FEATURE.md`, `TRACCIA_PARITA.md`, `TODO.md`,
  `STATO_INTEGRAZIONE.md`, `PERFORMANCE.md`, `inventario/*`, `prove/*`.
- **Evidenze live più recenti**
  - `prove/p8c-longform/` (46 parole → 6 blocchi → 2 capitoli con qwen3.5:9b, 2 marcatori)
  - `prove/p8c-search/` (21 documenti, 3 query con risultati attesi)
  - `prove/p8d-story/` (storia 10 scene, keyframe 512, place in timeline; libreria sheet/plate in
    dry run), `prove/p9b-prompt-params/` (prompt AI, H3 base+full-reference, dry run v2v 23 nodi,
    78 parametri), `prove/p9d-reference/` (reference image live, rifiuto video senza input).
- **Verifiche**: `cargo fmt --check`, clippy `-D warnings` (engine/ui/cli) puliti; **engine 1008+17
  test**, **UI 368 unit + 4 + 1 + integrazione** verdi. Due test real-time
  (`marked_playback_position`, `slip_monitor_ui`) sono flaky sotto carico e passano isolati.

## Cosa manca (ordine proposto alla ripresa)

1. **P9c** — coda varianti (seed deterministici, prepare/invia con conferma, dedupe, `/free`
   controllato ogni N) e modifica interattiva dei widget del workflow.
2. **P9d resto** — generazione **video live** con i workflow v2v/r2v copiati (H3 richiede offload:
   modelli int8/int4 ~20 GB su 8 GB VRAM) e prova audio live Ace Step; estensione sezioni.
3. **P8c resto** — assistente trascrizione con citazioni (l'indice esiste: `nl.search.*`), export
   MD/JSON dei capitoli, traduzioni EN/ES (import SRT/VTT già coperto da `captions.import`),
   topics/rough-cut.
4. **P8d resto** — montage planner/creative montage/audio onset, shot map, pipeline/flows.
5. **P8e** — subtitle studio, motion/titoli, graphic objects, narration/audio master.
6. **P8f** — servizi/versioni/album/production package/storage/social + thumbnail nativa (con P6).
7. **P2/P3b/P5 residui**, poi **P7** — release `cargo build --release` con tutti i pannelli,
   misure performance in `prove/p7-performance/`, regressione GTA6 (solo lettura), ricevute,
   pulizia finale della cache debug e spazio.

## Come riprendere

- Riprendere il goal attivo (azione `resume`) e continuare da **P9c**, poi P9d resto.
- Riferimenti: `documentazione/TRACCIA_PARITA.md` (piano P1–P9), `TODO.md` (stato), questo file.
- Regole invariate: sorgenti esterni in sola lettura, progetti utente intatti, mai Ollama e ComfyUI
  insieme, niente download obbligatori, niente "FilmCraft" nei testi visibili, test mai indeboliti.
