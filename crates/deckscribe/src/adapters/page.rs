//! `deck.html`: the deck for people. One self-contained page, opened straight
//! from the folder: each part's image beside what was said over it, with the
//! summary and a jump list on top. Images are linked from `slides/`, so the
//! page works only next to them.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::Result;
use deckscribe_core::ports::DeckWriter;
use deckscribe_core::time::clock;
use deckscribe_core::{Deck, DeckPart, PartKind};

pub const PAGE: &str = "deck.html";

pub struct Page;

impl DeckWriter for Page {
    fn write(&self, deck: &Deck, out: &Path) -> Result<()> {
        fs::write(out.join(PAGE), html(deck))?;
        Ok(())
    }
}

/// Escapes text for HTML element content and quoted attributes.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// A part's title, or for a return, the title of the part it returns to.
fn title_of<'a>(deck: &'a Deck, p: &'a DeckPart) -> Option<&'a str> {
    p.same_as
        .and_then(|n| deck.parts.iter().find(|q| q.number == n))
        .unwrap_or(p)
        .title()
}

fn label(deck: &Deck, p: &DeckPart) -> String {
    match (p.kind, title_of(deck, p)) {
        (PartKind::Slide, Some(t)) => t.to_string(),
        (PartKind::Slide, None) => "Slide".to_string(),
        (PartKind::Live, _) => "Speaker or video".to_string(),
        (PartKind::Whole, _) => "Whole recording".to_string(),
    }
}

const STYLE: &str = r#"
:root{--bg:#f6f5f1;--card:#fff;--fg:#1d1d1b;--muted:#6b6a64;--line:#e2e0d8;--accent:#2f5d8a;--live:#8a5a2f}
@media (prefers-color-scheme:dark){:root{--bg:#161615;--card:#1f1f1d;--fg:#ecebe6;--muted:#9c9a92;--line:#33322e;--accent:#8fb6dd;--live:#d9a878}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:1280px;margin:0 auto;padding:24px max(16px,3vw) 64px}
header h1{font-size:1.9rem;line-height:1.25;margin:0 0 4px;text-wrap:balance}
.meta{color:var(--muted);font-size:.9rem;margin:0 0 16px}
.summary{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px 20px;margin:0 0 20px}
.summary ul{margin:8px 0 0;padding-left:20px}
.summary .note{color:var(--muted);font-size:.85rem;margin:8px 0 0}
nav details{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:10px 16px;margin:0 0 28px}
nav summary{cursor:pointer;font-weight:600}
nav ol{list-style:none;margin:10px 0 0;padding:0;columns:2 320px;column-gap:24px}
nav li{break-inside:avoid;font-size:.9rem;padding:2px 0}
nav a{color:var(--fg);text-decoration:none}
nav a:hover{color:var(--accent)}
.t{font-variant-numeric:tabular-nums;color:var(--muted)}
.part{display:grid;grid-template-columns:minmax(0,5fr) minmax(0,6fr);gap:24px;align-items:start;
  padding:24px 0;border-top:1px solid var(--line);scroll-margin-top:8px}
.part figure{margin:0;position:sticky;top:12px}
.part img{display:block;width:100%;height:auto;border-radius:6px;border:1px solid var(--line);background:#000}
.part h2{font-size:1.15rem;line-height:1.35;margin:0 0 6px;text-wrap:balance}
.part h2 .n{color:var(--accent);margin-right:6px}
.part.live h2 .n{color:var(--live)}
.part .when{font-size:.85rem;margin:0 0 12px}
.part .when a{color:var(--muted)}
.said{margin:0;max-width:70ch}
.said.none{color:var(--muted);font-style:italic}
.onslide{margin:12px 0 0;font-size:.9rem;color:var(--muted)}
.onslide summary{cursor:pointer}
.onslide ul{margin:6px 0;padding-left:20px}
.back{color:var(--muted);font-size:.9rem}
@media (max-width:760px){.part{grid-template-columns:1fr;gap:12px}.part figure{position:static}}
"#;

fn html(deck: &Deck) -> String {
    let heading = deck
        .summary
        .as_ref()
        .and_then(|s| s.title.clone())
        .unwrap_or_else(|| deck.recording.clone());
    let mut s = String::new();
    let _ = write!(
        s,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"color-scheme\" content=\"light dark\">\n\
         <title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n<header>\n<h1>{}</h1>\n",
        esc(&heading),
        esc(&heading)
    );
    let mut counts = vec![format!("{} slides", deck.slides())];
    if deck.live_parts() > 0 {
        counts.push(format!("{} speaker or video parts", deck.live_parts()));
    }
    let _ = writeln!(
        s,
        "<p class=\"meta\">{} · {} · {} · {}</p>\n</header>",
        esc(&deck.recording),
        clock(deck.duration),
        counts.join(", "),
        esc(&deck.about.tool)
    );

    if let Some(sm) = &deck.summary {
        s.push_str("<section class=\"summary\">\n");
        if let Some(sp) = &sm.speaker {
            let _ = writeln!(s, "<div><strong>Speaker:</strong> {}</div>", esc(sp));
        }
        if !sm.key_ideas.is_empty() {
            s.push_str("<strong>Key ideas</strong>\n<ul>\n");
            for k in &sm.key_ideas {
                let _ = writeln!(s, "<li>{}</li>", esc(k));
            }
            s.push_str("</ul>\n");
        }
        s.push_str(
            "<p class=\"note\">Written by a text model from the slides and the transcript; \
             check claims against the parts below.</p>\n</section>\n",
        );
    }

    let _ = writeln!(
        s,
        "<nav><details><summary>All {} parts</summary>\n<ol>",
        deck.parts.len()
    );
    for p in &deck.parts {
        let _ = writeln!(
            s,
            "<li><a href=\"#p{n}\"><span class=\"t\">#{n} {}</span> {}</a></li>",
            clock(p.span.start),
            esc(&label(deck, p)),
            n = p.number
        );
    }
    s.push_str("</ol></details></nav>\n");

    let has_speech = !deck.words.is_empty() || deck.about.speech_model.is_some();
    for p in &deck.parts {
        let live = if p.kind == PartKind::Slide {
            ""
        } else {
            " live"
        };
        let _ = writeln!(
            s,
            "<section class=\"part{live}\" id=\"p{n}\">",
            n = p.number
        );
        match &p.image {
            Some(img) => {
                let _ = writeln!(
                    s,
                    "<figure><a href=\"{src}\"><img src=\"{src}\" alt=\"Part {n}\" loading=\"lazy\"></a></figure>",
                    src = esc(img),
                    n = p.number
                );
            }
            None => s.push_str("<figure></figure>\n"),
        }
        s.push_str("<div>\n");
        let _ = writeln!(
            s,
            "<h2><span class=\"n\">#{}</span>{}</h2>",
            p.number,
            esc(&label(deck, p))
        );
        let _ = writeln!(
            s,
            "<p class=\"when t\"><a href=\"#p{n}\">{}–{}</a></p>",
            clock(p.span.start),
            clock(p.span.end),
            n = p.number
        );
        if has_speech {
            if p.narration.is_empty() {
                s.push_str("<p class=\"said none\">Nothing was said.</p>\n");
            } else {
                let _ = writeln!(s, "<p class=\"said\">{}</p>", esc(&p.narration));
            }
        }
        if let Some(n) = p.same_as {
            let _ = writeln!(
                s,
                "<p class=\"back\">Back to <a href=\"#p{n}\">#{n}</a>; its slide text is there.</p>"
            );
        } else if let Some(t) = &p.slide_text {
            s.push_str("<details class=\"onslide\"><summary>On the slide</summary>\n");
            if !t.text.is_empty() {
                s.push_str("<ul>\n");
                for line in &t.text {
                    let _ = writeln!(s, "<li>{}</li>", esc(line));
                }
                s.push_str("</ul>\n");
            }
            if let Some(v) = &t.visuals {
                let _ = writeln!(s, "<p><strong>Visuals:</strong> {}</p>", esc(v));
            }
            if let Some(raw) = &t.unparsed {
                let _ = writeln!(s, "<p>{}</p>", esc(raw));
            }
            s.push_str("</details>\n");
        }
        s.push_str("</div>\n</section>\n");
    }
    s.push_str("</main>\n</body>\n</html>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckscribe_core::About;
    use deckscribe_core::time::Span;

    fn part(number: usize, kind: PartKind, narration: &str) -> DeckPart {
        DeckPart {
            number,
            kind,
            span: Span {
                start: (number - 1) as f64 * 10.0,
                end: number as f64 * 10.0,
            },
            image_at: None,
            image: Some(format!("slides/slide-{number:02}.jpg")),
            narration: narration.to_string(),
            words: 0..0,
            slide_text: None,
            same_as: None,
        }
    }

    #[test]
    fn page_escapes_and_links_every_part() {
        let deck = Deck {
            recording: "talk <1>.mp4".into(),
            duration: 20.0,
            about: About {
                speech_model: Some("parakeet".into()),
                ..About::default()
            },
            parts: vec![
                part(1, PartKind::Slide, "a < b & \"c\""),
                part(2, PartKind::Live, ""),
            ],
            words: Vec::new(),
            summary: None,
            corrected: Vec::new(),
            possible: Vec::new(),
        };
        let h = html(&deck);
        assert!(h.contains("<title>talk &lt;1&gt;.mp4</title>"));
        assert!(h.contains("a &lt; b &amp; &quot;c&quot;"));
        assert!(!h.contains("a < b"));
        assert!(h.contains("href=\"#p1\"") && h.contains("id=\"p2\""));
        assert!(h.contains("src=\"slides/slide-02.jpg\""));
        assert!(h.contains("class=\"part live\""));
    }
}
