# deckscribe

Turns a presentation video into one image per slide, the words spoken over
each slide, and optionally a description of each slide. Everything runs
locally.

```sh
deckscribe talk.mp4                                   # slides + transcript
deckscribe talk.mp4 --describe-url http://127.0.0.1:8012   # + descriptions
```

Output, in `talk-slides/` by default:

- `index.md`: the slides with their times
- `slide_NNN.md`: one slide: times, image, description, transcript
- `slides.md`: all slides in one file
- `slides/slide_NNN.jpg`: the slide images, full resolution
- `slides.json`, `words.json`: the same data for scripts, with every word's start and end

## How it works

1. ffmpeg decodes small grayscale frames (480x300, 5 per second) and 16 kHz
   audio; nothing intermediate is written to disk.
2. Regions that keep changing whatever the slide does (a webcam picture,
   burned-in captions) are found by how often each 10x10 block changes
   between frames 5 s apart, and masked.
3. A slide is a stretch of at least 6 s where the masked picture stays still.
   Neighbouring stretches showing the same picture are merged.
4. The last still frame of each slide is saved at full resolution.
5. Parakeet TDT 0.6B v2 transcribes the audio on the GPU through
   [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp), with a
   start and end time for every word.
6. Optionally, each slide image (with overlays blacked out) is sent to an
   OpenAI-compatible server that accepts images, which describes it.
7. Each word is filed under the slide on screen at its midpoint.

## Requirements

- ffmpeg on `PATH` (the Nix package brings its own)
- a Vulkan GPU driver for fast transcription; otherwise it runs on the CPU
- for descriptions: a vision model behind an OpenAI-compatible API, such as
  llama.cpp's `llama-server` with a model and its `mmproj` file

The speech model is not included. On first run, Parakeet TDT 0.6B v2 by NVIDIA
(CC BY 4.0, about 700 MB) is downloaded from a pinned revision to
`~/.cache/deckscribe/models` and checked against its SHA-256. `--model PATH`
uses another file.

## Build

```sh
nix build            # package, with ffmpeg wrapped in
nix develop -c cargo build --release
```

`transcribe-cpp` and `transcribe-cpp-sys` must be pinned to the same version;
the bindings refuse to load a native library from another release.
