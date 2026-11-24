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
pub fn transcribe(recognizer: &mut dyn Recognizer, pcm: &[f32], piece_seconds: usize) -> Result<Vec<Word>> {
    let mut words = Vec::new();
    for (a, b) in pieces(pcm, piece_seconds.max(1) * AUDIO_RATE) {
        let offset = a as f64 / AUDIO_RATE as f64;
        let piece = recognizer.recognize(&pcm[a..b]).with_context(|| format!("transcribing at {offset:.0}s"))?;
        words.extend(piece.into_iter().filter(|w| !w.text.trim().is_empty()).map(|w| Word {
            start: offset + w.start,
            end: offset + w.end,
            text: w.text.trim().to_string(),
        }));
    }
    Ok(words)
}
