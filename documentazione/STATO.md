# STATO — punto di ripresa (aggiornato 21/11/2026, sera)

Documento vivo di una pagina. Uso: `MANUALE.md`. Tecnica: `ARCHITETTURA.md`. Storia completa (non autoritativa): `documentazione/archivio/` e git.

## Dove siamo nel piano di restauro

✅ Fase 0 sicurezza (lavoro Codex committato `96cc5d5`, tag `restore-base-20261121`) · ✅ Fase 1 dolori (durata reale verificata sul GTA6, rename cartelle, chunk silenziosi rendicontati) · ✅ Fase 2 indice SQLite FTS + ricerca + QA REA sullo schema reale · ✅ Fase 3a/3b shell (7 slices: timeline canvas, filmstrip, banda waveform, monitor con playback, Trascrizione con correzioni IA ancorate e traduzioni, Shorts, Ispettore+Sorgente, algebra ripple/roll/slip/slide con 8 test, servita dal binario su `/shell`) · ✅ R1 coda-risorse (mai più rifiuti per RAM), R2 pulsante Libera RAM/VRAM (testato senza toccare la generazione ComfyUI in corso), R3 GPU→CPU onesto, R5 razor C · ✅ promozione canale stabile al binario certificato (backup + ricevuta) · 🟡 Fase 4 (tre documenti vivi creati; archiviazione in corso) · ⬜ Fase 5 (real_playback + regressione proporzionata quando ComfyUI è libero, sessione REA completa, tag finale).

## Verde aggiornato (su sorgenti attuali)

- fmt · clippy -D warnings · **193 unit test** · algebra test **8/8** · svelte-check **0/0**
- baseline mirato `v1-baseline/20261121T142020Z` **5/5** (longform_chapters, project_delete/rename_browser, transcript_assistant, asr_guided) — SHA `ade48793…` = exe promosso a stabile e usato anche per l'anteprima 8099 (poi ricostruito con le aggiunte shell; ricertificare prima del prossimo uso del launcher).
- live verificato oggi su dati reali GTA6: durata/copertura, rename, slip ±frame con restore byte-esatto, correzioni manuali round-trip, shorts manual+cleanup, riproduzione Programma, shell da `/shell`.

## Prossimo lavoro verificabile

1. **Ricertificare** l'exe corrente (`v1_baseline --only longform_chapters project_delete_browser project_rename_browser transcript_assistant asr_guided`) dopo l'ultimo rebuild e riaprire il launcher come prima cosa.
2. **Fase 5 quando ComfyUI è libero**: `real_playback`, sottotitoli nella shell (pannello + cue), filmstrip con frame per tempo (oggi poster unico per clip), Sorgente avanzato (trim drag), poi regressione proporzionata + tag `restore-YYYYMMDD`.
3. **Prova reale IA** (Ollama acceso) sulle correzioni GTA6: proposta→approvazione→verifica che il raw resti; aggiungere al dossier.
4. Porting progressivo pannelli classici rimanenti (Motion mixer regia) nella shell → adapter o riscrittura, uno per commit, UI classica sempre autoritativa finché non coperto.
5. Pulizia finale `da_eliminare` a svuotamento del proprietario (~545 MB reali già isolati con note).

## Avvio rapido da memoria

```powershell
# GUI stabile o anteprima (launcher, porte 8088/8099, dati separati):
.\Avvia_NL_Video_Suite.cmd
# shell:  http://127.0.0.1:8099/shell/   (servita dal binario, niente Node)
# dev shell:  sviluppo\tools\Avvia_Shell.ps1
```
