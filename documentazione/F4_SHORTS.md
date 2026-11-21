# F4a — Find Shorts locale

Data: 2026-11-21. Commit F3a di riferimento: `0133016c`.

## Comando

`Sequence > Find Shorts` oppure:

```json
{"command":"sequence.findShorts","params":{"minSeconds":10,"maxSeconds":45,"count":5}}
```

Richiede una sequenza aperta e una trascrizione con timing di parola. Non chiama servizi
esterni: è il passo locale e deterministico del montaggio AI, dichiarato come tale.

## Cosa fa

1. Raggruppa le parole in paragrafi (pausa > 1,2 s) e costruisce finestre da `minSeconds` a
   `maxSeconds`, preferendo la chiusura del paragrafo.
2. Assegna un punteggio 0–100 a durata, hook iniziale, autonomia dal contesto, chiusura di frase e
   ritmo del parlato.
3. Deduplica le finestre sovrapposte (oltre il 55% della più corta).
4. **Rifinisce i confini** nel silenzio misurato tra le parole: lead massimo 120 ms, coda massima
   280 ms, senza invadere la parola precedente/successiva, snappati al frame.
5. Crea una **sequenza per candidato** nel bin `AI Shorts` del Project panel, copiando le porzioni
   di tutte le tracce (video, audio, sottotitoli) interessate dal taglio, con link e trim.
6. Aggiunge un **range marker** sulla sequenza sorgente per ogni candidato (colore Rose), così i
   tagli si rivedono anche in Timeline/Markers.
7. Tutto in un solo passo di undo.
8. Una seconda esecuzione sugli stessi candidati non duplica nulla: le sequenze già presenti
   in `AI Shorts` vengono saltate e il comando resta un no-op (errore leggibile, nessun marker
   duplicato).

## GTA6

- Trascrizione importata: 377 parole sulla timeline.
- `minSeconds 10, maxSeconds 45, count 5` → **2 short**:
  - `Short 01` 0–30,2 s, parole 0–73, 907 frame;
  - `Short 02` 28,8–58,5 s, parole 290–376.
- Markers sulla sorgente: 2; bin `AI Shorts`: 2 sequenze.
- Aprendo `Short 01`, il Text panel mostra **74 parole** (le stesse del candidato) e il Program
  monitor mostra il footage.
- 0 media copiati: le sequenze condividono i media originali.

Prove: `documentazione/prove/f4-shorts/source-markers.png`,
`short-sequence.png`, `short-transcript.png`.

## Export e persistenza

- `Short 01` esportato con `file.exportMedia` (H.264, NVENC, 8 Mbps): **1080×1920, 30 fps,
  H.264/AAC 48 kHz stereo, 30,25 s, 907 frame, 32.264.515 byte**, ffprobe in
  `short01-ffprobe.json`.
- Progetto salvato in `gta6-shorts.fcproj` e riaperto: nome, 2 marker sulla sorgente, bin
  `AI Shorts` con 2 sequenze e 377 parole di transcript sono tutti presenti.

## Limiti

- Il punteggio è euristico: non è un giudizio creativo né una previsione di visualizzazioni.
- Le transizioni tra clip non vengono copiate nelle sequenze derivate; i tagli sono netti.
- Le tracce sottotitoli vengono copiate se presenti nella sorgente.
- Il ranking LLM/hook verificabile (AutoClip) resta F4b; il formato di uscita è già pronto
  (`created[]` con `sequence`, `score`, `startWord`, `endWord`).
