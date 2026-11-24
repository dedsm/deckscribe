---
name: deckscribe
description: Read deckscribe decks, made from recorded talks, lectures and screen-recorded presentations: the slides that were shown, what was said over each, and what is on each slide. Use whenever a message names or attaches a `.deck` folder, its `deck.md`, `deck.json` or `contact-sheet.jpg`, starts with a `[deckscribe …]` header, or asks about the content of a recorded talk; and when you need to turn a talk recording into slides and a transcript with `deckscribe`.
license: MIT
---

# deckscribe decks

A deck is a folder (`talk.deck/`) made from one recording of a talk. It holds
the slides that were on screen, the words spoken while each was shown, and,
when a vision model was used, what is written and drawn on each slide. Use it
to answer questions about the talk, summarise it, or quote it, without
watching the video.

## Read the deck

1. Open `deck.md` first. It starts with a `[deckscribe · recording · N slides ·
   length · folder]` header, says how it was made, then lists every part on one
   line: `#8 (8:59–12:20) High-quality verification — "Something interesting
   about that skill is…"`. After the list, each part has its image, **On the
   slide** (title, text, visuals) and **Said** (the narration).
2. Open `contact-sheet.jpg` to see every slide at once, in order, left to right
   and top to bottom, each marked `#N`. Open `slides/slide-NN.jpg` for one slide
   at full size; `#8` is `slide-08.jpg`.
3. `deck.json` has the same content as data (`schema: "deckscribe.deck/1"`):
   `parts[]` with `number`, `kind` (`slide`, `live`, `whole`), `start`, `end`,
   `title`, `image`, `slide_text {title, text[], visuals}` and `narration`, plus
   `about` (tool, models, masked regions).
4. `words.json` is every word as `[start, end, "word"]` in seconds. Read it only
   when you need exact timing, such as quoting the moment a phrase was said.

Name moments the way `deck.md` does, as `#8 (8:59)`, so a person can find
them in the recording.

## What to trust

- **Said** is machine transcription. It mishears names, products and technical
  terms. When it disagrees with the slide on how a name is spelled, the slide
  is right.
- **On the slide** is a vision model's reading. Text is usually right;
  diagrams, arrows and which box connects to which are often wrong. Look at the
  image before relying on a diagram's structure.
- A part marked `live` is camera, demo or video with no slide; its image is a
  still from the middle of it.
- Slides that change step by step (bullets appearing, a pen mark added) can
  show up as several consecutive parts with nearly the same image.
- Words spoken during a slide change go to the next slide, except that a
  sentence started on one slide stays with it.

## Making a deck

```sh
deckscribe talk.mp4                         # → talk.deck/, prints the path of deck.md
deckscribe talk.mp4 --describe-url http://127.0.0.1:8012 --describe-model gemma --no-think
deckscribe talk.mp4 --no-transcript         # slides only, no speech model
deckscribe talk.mp4 --no-slides             # transcript only
deckscribe skill                            # this guide
```

It needs ffmpeg on PATH. The first run downloads the speech model (about
700 MB). Reading slides needs an OpenAI-compatible server with a vision model;
without `--describe-url` the deck has images and narration but no **On the
slide**. A 40-minute 1080p talk takes a few minutes; reading slides adds about
10–15 s per slide on a local model.

Leave a deck's files as they are; make a new deck to change settings.
