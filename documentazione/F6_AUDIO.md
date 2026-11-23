# F6 — Musica con ComfyUI

Data: 2026-11-23. Comando:

```json
{"command":"audio.generateMusic","params":{
  "prompt":"warm cinematic soul, 92 bpm, Rhodes piano, brushed drums",
  "lyrics":"[Verse]\n…","seconds":30,"seed":7,"wait":false}}
```

Workflow predefinito: `workflows/regista/audio/audio_ace_step1_5_xl_sft.json`.
Per MiniMax Music passare `workflow` esplicito
(`workflows/regista/audio/MiniMax Music.json`): il prompt riempie `caption` e le parole
riempiono `lyrics`; il resto del workflow resta quello dell'operatore.

## Cosa fa

- Usa lo stesso bridge ComfyUI di F5: submit, polling, download e import nel progetto.
- Imposta `duration`/`max_duration` quando il workflow li espone e `seconds` è richiesto.
- Un job in background, una voce in Progress, un passo di undo per l'import.

## Limiti

- La qualità dipende dal workflow e dai modelli installati; la suite non li cambia.
- Il testo (quando il workflow lo prevede) è una proposta da ascoltare, non un risultato
  approvato.
- Nessun missaggio automatico in questa fase: il file importato resta materiale grezzo nel
  Project panel.

## Verifica

In corso con Ace Step 1.5 (30 s) su progetto demo; ricevuta in `documentazione/prove/f6-audio/`.
