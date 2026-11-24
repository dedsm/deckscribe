//! Speech: the transcript of a recording, built piece by piece with a
//! recognizer behind a port.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ports::Recognizer;

/// Audio sample rate the recognizer is fed.
pub const AUDIO_RATE: usize = 16_000;

/// A spoken word and when it was said, in seconds from the recording's start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

impl Word {
    pub fn midpoint(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// Piece boundaries as sample ranges: at most `max` samples each, each ending
/// at the quietest 200 ms within its last 3 s, so a cut rarely lands inside a word.
pub fn pieces(pcm: &[f32], max: usize) -> Vec<(usize, usize)> {
    let (win, search) = (AUDIO_RATE / 5, 3 * AUDIO_RATE);
    let mut out = Vec::new();
    let mut start = 0;
    while start < pcm.len() {
        let hard_end = (start + max).min(pcm.len());
        if hard_end == pcm.len() {
            out.push((start, hard_end));
            break;
        }
        let lo = hard_end.saturating_sub(search).max(start + win);
        let mut best = (f32::MAX, hard_end);
        let mut s = lo;
        while s + win <= hard_end {
            let energy: f32 = pcm[s..s + win].iter().map(|x| x * x).sum();
            if energy < best.0 {
                best = (energy, s + win / 2);
            }
            s += win / 4;
        }
        out.push((start, best.1));
        start = best.1;
    }
    out
}

/// The transcript of `pcm` (16 kHz mono): pieces of at most `piece_seconds`,
/// each recognized and shifted onto the recording's timeline.
pub fn transcribe(
    recognizer: &mut dyn Recognizer,
    pcm: &[f32],
    piece_seconds: usize,
) -> Result<Vec<Word>> {
    let mut words = Vec::new();
    for (a, b) in pieces(pcm, piece_seconds.max(1) * AUDIO_RATE) {
        let offset = a as f64 / AUDIO_RATE as f64;
        let piece = recognizer
            .recognize(&pcm[a..b])
            .with_context(|| format!("transcribing at {offset:.0}s"))?;
        words.extend(
            piece
                .into_iter()
                .filter(|w| !w.text.trim().is_empty())
                .map(|w| Word {
                    start: offset + w.start,
                    end: offset + w.end,
                    text: w.text.trim().to_string(),
                }),
        );
    }
    Ok(words)
}

/// Hesitations that carry nothing when reading: "uh", "um" and the like, with
/// any punctuation the recognizer attached.
pub fn is_filler(text: &str) -> bool {
    let t = text
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_ascii_lowercase();
    matches!(
        t.as_str(),
        "uh" | "um" | "uhm" | "umm" | "uhh" | "erm" | "er" | "ah" | "hmm" | "mm"
    )
}

/// Words as they read best: fillers dropped, and a ", you know," aside removed.
/// The exact words stay in the transcript.
pub fn readable(words: &[&str]) -> String {
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if is_filler(w) {
            i += 1;
            continue;
        }
        // "you know," as an aside: followed by a comma, and at a clause start.
        let clause_start = out.last().is_none_or(|p| p.ends_with([',', '.', '?', '!']));
        if clause_start
            && w.eq_ignore_ascii_case("you")
            && words
                .get(i + 1)
                .is_some_and(|n| n.eq_ignore_ascii_case("know,"))
        {
            i += 2;
            continue;
        }
        out.push(w);
        i += 1;
    }
    out.join(" ")
}
