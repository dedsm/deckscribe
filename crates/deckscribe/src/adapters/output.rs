//! Deck writers: markdown for people, JSON for scripts.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::Result;
use deckscribe_core::ports::DeckWriter;
use deckscribe_core::time::clock;
use deckscribe_core::{Deck, DeckPart, PartKind};
use serde_json::json;

fn label(p: &DeckPart) -> String {
    match p.kind {
        PartKind::Slide => format!("Slide {}", p.number),
        PartKind::Live => format!("Part {}: speaker or video, no slide", p.number),
        PartKind::Whole => "Transcript".to_string(),
    }
}

fn kind(p: &DeckPart) -> &'static str {
    match p.kind {
        PartKind::Slide => "slide",
        PartKind::Live => "live",
        PartKind::Whole => "whole",
    }
}

/// index.md, one slide_NNN.md per part, and slides.md with everything.
pub struct Markdown;

impl DeckWriter for Markdown {
    fn write(&self, deck: &Deck, out: &Path) -> Result<()> {
        let live = deck.parts.iter().filter(|p| p.kind == PartKind::Live).count();
        let mut index = format!("# {}\n\n{} slides, {live} speaker/video parts\n\n", deck.recording, deck.slides());
        let mut all = format!("# {}\n", deck.recording);
        for p in &deck.parts {
            let n = p.number;
            let (label, times) = (label(p), format!("{}–{}", clock(p.span.start), clock(p.span.end)));
            let mut body = format!("# {label} · {times}\n");
            if let Some(img) = &p.image {
                write!(body, "\n![{label}]({img})\n")?;
            }
            if let Some(d) = &p.description {
                write!(body, "\n## Description\n\n{d}\n")?;
            }
            write!(body, "\n## Transcript\n\n{}\n", p.narration)?;
            let page = format!(
                "---\nslide: {n}\nkind: {}\ntime_start: {:.1}\ntime_end: {:.1}\n---\n\n{body}",
                kind(p),
                p.span.start,
                p.span.end
            );
            fs::write(out.join(format!("slide_{n:03}.md")), page)?;
            writeln!(index, "- [{label}](slide_{n:03}.md) · {times}")?;
            write!(all, "\n---\n\n{}", body.replacen("# ", "## ", 1))?;
        }
        fs::write(out.join("index.md"), index)?;
        fs::write(out.join("slides.md"), all)?;
        Ok(())
    }
}

/// slides.json (one record per part) and words.json (every word).
pub struct Json;

impl DeckWriter for Json {
    fn write(&self, deck: &Deck, out: &Path) -> Result<()> {
        let parts: Vec<_> = deck
            .parts
            .iter()
            .map(|p| {
                json!({"slide": p.number, "kind": kind(p), "start": p.span.start, "end": p.span.end,
                       "frame": p.image_at, "image": p.image, "transcript": p.narration,
                       "description": p.description})
            })
            .collect();
        fs::write(out.join("slides.json"), serde_json::to_string_pretty(&parts)?)?;
        fs::write(out.join("words.json"), serde_json::to_string_pretty(&deck.words)?)?;
        Ok(())
    }
}
