# Provenance — workflow Prompter copiati nella libreria (traccia in git)

Data: 23/11/2026. La cartella `workflows/` è esclusa da git (`/workflows/*`), quindi i file copiati
vivono sul disco ma non nel repository; qui resta la traccia verificabile. Originali in
`K:\AI\Prompter_Comfyui\workflows\` (letti in sola lettura, **mai modificati**), copiati su richiesta
esplicita dell'operatore ("copiati i workflow presenti che sono tra i principali che intendo
utilizzare, per adesso") in `K:\AI\NL_Video_Suite\workflows\prompter\`.

| File | Nodi | Uso previsto |
|---|---|---|
| `apex_h3_character_sheet.json` | 151 | character sheet / reference coerenti (immagini) |
| `krea2_turbo.json` | 22 | immagini veloci (Krea 2 turbo) |
| `MiniMax Music.json` | 12 | musica (MiniMax) |
| `minimax_h3_video_reference.json` | 27 | video reference-to-video (r2v) |
| `minimax ref all V2V.json` | 23 | video-to-video con reference |
| `Minimax-H3_-_V2V_Extend_Any_Video_latent_masking.json` | 100 | estensione video (v2v extend) |
| `qwen_reference.json` | 167 | reference/editing immagini (Qwen) |
| `video_minimax_h3_r2v (2).json` | 32 | video reference-to-video |

La verifica live (23/11/2026, ComfyUI 0.43.2, 5991 classi) dà **0 classi mancanti e 0 errori** per
questi 8 workflow; restano solo warning attesi (slot prompt da riempire, reference del template che
il pannello sostituisce). Evidenza: `documentazione/prove/p9-workflows/`.

Per aggiungerne altri: pannello **NL Workflows ▸ Import** (l'app copia in `workflows/nl-imports/`,
mai sovrascrivendo) oppure copiando il JSON in questa cartella. L'app non riscrive mai i file
originali: `nl.workflows.fix` produce una copia riparata nella cartella dati utente.
