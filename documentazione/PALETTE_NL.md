# Palette NL Video Suite

Derived from the owner's mark (`LogoMioBozza.svg`): a neon studio microphone in violet and
magenta with a cyan voice waveform. The app keeps FilmCraft's professional dark layout and
replaces its neutral greys and blue accent with this palette.

## Core

| Token | Hex | Use |
|---|---|---|
| Violet | `#7531FE` | primary accent, focus, selected controls, primary buttons |
| Violet light | `#A46BFF` | wordmark "NL", hover glow, icon highlights |
| Magenta | `#C42B86` | secondary accent, hot numeric values, active word |
| Magenta bright | `#E13A8E` | icon gradient end, warnings that must pop |
| Cyan | `#8FF0FF` | playhead, timecode, waveform, karaoke highlight |
| Near-black plum | `#08050D` | app background, monitor background |
| Panel plum | `#171021` | panel body |
| Header plum | `#140C1F` | header, tab strip, timeline background |
| Text | `#EDE6F5` | primary text |
| Text dim | `#B4A6C6` | secondary text and icons |
| Text faint | `#7A6A8C` | disabled and tertiary text |
| Border | `#362543` | fields and separators |

## Rules

- Violet is the only colour that means "selected / primary action". Magenta is an emphasis, not a
  second selection colour.
- Cyan is reserved for time: playhead, timecode, waveform and the spoken-word highlight. It is not
  a button colour.
- Backgrounds stay plum-black; avoid pure black and pure white fields.
- Caption text uses near-white `#FFFFFF` with an outline of `#101018` and a magenta/cyan highlight,
  so it remains readable on any footage.
- Contrast: body text at least 4.5:1 on its surface; controls at least 3:1. The values above are
  chosen for the dark theme; the light theme uses darker text (`#241832`) on lavender surfaces
  (`#F7F3FB`).

## Where it lives

- `filmcraft-nl/crates/ui-egui/src/theme.rs` — theme tokens (dark, medium, light).
- `filmcraft-nl/assets/app-icon/nlvideo.svg` — app icon source.
- `filmcraft-nl/docs/brand/nlvideo-wordmark-*.svg` — wordmark source.
- `sviluppo/src/subtitle_studio.rs` — default subtitle colours (`#FFFFFF`, `#E5469C`).

When adding a panel or a widget, read colours from `Tokens`; do not hardcode a hex value in a
widget.
