//! Alignment: each word belongs to the part whose span contains its midpoint;
//! a sentence cut by a part boundary stays with the part where it started.

use std::ops::Range;

use crate::speech::Word;
use crate::time::Span;

/// Longest a sentence may run past a part boundary and still be kept whole
/// in the earlier part, in seconds.
pub const SENTENCE_SNAP: f64 = 3.0;

fn ends_sentence(w: &Word) -> bool {
    w.text
        .trim_end_matches(['"', '\'', ')', '”', '’'])
        .ends_with(['.', '?', '!'])
}

/// Word index ranges of each span. Words must be in time order; spans must be
/// ordered and tile the recording. Words outside every span go to the nearest end.
pub fn assign(words: &[Word], spans: &[Span], snap: f64) -> Vec<Range<usize>> {
    if spans.is_empty() {
        return Vec::new();
    }
    // First word index of each span, by midpoint.
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
    // Keep a sentence whole: if the words just after a boundary finish a
    // sentence begun before it, within `snap` seconds, they move back.
    for k in 1..spans.len() {
        let b = starts[k];
        if b == 0 || b >= words.len() || ends_sentence(&words[b - 1]) {
            continue;
        }
        let end_of = |j: usize| starts.get(k + 1).copied().unwrap_or(words.len()) > j;
        let mut j = b;
        while j < words.len() && end_of(j) && words[j].end - spans[k].start <= snap {
            if ends_sentence(&words[j]) {
                starts[k] = j + 1;
                break;
            }
            j += 1;
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

/// The narration of each span as text.
pub fn narrate(words: &[Word], spans: &[Span]) -> Vec<String> {
    assign(words, spans, SENTENCE_SNAP)
        .into_iter()
        .map(|r| {
            words[r]
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}
