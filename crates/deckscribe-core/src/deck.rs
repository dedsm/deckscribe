//! The deck: the result of scribing a recording, and the rules it keeps.

use anyhow::{Result, ensure};
use serde::Serialize;

use crate::description::SlideText;
use crate::speech::Word;
use crate::time::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PartKind {
    /// A slide that was shown.
    Slide,
    /// Camera, demo or video with no slide.
    Live,
    /// The whole recording as one part, when slides were not looked for.
    Whole,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeckPart {
    /// 1-based position in the deck: the `#N` people and agents refer to.
    pub number: usize,
    pub kind: PartKind,
    pub span: Span,
    /// The instant the image was taken from, if the part has one.
    pub image_at: Option<f64>,
    /// Image path relative to the deck folder.
    pub image: Option<String>,
    pub narration: String,
    /// What a vision model read on the slide; slides only.
    pub slide_text: Option<SlideText>,
}

impl DeckPart {
    /// A short name: the slide's title, else its first line of text.
    pub fn title(&self) -> Option<&str> {
        let t = self.slide_text.as_ref()?;
        t.title
            .as_deref()
            .or_else(|| t.text.first().map(String::as_str))
    }
}

/// How the deck was made.
#[derive(Clone, Debug, Default, Serialize)]
pub struct About {
    /// "deckscribe 0.1.0".
    pub tool: String,
    /// Seconds since the Unix epoch.
    pub created_unix: u64,
    /// The speech model and where it ran, if speech was on.
    pub speech_model: Option<String>,
    /// The vision model and server, if descriptions were on.
    pub vision_model: Option<String>,
    /// Overlay regions that were masked, with how much of the recording each covered.
    pub masked: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Deck {
    /// The recording's file name.
    pub recording: String,
    pub duration: f64,
    pub about: About,
    pub parts: Vec<DeckPart>,
    /// Every word of the transcript, in order; empty if speech was off.
    pub words: Vec<Word>,
}

impl Deck {
    pub fn slides(&self) -> usize {
        self.parts
            .iter()
            .filter(|p| p.kind == PartKind::Slide)
            .count()
    }

    pub fn live_parts(&self) -> usize {
        self.parts
            .iter()
            .filter(|p| p.kind == PartKind::Live)
            .count()
    }

    /// Checks the deck's invariants: parts in order and tiling the recording,
    /// images inside their part, slide text only on slides, and every word in
    /// exactly one narration.
    pub fn check(&self) -> Result<()> {
        ensure!(!self.parts.is_empty(), "a deck has at least one part");
        ensure!(
            self.parts[0].span.start == 0.0,
            "the first part starts at 0"
        );
        for (k, p) in self.parts.iter().enumerate() {
            ensure!(p.number == k + 1, "part {} is numbered {}", k + 1, p.number);
            ensure!(
                p.span.end > p.span.start,
                "part {} has an empty span",
                p.number
            );
            if let Some(next) = self.parts.get(k + 1) {
                ensure!(
                    (next.span.start - p.span.end).abs() < 1e-9,
                    "parts {} and {} do not meet",
                    p.number,
                    next.number
                );
            }
            if let Some(t) = p.image_at {
                ensure!(
                    t >= p.span.start && t <= p.span.end,
                    "part {}'s image lies outside it",
                    p.number
                );
            }
            ensure!(
                p.slide_text.is_none() || p.kind == PartKind::Slide,
                "part {} has slide text but is not a slide",
                p.number
            );
        }
        let last = self.parts.last().map_or(0.0, |p| p.span.end);
        ensure!(
            last >= self.duration - 1e-6,
            "the last part ends at {last:.1}s, before the recording ({:.1}s)",
            self.duration
        );
        let narrated: usize = self
            .parts
            .iter()
            .map(|p| p.narration.split_whitespace().count())
            .sum();
        let spoken: usize = self
            .words
            .iter()
            .map(|w| w.text.split_whitespace().count())
            .sum();
        ensure!(
            narrated == spoken,
            "{spoken} words spoken but {narrated} narrated"
        );
        Ok(())
    }
}
