//! Alignment: each word belongs to the part whose span contains its midpoint;
//! a sentence cut by a part boundary stays with the part where it started.

use std::ops::Range;

use crate::speech::Word;
use crate::time::Span;

/// How far, in seconds, a cut between parts may move to land on the end of a
/// sentence instead of inside one.
pub const SENTENCE_SNAP: f64 = 8.0;

fn ends_sentence(w: &Word) -> bool {
    w.text
        .trim_end_matches(['"', '\'', ')', '”', '’'])
        .ends_with(['.', '?', '!'])
}

/// Word index ranges of each span. Words must be in time order; spans must be
/// ordered and tile the recording. Each word goes to the span containing its
/// midpoint, then each cut between spans moves to the nearest sentence end
/// within `snap` seconds, before or after it, so no narration starts or ends
/// mid-sentence. A cut with no sentence end nearby stays where the slide changed.
pub fn assign(words: &[Word], spans: &[Span], snap: f64) -> Vec<Range<usize>> {
    if spans.is_empty() {
        return Vec::new();
    }
    let mut starts = Vec::with_capacity(spans.len());
    let mut i = 0;
    for (k, s) in spans.iter().enumerate() {
        if k == 0 {
            starts.push(0);
            continue;
        }
        while i < words.len() && words[i].midpoint() < s.start {
            i += 1;
        }
        starts.push(i);
    }
    for k in 1..spans.len() {
        let b = starts[k];
        if b == 0 || b >= words.len() || ends_sentence(&words[b - 1]) {
            continue;
        }
        let at = spans[k].start;
        // Never past the previous cut or the next one.
        let (lo, hi) = (
            starts[k - 1],
            starts.get(k + 1).copied().unwrap_or(words.len()),
        );
        let before = (lo..b)
            .rev()
            .find(|&j| ends_sentence(&words[j]))
            .filter(|&j| at - words[j].end <= snap);
        let after = (b..hi)
            .find(|&j| ends_sentence(&words[j]))
            .filter(|&j| words[j].end - at <= snap);
        let best = match (before, after) {
            (Some(x), Some(y)) => Some(if at - words[x].end <= words[y].end - at {
                x
            } else {
                y
            }),
            (x, y) => x.or(y),
        };
        if let Some(e) = best {
            starts[k] = e + 1;
        }
    }
    (0..spans.len())
        .map(|k| {
            starts[k]
                ..starts
                    .get(k + 1)
                    .copied()
                    .unwrap_or(words.len())
                    .max(starts[k])
        })
        .collect()
}

/// The narration of each span as text, without fillers (see
/// `speech::readable`).
pub fn narrate(words: &[Word], spans: &[Span]) -> Vec<String> {
    assign(words, spans, SENTENCE_SNAP)
        .into_iter()
        .map(|r| {
            crate::speech::readable(&words[r].iter().map(|w| w.text.as_str()).collect::<Vec<_>>())
        })
        .collect()
}
