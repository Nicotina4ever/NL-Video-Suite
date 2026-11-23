NL Video Suite 0.1.0 - portable Windows bundle

Start it with: Avvia NL Video Suite.cmd

What is inside:
  NL Video Suite.exe      the editor (no installer, no registry writes)
  workflows\              the ComfyUI workflow library
  tools\                  ffmpeg/ffprobe copied from the suite when available
  DISTRIBUZIONE.md        how to build on Linux and macOS, and how to point the suite at AI services

What is NOT inside, by design:
  ComfyUI, Ollama or any language model, and any generated media. Install them separately and
  configure their addresses in Settings > NL Video Suite (ComfyUI address, language-model
  provider, ASR folder). The suite never downloads a model by itself and never starts a service.

Settings are per user (%APPDATA%\NL Video Suite); projects are normal files you choose.
