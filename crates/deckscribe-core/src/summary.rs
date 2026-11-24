//! Summary: an optional pass with a text model over the finished deck, for a
//! reader deciding whether to read on: the talk's title, speaker and main
//! claims.

use std::fmt::Write as _;

use serde::Serialize;

use crate::deck::{Deck, PartKind};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Summary {
    pub title: Option<String>,
    pub speaker: Option<String>,
    /// The talk's main claims, one sentence each.
    pub thesis: Vec<String>,
}

/// The request: every part's slide text and narration, then the format.
pub fn prompt(deck: &Deck) -> String {
    let mut s = String::from(
        "Below is a recorded talk, part by part: the text read from each slide, and what the speaker \
         said (machine transcription).\n\n",
    );
    for p in &deck.parts {
        let _ = writeln!(s, "## Part {}", p.number);
        if let Some(t) = &p.slide_text {
            if let Some(title) = &t.title {
                let _ = writeln!(s, "Slide title: {title}");
            }
            if !t.text.is_empty() {
                let _ = writeln!(s, "Slide text: {}", t.text.join(" | "));
            }
        } else if p.kind == PartKind::Live {
            s.push_str("(camera or video, no slide)\n");
        }
        let _ = writeln!(s, "Said: {}\n", p.narration);
    }
    s.push_str(
        "Answer in exactly this format and nothing else:\n\
         TITLE: the talk's title, if a slide or the speaker states one (or Unknown).\n\
         SPEAKER: the speaker's full name, as the slides or the introduction give it (or Unknown).\n\
         THESIS:\n\
         - the talk's main claims: three lines, one sentence each.\n",
    );
    s
}

fn value(s: &str) -> Option<String> {
    let v = s.trim().trim_matches('*').trim();
    (!v.is_empty() && !v.eq_ignore_ascii_case("unknown") && !v.eq_ignore_ascii_case("none"))
        .then(|| v.to_string())
}

pub fn parse(reply: &str) -> Summary {
    let mut out = Summary::default();
    let mut in_thesis = false;
    for line in reply.lines() {
        let l = line.trim().trim_start_matches("**").replace("**", "");
        let l = l.trim();
        if let Some(v) = l.strip_prefix("TITLE:") {
            out.title = value(v);
            in_thesis = false;
        } else if let Some(v) = l.strip_prefix("SPEAKER:") {
            out.speaker = value(v);
            in_thesis = false;
        } else if l.starts_with("THESIS:") {
            in_thesis = true;
        } else if in_thesis && !l.is_empty() {
            let item = l
                .trim_start_matches(['-', '*', '•'])
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
            out.thesis.extend(value(item));
        }
    }
    out.thesis.truncate(5);
    out
}
