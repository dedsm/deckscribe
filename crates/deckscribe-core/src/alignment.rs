//! Alignment: each word belongs to the part whose span contains its midpoint.

use crate::speech::Word;
use crate::time::Span;

/// The narration of each span: the words whose midpoint falls inside it.
/// Spans must be ordered and tile the recording; words outside every span go
/// to the nearest end.
pub fn narrate(words: &[Word], spans: &[Span]) -> Vec<String> {
    let mut out: Vec<Vec<&str>> = vec![Vec::new(); spans.len()];
    if spans.is_empty() {
        return Vec::new();
    }
    for w in words {
        let mid = w.midpoint();
        let k = spans
            .iter()
            .position(|s| s.contains(mid))
            .unwrap_or(if mid < spans[0].start { 0 } else { spans.len() - 1 });
        out[k].push(w.text.as_str());
    }
    out.into_iter().map(|v| v.join(" ")).collect()
}
