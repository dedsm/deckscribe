//! Speech to words with timestamps: transcribe.cpp (the engine Hex embeds)
//! running a Parakeet model on the GPU, in pieces of at most `chunk` seconds.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use transcribe_cpp::{Model, RunOptions, TimestampKind};

use crate::media::AUDIO_RATE;

#[derive(Clone, Debug, Serialize)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Piece boundaries: at most `max` samples each, ending at the quietest
/// 200 ms inside the last 3 s, so a cut rarely lands inside a word.
fn pieces(pcm: &[f32], max: usize) -> Vec<(usize, usize)> {
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

pub fn words(model_path: &Path, pcm: &[f32], chunk_seconds: usize) -> Result<Vec<Word>> {
    transcribe_cpp::init_backends_default().context("starting the speech engine")?;
    let model = Model::load(model_path)
        .with_context(|| format!("loading the speech model {}", model_path.display()))?;
    eprintln!("speech: {} on {}", model.arch(), model.backend());
    let mut session = model.session()?;
    let opts = RunOptions { timestamps: TimestampKind::Word, ..Default::default() };
    let mut words = Vec::new();
    for (a, b) in pieces(pcm, chunk_seconds * AUDIO_RATE) {
        let offset = a as f64 / AUDIO_RATE as f64;
        let result = session
            .run(&pcm[a..b], &opts)
            .with_context(|| format!("transcribing at {offset:.0}s"))?;
        words.extend(result.words.into_iter().filter_map(|w| {
            let text = w.text.trim();
            (!text.is_empty()).then(|| Word {
                start: offset + w.t0_ms as f64 / 1000.0,
                end: offset + w.t1_ms as f64 / 1000.0,
                text: text.to_string(),
            })
        }));
    }
    Ok(words)
}

/// The words spoken during each slide: a word belongs to the slide whose
/// range contains its midpoint.
pub fn per_slide(words: &[Word], ranges: &[(f64, f64)]) -> Vec<String> {
    let mut out = vec![Vec::new(); ranges.len()];
    for w in words {
        let mid = (w.start + w.end) / 2.0;
        let k = ranges
            .iter()
            .position(|&(s, e)| mid >= s && mid < e)
            .unwrap_or(ranges.len().saturating_sub(1));
        if let Some(v) = out.get_mut(k) {
            v.push(w.text.as_str());
        }
    }
    out.into_iter().map(|v| v.join(" ")).collect()
}
