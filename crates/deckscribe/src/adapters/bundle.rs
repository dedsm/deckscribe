//! The deck folder, written for agents first: `deck.md` is the one file to
//! read, and says how to read the rest.
//!
//! ```text
//! talk.deck/
//!   deck.md            header, how to read the deck, one line per part, then every part
//!   deck.html          the same for people: each image beside what was said (see `page`)
//!   deck.json          the same as data (schema "deckscribe.deck/1")
//!   words.json         every word as [start, end, "text"], seconds
//!   contact-sheet.jpg  every part's image in order, each marked #N
//!   slides/slide-NN.jpg
//! ```

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::Result;
use deckscribe_core::ports::DeckWriter;
use deckscribe_core::time::clock;
use deckscribe_core::{Deck, DeckPart, PartKind};
use serde_json::{Value, json};

pub const SCHEMA: &str = "deckscribe.deck/1";
pub const ENTRY: &str = "deck.md";
pub const CONTACT_SHEET: &str = "contact-sheet.jpg";
pub const WORDS: &str = "words.json";

pub struct Bundle;

impl DeckWriter for Bundle {
    fn write(&self, deck: &Deck, out: &Path) -> Result<()> {
        let folder = out.canonicalize().unwrap_or_else(|_| out.to_path_buf());
        fs::write(
            out.join(ENTRY),
            markdown(
                deck,
                &folder.display().to_string(),
                out.join(CONTACT_SHEET).exists(),
            ),
        )?;
        fs::write(
            out.join("deck.json"),
            serde_json::to_string_pretty(&manifest(deck))?,
        )?;
        let words: Vec<Value> = deck
            .words
            .iter()
            .map(|w| json!([round(w.start), round(w.end), w.text]))
            .collect();
        fs::write(out.join(WORDS), serde_json::to_string(&words)?)?;
        Ok(())
    }
}

fn parts_list(parts: &[usize]) -> String {
    parts
        .iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn round(t: f64) -> f64 {
    (t * 100.0).round() / 100.0
}

fn moment(p: &DeckPart) -> String {
    format!(
        "#{} ({}–{})",
        p.number,
        clock(p.span.start),
        clock(p.span.end)
    )
}

fn kind_label(p: &DeckPart) -> &'static str {
    match p.kind {
        PartKind::Slide => "slide",
        PartKind::Live => "speaker or video, no slide",
        PartKind::Whole => "whole recording",
    }
}

/// The first words of a narration, for the list of parts.
fn opening(narration: &str, words: usize) -> String {
    let w: Vec<&str> = narration.split_whitespace().collect();
    if w.is_empty() {
        return String::new();
    }
    let mut s = w[..w.len().min(words)].join(" ");
    if w.len() > words {
        s.push('…');
    }
    format!("\"{s}\"")
}

/// UTC "YYYY-MM-DD HH:MM" from Unix seconds (days-to-civil, proleptic Gregorian).
fn utc(unix: u64) -> String {
    let (days, secs) = ((unix / 86_400) as i64, unix % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        secs / 3600,
        secs / 60 % 60
    )
}

/// A part's title, or for a return, the title of the part it returns to.
fn title_of(deck: &Deck, p: &DeckPart) -> Option<String> {
    let src = p
        .same_as
        .and_then(|n| deck.parts.iter().find(|q| q.number == n))
        .unwrap_or(p);
    src.title().map(str::to_string)
}

fn markdown(deck: &Deck, folder: &str, has_sheet: bool) -> String {
    let a = &deck.about;
    let described = deck.parts.iter().any(|p| p.slide_text.is_some());
    let has_speech = !deck.words.is_empty() || a.speech_model.is_some();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "[deckscribe · {} · {} slides · {} · {folder}]\n",
        deck.recording,
        deck.slides(),
        clock(deck.duration)
    );
    let heading = deck
        .summary
        .as_ref()
        .and_then(|sm| sm.title.clone())
        .unwrap_or_else(|| deck.recording.clone());
    let _ = writeln!(s, "# {heading}\n");
    let mut counts = vec![format!("{} slides", deck.slides())];
    if deck.live_parts() > 0 {
        counts.push(format!("{} speaker or video parts", deck.live_parts()));
    }
    let _ = write!(
        s,
        "{} turned into the slides that were shown and what was said over each: {} over {}. Made by {} on {}",
        deck.recording,
        counts.join(" and "),
        clock(deck.duration),
        a.tool,
        utc(a.created_unix)
    );
    match (&a.speech_model, &a.vision_model) {
        (Some(sp), Some(v)) => {
            let _ = write!(s, ", with {sp} for speech and {v} for slide text");
        }
        (Some(sp), None) => {
            let _ = write!(s, ", with {sp} for speech");
        }
        (None, Some(v)) => {
            let _ = write!(s, ", with {v} for slide text");
        }
        (None, None) => {}
    }
    s.push_str(".\n\n");

    if let Some(sm) = &deck.summary {
        s.push_str("## Summary\n\n");
        if let Some(t) = &sm.title {
            let note = if sm.title_proposed {
                " (proposed; no slide states a title)"
            } else {
                ""
            };
            let _ = writeln!(s, "**Talk:** {t}{note}  ");
        }
        if let Some(sp) = &sm.speaker {
            let _ = writeln!(s, "**Speaker:** {sp}  ");
        }
        if !sm.key_ideas.is_empty() {
            s.push_str("\n**Key ideas:**\n\n");
            for t in &sm.key_ideas {
                let _ = writeln!(s, "- {t}");
            }
        }
        s.push_str("\nWritten by a text model from the slides and the transcript; check claims against the parts below.\n\n");
    }

    if !deck.corrected.is_empty() || !deck.possible.is_empty() {
        s.push_str("## Names and terms\n\n");
        if !deck.corrected.is_empty() {
            s.push_str(
                "Corrected in **Said**: names the slides spell out that the transcription got wrong.\n\n\
                 | Heard | On the slide | Parts |\n|---|---|---|\n",
            );
            for c in &deck.corrected {
                let _ = writeln!(
                    s,
                    "| {} | {} | {} |",
                    c.heard,
                    c.correct,
                    parts_list(&c.parts)
                );
            }
            s.push('\n');
        }
        if !deck.possible.is_empty() {
            s.push_str(
                "Possible mishearings, left as heard: a word in **Said** close in spelling or sound to a \
                 term on the slide being shown. Prefer the slide's spelling when they mean the same thing.\n\n\
                 | Heard | On the slide | Parts |\n|---|---|---|\n",
            );
            for c in &deck.possible {
                let _ = writeln!(
                    s,
                    "| {} | {} | {} |",
                    c.heard,
                    c.correct,
                    parts_list(&c.parts)
                );
            }
            s.push('\n');
        }
    }

    s.push_str("## How to read this deck\n\n");
    s.push_str(
        "- This deck is made by small local models and is best effort; read it with judgement. \
         Speech recognition errors remain in **Said** where the right word is not on a slide \
         (an ordinary word heard as another, such as \"slot\" for \"slop\" or \"tech dep\" for \
         \"tech debt\"): correct them from context.\n",
    );
    s.push_str(
        "- Parts are numbered. Refer to them the way the list below does, as `#N (m:ss)`, \
         so a person can find the moment in the recording.\n",
    );
    s.push_str("- Each part below gives its image");
    if described {
        s.push_str(", what a vision model read on the slide (**On the slide**),");
    }
    if has_speech {
        s.push_str(" and the words spoken while it was shown (**Said**)");
    }
    s.push_str(".\n");
    if has_sheet {
        let _ = writeln!(
            s,
            "- `{CONTACT_SHEET}` shows every part's image in order, left to right and top to bottom, \
             each marked `#N`. Look at it, or at the images in `slides/`, when layout or diagrams matter."
        );
    }
    if described {
        s.push_str(
            "- **On the slide** is a model's reading: it can misread diagrams and how their parts \
             connect. Check the image before relying on it.\n",
        );
    }
    if has_speech {
        s.push_str(
            "- **Said** is machine transcription with fillers (uh, um) left out. It mishears names \
             and technical terms; when it disagrees with the slide on a name, trust the slide.\n",
        );
        s.push_str(
            "- Words go to the slide on screen when they were said. Where a slide change falls inside \
             a sentence, the cut moves to the nearest end of a sentence within 8 s; a longer \
             sentence can still be split.\n",
        );
    }
    if deck.parts.iter().any(|p| p.same_as.is_some()) {
        s.push_str(
            "- A part marked *back to #N* returns to a slide already shown; see #N for its text.\n",
        );
    }
    if !a.masked.is_empty() {
        let _ = writeln!(
            s,
            "- Regions that kept changing (webcam, captions, speaker panel) were ignored when finding \
             slides{}: {}.",
            if described {
                ", and blacked out where they were changing before each slide was read"
            } else {
                ""
            },
            a.masked.join("; ")
        );
    }
    let _ = writeln!(
        s,
        "- `deck.json` holds the same content as data. `{WORDS}` holds every word as \
         `[start, end, \"word\"]` in seconds; open it only when exact timing matters.\n"
    );

    s.push_str("## Parts\n\n");
    for p in &deck.parts {
        let mut line = moment(p);
        if p.kind != PartKind::Slide {
            let _ = write!(line, " [{}]", kind_label(p));
        }
        if let Some(t) = title_of(deck, p) {
            let _ = write!(line, " {t}");
        }
        if let Some(n) = p.same_as {
            let _ = write!(line, " *back to #{n}*");
        }
        let said = opening(&p.narration, 12);
        if !said.is_empty() {
            let _ = write!(line, " — {said}");
        }
        let _ = writeln!(s, "{line}");
    }

    for p in &deck.parts {
        let _ = write!(s, "\n---\n\n## {}", moment(p));
        match (p.kind, title_of(deck, p)) {
            (PartKind::Slide, Some(t)) => {
                let _ = write!(s, " · {t}");
            }
            (PartKind::Slide, None) => {}
            _ => {
                let _ = write!(s, " · {}", kind_label(p));
            }
        }
        if let Some(n) = p.same_as {
            let _ = write!(s, " · *back to #{n}*");
        }
        s.push_str("\n\n");
        if let Some(img) = &p.image {
            let _ = writeln!(s, "![#{}]({img})\n", p.number);
        }
        if let Some(n) = p.same_as {
            let _ = writeln!(s, "### On the slide\n\nSame slide as #{n}.\n");
        } else if let Some(t) = &p.slide_text {
            s.push_str("### On the slide\n\n");
            if let Some(title) = &t.title {
                let _ = writeln!(s, "**Title:** {title}\n");
            }
            if !t.text.is_empty() {
                s.push_str("**Text:**\n\n");
                for line in &t.text {
                    let _ = writeln!(s, "- {line}");
                }
                s.push('\n');
            }
            if let Some(v) = &t.visuals {
                let _ = writeln!(s, "**Visuals:** {v}\n");
            }
            if let Some(raw) = &t.unparsed {
                let _ = writeln!(s, "{raw}\n");
            }
        }
        if has_speech {
            let said = if p.narration.is_empty() {
                "(nothing)"
            } else {
                p.narration.as_str()
            };
            let _ = writeln!(s, "### Said\n\n{said}");
        }
    }
    s
}

fn manifest(deck: &Deck) -> Value {
    let parts: Vec<Value> = deck
        .parts
        .iter()
        .map(|p| {
            json!({
                "number": p.number,
                "kind": p.kind,
                "start": round(p.span.start),
                "end": round(p.span.end),
                "title": p.title(),
                "image": p.image,
                "image_at": p.image_at.map(round),
                "slide_text": p.slide_text,
                "same_as": p.same_as,
                "narration": p.narration,
                "words": [p.words.start, p.words.end],
            })
        })
        .collect();
    json!({
        "schema": SCHEMA,
        "recording": deck.recording,
        "duration": round(deck.duration),
        "about": deck.about,
        "summary": deck.summary,
        "corrected": deck.corrected,
        "possible_mishearings": deck.possible,
        "files": {"entry": ENTRY, "page": crate::adapters::page::PAGE, "contact_sheet": CONTACT_SHEET, "words": WORDS, "slides": "slides/"},
        "parts": parts,
    })
}

#[cfg(test)]
mod tests {
    use super::utc;

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc(1_795_528_800), "2026-11-24 14:00 UTC");
    }
}
