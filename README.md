# deckscribe

Turns a recorded talk into a **deck**: the slides that were on screen, what was
said over each, and what is written and drawn on each slide. The output is a
folder written for agents to read, with one file to start from. Everything runs
locally.

```sh
deckscribe talk.mp4
deckscribe talk.mp4 --describe-url http://127.0.0.1:8012 --describe-model gemma --no-think
```

```
talk.deck/
  deck.md            read this first: how to read the deck, one line per slide, then every slide
  contact-sheet.jpg  every slide in one image, each marked #N
  slides/slide-NN.jpg
  deck.json          the same content as data (schema deckscribe.deck/1)
  words.json         every word as [start, end, "word"], in seconds
```

Agents: `deckscribe skill` prints the guide in
[`skills/deckscribe/SKILL.md`](skills/deckscribe/SKILL.md).

## How it works

- **Slides.** ffmpeg decodes small gray frames. The video is split where most of
  the picture changes at once; in each stretch, regions that keep changing
  (webcam, captions, speaker panel) are masked; a slide is a stretch of at
  least 6 s where the rest stays still. Camera and video stretches become
  separate parts.
- **Words.** Parakeet TDT 0.6B v2 transcribes on the GPU through
  [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp), with a
  start and end for every word. Each word goes to the slide on screen when it
  was said; a sentence begun on one slide stays with it.
- **Slide text** (optional). Each slide image, with overlays blacked out, goes
  to any OpenAI-compatible server with a vision model, and the answer is kept
  as title, text lines and visuals.

On 10 lectures with hand-annotated slide changes it scores F1 0.85 (within
2 s); half of all words start within 0.07 s of human-labelled times. See
[`bench/`](bench/) and [`docs/design.md`](docs/design.md).

## Options

| Flag | Effect |
|---|---|
| `--no-transcript` | slides only; no speech model, no GPU |
| `--no-slides` | transcript only |
| `--describe-url URL`, `--describe-model NAME`, `--no-think` | read each slide with a vision model |
| `--no-mask`, `--mask x,y,w,h` | turn off overlay finding, or add a region to ignore |
| `--no-split`, `--no-live-parts` | one mask for the whole video; fold camera stretches into slides |
| `--min-slide`, `--change`, `--same` | tune slide detection |
| `--model FILE` | another speech model |

## Requirements

- ffmpeg on `PATH` (the Nix package brings its own).
- A Vulkan driver (Linux, Windows) or Metal (macOS) for fast transcription.
- For slide text: a vision model behind an OpenAI-compatible API, such as
  llama.cpp's `llama-server` with a model and its `mmproj` file.

The speech model is not included. On first run, Parakeet TDT 0.6B v2 by NVIDIA
(CC BY 4.0, about 700 MB) is downloaded from a pinned revision into the user
cache folder and checked against its SHA-256.

## Build

```sh
nix build                 # the package, ffmpeg wrapped in
nix develop               # Rust, CMake, Vulkan headers, glslc, ffmpeg, cargo-deny
cargo build --release
cargo test --workspace
cargo deny check licenses bans sources
```

Cargo features: `transcribe` (default; builds transcribe.cpp with CMake) and
`describe` (default). `--no-default-features` gives a pure-Rust, slides-only
build. Licensing and the bill of materials: [`docs/licenses.md`](docs/licenses.md).

## License

MIT.
