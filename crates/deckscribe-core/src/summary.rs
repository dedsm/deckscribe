//! Summary: an optional pass with a text model over the finished deck, for a
//! reader deciding whether to read on: the talk's title, speaker and main
//! claims.

use std::fmt::Write as _;

use serde::Serialize;

use crate::deck::{Deck, PartKind};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Summary {
    pub title: Option<String>,
    /// The title was proposed by the model; no slide or the speaker states one.
    pub title_proposed: bool,
    pub speaker: Option<String>,
    /// What is specific to this talk: techniques, rules, examples, named
    /// things, each with the part it comes from.
    pub key_ideas: Vec<String>,
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
         TITLE: the talk's title as a slide or the speaker states it. If none is stated, propose a \
         short, specific title and add (proposed) after it.\n\
         SPEAKER: the speaker's full name, as the slides or the introduction give it (or Unknown).\n\
         KEY IDEAS:\n\
         - five lines. Each names one concrete thing from this talk: a technique, a rule, a \
         numbered process, a tool, an example or a role, with what it is for, ending with the part \
         it comes from, like (#9). Nothing that would fit any talk on the same subject.\n",
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
    let mut in_list = false;
    for line in reply.lines() {
        let l = line.trim().replace("**", "");
        let l = l.trim();
        if let Some(v) = l.strip_prefix("TITLE:") {
            let v = v.trim();
            let proposed = v.to_ascii_lowercase().ends_with("(proposed)");
            let v = if proposed {
                v[..v.len() - "(proposed)".len()].trim()
            } else {
                v
            };
            out.title = value(v);
            out.title_proposed = proposed && out.title.is_some();
            in_list = false;
        } else if let Some(v) = l.strip_prefix("SPEAKER:") {
            out.speaker = value(v);
            in_list = false;
        } else if l.starts_with("KEY IDEAS:") || l.starts_with("THESIS:") {
            in_list = true;
        } else if in_list && !l.is_empty() {
            let item = l
                .trim_start_matches(['-', '*', '•'])
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')');
            out.key_ideas.extend(value(item));
        }
    }
    out.key_ideas.truncate(7);
    out
}
