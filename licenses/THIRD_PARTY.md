# Componenti di terze parti

Il core della suite e i componenti vendorizzati non costituiscono un unico componente con una sola licenza. Conservare gli avvisi originali presenti in runtime, engines e modelli.

- FFmpeg/ffprobe: build statica Gyan GPL v3, con LICENSE e README originali in `ffmpeg/`. Il README identifica versione, configurazione e commit del sorgente corrispondente. Si usano come programmi separati, non come librerie collegate al core. [Condizioni ufficiali](https://ffmpeg.org/legal.html).
- Python: licenza originale in `runtime/python/LICENSE.txt`. Le dipendenze hanno le proprie licenze nelle cartelle dei pacchetti e `.dist-info`.
- Whisper large-v3 CTranslate2: copia locale della conversione, modello dichiarato MIT. [Systran](https://huggingface.co/Systran/faster-whisper-large-v3). Testo MIT di Whisper in `models/WHISPER-MIT.txt`.
- Allineatore italiano: Jonatas Grosman, `wav2vec2-large-xlsr-53-italian`, revisione fissata nel manifest dei modelli. [Model card dichiarata Apache 2.0](https://huggingface.co/jonatasgrosman/wav2vec2-large-xlsr-53-italian). Testo Apache incluso. Il modello VoxPopuli precedentemente implicito non viene incluso nel pacchetto portable.
- Ollama, ComfyUI, custom node e loro pesi restano installazioni esterne con condizioni proprie; non si assume una licenza comune o automaticamente compatibile per tutti i workflow.

Questa è una distinta informativa, non una certificazione legale o di idoneità commerciale. Prima di pubblicare una distribuzione per terzi occorre completare la verifica delle licenze incluse, delle attribuzioni e degli obblighi di disponibilità dei sorgenti (in particolare dei binari FFmpeg e relativi componenti). L'archivio è preparato per trasferire la propria installazione tra PC.

## Pacchetto Core sperimentale

Il Core non ridistribuisce FFmpeg, Python/Torch, checkpoint o font. Le dipendenze Rust del grafo Cargo.lock Windows x64 hanno inventario e testi originali in RUST_DEPENDENCIES.json/txt (151pacchetti, nessun avviso mancante al momento della raccolta). I copyright della libreria standard e i testi associati sono in rust-standard-library/. Le condizioni dei componenti compilati restano distinte dalla MIT del core. Questa raccolta non certifica il futuro portable completo con tutti i runtime/modelli.
