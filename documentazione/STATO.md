# STATO — punto di ripresa (aggiornato 21/11/2026, sera)

Documento vivo di una pagina. Uso: `MANUALE.md`. Tecnica: `ARCHITETTURA.md`. Storia completa (non autoritativa): `documentazione/archivio/` e git.

## Dove siamo nel piano di restauro

✅ Fase 0 sicurezza (lavoro Codex committato `96cc5d5`, tag `restore-base-20261121`) · ✅ Fase 1 dolori (durata reale verificata sul GTA6, rename cartelle, chunk silenziosi rendicontati) · ✅ Fase 2 indice SQLite FTS + ricerca + QA REA sullo schema reale · ✅ Fase 3a/3b shell (7 slices: timeline canvas, filmstrip, banda waveform, monitor con playback, Trascrizione con correzioni IA ancorate e traduzioni, Shorts, Ispettore+Sorgente, algebra ripple/roll/slip/slide con 8 test, servita dal binario su `/shell`) · ✅ R1 coda-risorse (mai più rifiuti per RAM), R2 pulsante Libera RAM/VRAM (testato senza toccare la generazione ComfyUI in corso), R3 GPU→CPU onesto, R5 razor C · ✅ Fase 4 documenti tre vivi + archivio + reinstradamento riferimenti · 🏷️ tag `restore-fase0-4-20261121` · ⬜ Fase 5 (sottotitoli shell, prove gate Comfy libero, sessione REA completa, tag finale).

## Verde aggiornato (su sorgenti attuali)

- fmt · clippy -D warnings · **193 unit test** · algebra test **8/8** · svelte-check **0/0**
- baseline mirato `v1-baseline/20261121T152652Z` **5/5 PASS** (longform_chapters, project_delete/rename_browser, transcript_assistant, asr_guided) — SHA `2576e868…` = binario **promosso a stabile** (2ª promozione, backup della precedente; la prima portava uno sha senza `/shell`) + smoke stabile `/shell` 200, progetto rinominato che risolve, thumbnail ok, 0 warning.
- live verificato oggi su dati reali GTA6: durata/copertura, rename con ricevuta doppia, slip ±frame con restore byte-esatto, correzioni manuali round-trip, shorts manual+cleanup, riproduzione Programma, shell da `/shell`, razor C.

## Prossimo lavoro verificabile

1. **Fase 5a**: pannello Sottotitoli nella shell (cue, stile, export) — poi Motion/mixer/regia con lo stesso pattern.
2. **Fase 5b quando ComfyUI è libero**: `real_playback` + regressione proporzionata (`v1_baseline` completo senza --only).
3. **Prova reale IA** (Ollama acceso) sulle correzioni GTA6: proposta→approvazione→verifica che il raw resti intatto; archiviare nel dossier.
4. **Sessione REA completa** sul reale (apri→trascrivi→cerca→elimina QA→riapri) appena la finestra tool è stabile, + filmstrip per-time con `/frame`.
5. Pulizia finale: `da_eliminare` (~545 MB reali + EXE promozioni vecchie) a svuotamento del proprietario; riconfigurare launcher se compare un nuovo exe non certificato.

## Avvio rapido da memoria

```powershell
# GUI stabile o anteprima (launcher, porte 8088/8099, dati separati):
.\Avvia_NL_Video_Suite.cmd
# shell:  http://127.0.0.1:8099/shell/   (servita dal binario, niente Node)
# dev shell:  sviluppo\tools\Avvia_Shell.ps1
```
