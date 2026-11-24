//! Description: what is asked of a vision model about a slide image, how its
//! answer is read, and which slides are the same slide. The model itself is
//! behind the `Describer` port.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

/// The question sent with each slide image.
pub const PROMPT: &str = "This is a frame from a recorded presentation. \
Ignore any webcam or speaker video, burned-in captions, blacked-out areas, application toolbars and menus, \
window and document titles, tabs, and zoom indicators. \
If the frame shows more than one slide, describe only the one that fills most of the frame. \
Skip text that is cut off by the edge of the frame or hidden by a blacked-out area. \
Describe only the slide content, in this format:\n\
TITLE: the slide's own main heading, transcribed exactly (or 'None').\n\
TEXT: every other visible text, in reading order, one item per line.\n\
VISUALS: diagrams, icons, arrows and how they connect, in one or two sentences.\n\
Do not guess at anything you cannot read.";

/// Width, in pixels, of the image sent to the model.
pub const IMAGE_WIDTH: u32 = 1280;

/// What a vision model read on a slide.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SlideText {
    pub title: Option<String>,
    /// Other visible text, in reading order, one item per entry.
    pub text: Vec<String>,
    pub visuals: Option<String>,
    /// The model's answer as given, when it did not follow the format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unparsed: Option<String>,
}

fn none_if_empty(s: &str) -> Option<String> {
    let s = s.trim().trim_matches('*').trim();
    let lower = s.to_ascii_lowercase();
    (!s.is_empty() && lower != "none" && lower != "none." && lower != "'none'")
        .then(|| s.to_string())
}

/// A title wholly in parentheses or brackets is a sticker or a label, not a heading.
fn plausible_title(t: &str) -> bool {
    let wrapped =
        (t.starts_with('(') && t.ends_with(')')) || (t.starts_with('[') && t.ends_with(']'));
    !wrapped && t.chars().filter(|c| c.is_alphabetic()).count() >= 2
}

/// Reads a `TITLE: … TEXT: … VISUALS: …` answer. Anything else is kept as
/// `unparsed`.
pub fn parse(reply: &str) -> SlideText {
    #[derive(PartialEq)]
    enum At {
        Start,
        Title,
        Text,
        Visuals,
    }
    let mut out = SlideText::default();
    let (mut at, mut visuals) = (At::Start, String::new());
    for line in reply.lines() {
        let l = line.trim();
        let field = |name: &str| {
            l.strip_prefix(name)
                .or_else(|| l.strip_prefix(&format!("**{name}**")))
                .map(str::trim)
        };
        if let Some(rest) = field("TITLE:") {
            match none_if_empty(rest) {
                Some(t) if plausible_title(&t) => out.title = Some(t),
                Some(t) => out.text.push(t),
                None => {}
            }
            at = At::Title;
        } else if let Some(rest) = field("TEXT:") {
            out.text.extend(none_if_empty(rest));
            at = At::Text;
        } else if let Some(rest) = field("VISUALS:") {
            visuals.push_str(rest);
            at = At::Visuals;
        } else if !l.is_empty() {
            match at {
                At::Text | At::Title => {
                    let item = l.trim_start_matches(['-', '*', '•']).trim();
                    out.text.extend(none_if_empty(item));
                }
                At::Visuals => {
                    visuals.push(' ');
                    visuals.push_str(l);
                }
                At::Start => {}
            }
        }
    }
    out.visuals = none_if_empty(&visuals);
    if out.title.is_none()
        && out.text.is_empty()
        && out.visuals.is_none()
        && !reply.trim().is_empty()
    {
        out.unparsed = Some(reply.trim().to_string());
    }
    out
}

fn key(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Removes window chrome the model read as slide text: lines (and titles)
/// that appear on at least 3 slides and at least 40% of them, like a document
/// name in the title bar. Slides are passed in deck order.
pub fn drop_chrome(slides: &mut [&mut SlideText]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for s in slides.iter() {
        let lines: HashSet<String> = s.title.iter().chain(&s.text).map(|l| key(l)).collect();
        for l in lines {
            *seen.entry(l).or_default() += 1;
        }
    }
    let threshold = 3.max((slides.len() as f64 * 0.4).ceil() as usize);
    let chrome: HashSet<String> = seen
        .into_iter()
        .filter(|(_, n)| *n >= threshold)
        .map(|(l, _)| l)
        .collect();
    if chrome.is_empty() {
        return;
    }
    for s in slides.iter_mut() {
        if s.title.as_deref().is_some_and(|t| chrome.contains(&key(t))) {
            s.title = None;
        }
        s.text.retain(|l| !chrome.contains(&key(l)));
    }
}

/// The words a slide shows, lowercased, for comparing slides.
pub fn words(t: &SlideText) -> HashSet<String> {
    t.title
        .iter()
        .chain(&t.text)
        .flat_map(|l| l.split(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

/// Shared words over all words of two readings (Jaccard), 0 when either has
/// fewer than 4 words to compare.
pub fn similarity(a: &SlideText, b: &SlideText) -> f64 {
    let (wa, wb) = (words(a), words(b));
    if wa.len() < 4 || wb.len() < 4 {
        return 0.0;
    }
    wa.intersection(&wb).count() as f64 / wa.union(&wb).count() as f64
}

/// Two readings this alike are the same slide without asking.
pub const SAME_BY_TEXT: f64 = 0.6;
/// Neighbours this alike, but under `SAME_BY_TEXT`, are worth asking the
/// model about with both images: the canvas may be zoomed or panned, so the
/// readings differ even when the slide is the same.
pub const ASK_ABOVE: f64 = 0.15;

/// Whether two readings are of the same slide by their words alone.
pub fn same_slide(a: &SlideText, b: &SlideText) -> bool {
    similarity(a, b) >= SAME_BY_TEXT
}

/// The question sent with two consecutive slide images.
pub const SAME_PROMPT: &str = "These are two frames from a recorded presentation, in order. \
Ignore any webcam or speaker video, captions, blacked-out areas, toolbars and window titles. \
Is the second frame showing the same slide as the first, possibly zoomed, panned or with an \
annotation added? A different slide that shares a layout or a heading is DIFFERENT. \
Answer with one word: SAME or DIFFERENT.";

/// Reads the answer to `SAME_PROMPT`; None if it is neither.
pub fn parse_same(reply: &str) -> Option<bool> {
    let r = reply
        .trim()
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_ascii_uppercase();
    if r.starts_with("SAME") {
        Some(true)
    } else if r.starts_with("DIFFERENT") {
        Some(false)
    } else {
        None
    }
}
