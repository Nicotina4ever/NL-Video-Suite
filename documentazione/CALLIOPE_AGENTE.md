# Agente di produzione — da Calliope a NL Video Suite

Data: 24/11/2026. Fonte studiata: `K:\AI\Calliope` (MIT), in particolare:
`calliope-backend/src/calliope/agent/`, `harness/`, `routers/workflows.py`,
`routers/story.py`, `README.md` (sezioni Agents, ComfyUI workflows, Agent settings).

Scopo: riportare in NL Video Suite la funzione utile di Calliope — un agente che aiuta a
costruire una storia o una serie di scene/oggetti e sceglie i workflow adatti — **senza copiare
codice o prompt**. Il comportamento è stato studiato e reimplementato sulle API già presenti in
`filmcraft-nl` (LLM locale, `nl.story`, `nl.agent`, workflow NL, ComfyUI).

## Cosa fa Calliope (comportamento)

- Un **agente di produzione** chat-driven: crea progetto, redige storia, sceneggiatura, asset e
  video; ogni chat è legata a un progetto.
- Un **planner** scompone il lavoro in sub-agenti: **story → script → assets → video**.
- Ruoli configurabili: Main agent, Planner, Story agent, Script agent, Assets agent, Video agent;
  ogni ruolo può usare un LLM diverso.
- I workflow ComfyUI sono scelti per **ruolo dei nodi** (`Input:prompt`, `Input:character`,
  `Output:video`, …) e abilitazione; il workflow video viene salvato sul clip.
- La generazione ha una **preview del prompt** prima di inviare il job; `Generate all` rispetta i
  setup salvati; gli errori di ComfyUI sono riportati con nodo e causa.
- Calliope **non** ha un selettore esplicito «qualità»: la scelta è per tipo (image/video) e
  workflow abilitato. La qualità richiesta dall'operatore è l'aggiunta di NL.

## Cosa è stato portato in NL (comportamento, non codice)

| Calliope | NL Video Suite |
|---|---|
| Chat di produzione legata al progetto | `nl.agent.chat` nel pannello **NL Agent**, con `mode: story/series/objects` e `place` per creare subito i placeholder |
| Planner story → script → assets → video | un solo agente che produce storia + scene + shot + prompt; le scene sono salvate con `nl.story.set` e diventano placeholder sulla traccia NL |
| Ruoli Main/Planner/Story/Script/Assets/Video | modalità **Storia** (scene/keyframe), **Serie** (episodi/clip video), **Oggetti** (sketch veloce), con `media: image/video` |
| Workflow per ruolo e abilitazione | `nl.agent.workflows` sceglie dalla libreria NL per `kind` (image/video) e **qualità**; il percorso scelto è salvato su ogni oggetto (`workflow`, `workflowQuality`, `workflowReason`) |
| Scelta del workflow al momento del render | `nl.agent.generate` usa il workflow dell'oggetto; se manca lo assegna dalla qualità del piano |
| Prompt preview prima del job | l'agente mostra prompt e workflow nel piano prima di «Genera e sostituisci»; la modifica inline è il prossimo passo |
| Sott-agenti multipli | non portati: un solo assistente con un planner interno; i ruoli separati sono un'evoluzione futura |
| SQLite/FastAPI/SvelteKit/CLI | non portati: NL usa sessione/progetto FilmCraft, comandi engine e pannelli egui |

## Qualità → workflow (implementazione NL)

Quattro livelli: **bozza**, **standard**, **alta**, **massima**. Per ogni workflow NL si stima la
qualità dal nome (`turbo`/`lightning` = bozza; `z-image`/`krea2`/`wan2.2` = standard/alta;
`flux2`/`ideogram`/`qwen`/`klein` = massima) e si scarta/penalizza ciò che è specialistico
(`edit`, `sheet`, `extend`, `v2v`, `r2v`, `reference`, `character`, `analisi`, `music`, `audio`)
per un uso testo→immagine/video. La distanza dalla qualità richiesta decide il punteggio.

Prova sulla libreria NL:

| Qualità | Workflow scelto |
|---|---|
| bozza | `image_krea2_turbo_t2i.json` |
| standard | `image_z_image.json` |
| alta | `Krea2_t2i_20260818_API.json` |
| massima | `flux2-text2image-Var.json` |

## Prova live

- **Storia**, qualità alta, media immagini: 3 scene, 3 placeholder sulla traccia NL, workflow
  `calliope/Krea2_t2i_20260818_API.json` su ogni oggetto, `story.json` salvato.
- **Serie**, qualità standard, media video: 3 episodi, 3 placeholder video, workflow
  `video_minimax_h3_t2v.json`.
- **Workflow consigliati**: la tabella sopra, da `nl.agent.workflows`.
- Evidenze: `documentazione/prove/p26-calliope-agent/`.

## Limiti dichiarati

- Un solo agente, non la squadra di sub-agenti di Calliope.
- Niente parser dei ruoli dei nodi (`Input:prompt`): NL usa i workflow esistenti e i suoi
  `graphics.generateComfy`/`replace_tokens`.
- Niente preview/edit del prompt prima del job: il piano è modificabile solo tramite rigenerazione
  AI per ora.
- La scelta qualità è euristica sul nome del workflow, non su benchmark reali.
- Generazione sincrona e raccolta asset esplicita restano i limiti già dichiarati di P25.
