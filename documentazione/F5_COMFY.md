# F5 — ComfyUI nel motore

Data: 2026-11-23. Comandi headless (visibili anche nei menu):

```json
{"command":"graphics.generateComfy","params":{
  "prompt":"A cinematic portrait…",
  "workflow":"workflows/regista/image/z-image-turbo.json",
  "negative":"…","seed":1,"width":1024,"height":1024,
  "wait":false,"timeoutSeconds":1800}}
```

`audio.generateMusic` usa lo stesso motore con il workflow audio predefinito
`workflows/regista/audio/audio_ace_step1_5_xl_sft.json` e i parametri `prompt`, `lyrics`,
`seconds`, `seed`.

## Cosa fa

1. Legge un workflow in formato API ComfyUI dal percorso scelto (relativo alla radice della suite).
2. Riempie il prompt in modo esplicito e verificabile:
   - sostituisce i token `{{prompt}}`, `{{negative}}`, `{{lyrics}}`, `{{seed}}`;
   - altrimenti usa il nodo il cui titolo contiene `Positive Prompt` o `CLIP Text Encode (Prompt)`;
   - per l'audio riempie `caption` (MiniMax) o `tags` (Ace Step) e, se richiesto, `lyrics`.
3. Imposta seed e, se richiesti, larghezza/altezza e durata audio.
4. Invia a `/prompt`, segue `/history/{id}`, scarica i file da `/view` e li importa nel Project
   panel con un solo passo di undo. Il job è in background e compare nella finestra Progress.
5. Non decide il modello: usa quello scritto nel workflow dell'operatore. Non scarica pesi.

## Limiti dichiarati

- Solo workflow in formato API; un workflow salvato dall'interfaccia ComfyUI (formato UI) va
  convertito o esportato in API.
- Il prompt deve essere collegato a un nodo testuale riconoscibile; un workflow senza slot
  testuale fallisce con un messaggio esplicito.
- Nessun upload di reference frame in questa fase: `Generate Cover` da frame e reframe verticale
  restano da collegare al bridge.
- La coda è quella di ComfyUI: la suite non uccide job e non libera modelli di altre app.

## Verifica

In corso con `z-image-turbo` su progetto demo; ricevuta in `documentazione/prove/f5-comfy/`.
