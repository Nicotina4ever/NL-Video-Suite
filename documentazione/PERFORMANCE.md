# Performance e default rapido

Requisito operatore (23/11/2026): **di base il programma deve restare molto rapido per i montaggi
veloci**, pur contenendo tutte le funzioni di FilmCraft e di NL Video Suite. Le funzioni AI si
aprono quando servono, non pesano sull'avvio né sull'editing.

## Cosa è garantito oggi

- **Nessun pannello NL agganciato di default**: il layout di avvio resta **Editing** (il layout
  familiare dell'editor) e nessuna workspace built-in contiene un pannello NL, quindi aprire un
  progetto non costruisce né disegna nulla di AI. Per un layout ancora più essenziale c'è
  **Quick Edit** (Window ▸ Workspaces ▸ Quick Edit: Program, Project, Effects, Timeline, meters). I
  pannelli NL si aprono da **Window** o dalla palette comandi quando servono.
- **Nessun lavoro AI all'apertura**: aprire un progetto non crea job, non contatta ComfyUI/Ollama,
  non legge `/object_info` e non analizza nulla. Test:
  `nl::performance_tests::opening_a_project_starts_no_ai_work`.
- **Comandi su richiesta**: i workflow vengono letti/classificati solo quando si apre il pannello o
  si genera; la lista è in cache per pochi secondi; `/object_info` viene letto una volta da
  **Verify** e riusato dal doctor finché la sessione vive (`s.nl.object_info`).
- **Analisi pesanti in background, cancellabili**: clipping, sgrossatura, ASR e generazioni girano
  come job con avanzamento e `jobs.cancel`; l'editing non si blocca.
- **GPU/CPU solo quando chiesto**: nessun servizio viene avviato o fermato automaticamente; Ollama e
  ComfyUI non vengono mai sovrapposti dalla suite.

## Misure da registrare (P7, release)

Con `filmcraft.exe` release, su questa macchina (RTX 4060 Ti, SSD):

| Misura | Metodo | Obiettivo |
|---|---|---|
| Avvio a freddo (finestra pronta) | `Measure-Command` dal launcher, 3 prove | ≤ 2,5 s |
| Apertura progetto demo | comando `file.open` + prima UI | ≤ 1,0 s |
| Apertura progetto GTA6 (377 parole) | idem | ≤ 3 s |
| Cambio workspace Quick→Editing | `window.workspace.editing` | ≤ 0,3 s |
| Nessun job all'apertura | `nl::performance_tests` | sempre |

Le misure e la ricevuta vanno in `documentazione/prove/p7-performance/` quando la release finale è
compilata; se un obiettivo non viene raggiunto si documenta il valore reale, senza dichiararlo
raggiunto.

## Regole di sviluppo che tengono il programma rapido

1. Un pannello non deve fare lavoro (I/O o rete) nel primo frame se non è visibile.
2. Le liste costose (workflow, modelli) si leggono su richiesta e si tengono in cache con scadenza.
3. Nessuna funzione deve avviare un job o contattare un servizio come effetto collaterale
   dell'apertura di un progetto o di un pannello.
4. Le feature pesanti (longform, narrative, montage planner, generazione) restano comandi/pannelli
   espliciti.
