//! Description: what is asked of a vision model about a slide image, and how
//! its answer is read. The model itself is behind the `Describer` port.

use serde::Serialize;

/// The question sent with each slide image.
pub const PROMPT: &str = "This is a frame from a recorded presentation. \
Ignore any webcam or speaker video, burned-in captions, application toolbars and menus, and zoom indicators. \
Describe only the slide content, in this format:\n\
TITLE: the main heading, transcribed exactly (or 'None').\n\
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
            out.title = none_if_empty(rest);
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
