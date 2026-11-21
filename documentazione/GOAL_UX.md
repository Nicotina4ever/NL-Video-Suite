# GOAL — NL Video Suite davvero usabile (Premiere-like)

**Criterio di uscita (misurabile):** *GTA6 → taglio → correzione trascrizione → mix → reframe 9:16 → copertina → export*, senza aprire un `<details>` nascosto e senza cambiare interfaccia.

Regola: una voce = un commit, spuntata solo con prova visibile (screenshot, output API, test).
Riferimenti di comportamento: Adobe Premiere Pro 2025 (solo layout/scorciatoie/comportamento, mai codice: workspace XML e `.kys` ufficiali) e `reference_software/filmcraft` (pattern UI, attribuzione in `licenses/THIRD_PARTY.md`).

## R0 — Fondamenta
- [x] Un solo prodotto: UI classica (ha tutte le funzioni e l'IA); shell Svelte = laboratorio
- [x] Backend già capace: reframe volti, sfondo sfocato, mixer a bus, sottotitoli, FTS

## R1 — Layout e anteprima solida
- [x] Anteprima nitida: canvas fino a 1280 px lato lungo (9:16 a 608×1080), ridisegno al resize
- [x] Splitter: persistenza, doppio clic/Home = reset; pavimento 320 px per il monitor, la pagina scorre su finestre basse
- [x] Mixer sempre raggiungibile (primo pulsante della toolbar + Shift+6)

## R2 — Mixer con console vera
- [x] Strip verticali per Voce/Musica/Effetti/Master con fader e dB sincronizzati
- [x] Meter live dagli analyser nel grafo Web Audio esistente
- [x] Mute/Solo, routing tracce, automazione; apertura da pulsante e Shift+6
- [ ] Salvataggio e ascolto verificati su progetto reale (percorso esistente, non ancora riprovato)

## R3 — Trascrizione clicca-e-scrivi
- [x] Doppio clic su parola → testo editabile inline
- [x] Invio/blur salva, Esc annulla, testo vuoto rifiutato
- [x] Salvataggio automatico (900 ms) e ripristino del testo originale che elimina la correzione
- [x] Suggerimento visibile sopra l'elenco

## R4 — Waveform e audio scollegabile
- [x] Forma d'onda reale sulle clip video in timeline
- [x] Interruttore Waveform persistente
- [x] «Scollega audio» (Ctrl+L): clip audio indipendente, video muto
- [x] «Ricollega»: rimuove la clip audio e ripristina il volume
- [x] Backend: video con audio ammesso sulla traccia audio (video muto rifiutato) + test

## R5 — Comandi facili
- [x] Palette comandi (Ctrl+Shift+P) con tutte le azioni del pannello attivo, barra e navigazione
- [x] Scorciatoie: V, C, S, Spazio, J/K/L, M, Ctrl+Z/Y (shell), Ctrl+L (scollega), Shift+6 (mixer)
- [ ] Tooltip con la scorciatoia su ogni pulsante (la palette mostra i title esistenti)

## R6 — Verifica e consegna
- [x] Test Rust 194/194 + sintassi JS dei moduli toccati
- [ ] Promozione canale stabile con ricevuta
- [ ] MANUALE/STATO aggiornati

## R7 — Copertina AI (Nano Banana-like)
- [x] Selettore workflow con input testo + reference immagine (narrative-controls)
- [x] Frame scelto dal playhead o dal tempo della sorgente, catturato a piena risoluzione
- [x] Descrizione + «prendi il parlato qui» come contesto (verificato su GTA6)
- [x] Genera dal frame, review, «usa come copertina» (payload verificato; generazione non avviata senza consenso)
- [x] Verifica payload: `7656.input_text`, `7658.image`, frame reale `4eabbb3c…`, 2 asset nel progetto di prova
