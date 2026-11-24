//! Lists mishearing candidates for an existing deck folder, to tune the rules
//! without re-running a recording: `cargo run --example mishearings -- talk.deck`.

use deckscribe_core::corrections::{Strength, candidates};
use deckscribe_core::{Deck, DeckPart, PartKind, SlideText, Span, Word};
use serde_json::Value;

fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: mishearings DECK_FOLDER"),
    );
    let deck: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("deck.json")).unwrap()).unwrap();
    let words: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("words.json")).unwrap()).unwrap();
    let words: Vec<Word> = words
        .iter()
        .map(|w| Word {
            start: w[0].as_f64().unwrap(),
            end: w[1].as_f64().unwrap(),
            text: w[2].as_str().unwrap().into(),
        })
        .collect();
    let parts = deck["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let t = &p["slide_text"];
            let slide_text = t.is_object().then(|| SlideText {
                title: t["title"].as_str().map(String::from),
                text: t["text"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Default::default()
            });
            // The deck's narration already has corrections applied; rebuild it from the words.
            let r =
                p["words"][0].as_u64().unwrap() as usize..p["words"][1].as_u64().unwrap() as usize;
            let narration = deckscribe_core::speech::readable(
                &words[r.clone()]
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>(),
            );
            DeckPart {
                number: p["number"].as_u64().unwrap() as usize,
                kind: if p["kind"] == "live" {
                    PartKind::Live
                } else {
                    PartKind::Slide
                },
                span: Span::new(p["start"].as_f64().unwrap(), p["end"].as_f64().unwrap()),
                image_at: None,
                image: None,
                narration,
                words: r,
                slide_text,
                same_as: None,
            }
        })
        .collect();
    let deck = Deck {
        recording: String::new(),
        duration: deck["duration"].as_f64().unwrap(),
        about: Default::default(),
        parts,
        words,
        summary: None,
        corrected: vec![],
        possible: vec![],
    };
    if std::env::args().any(|a| a == "--cuts") {
        let spans: Vec<Span> = deck.parts.iter().map(|p| p.span).collect();
        let ranges = deckscribe_core::alignment::assign(
            &deck.words,
            &spans,
            deckscribe_core::alignment::SENTENCE_SNAP,
        );
        for (k, r) in ranges.iter().enumerate().skip(1) {
            let prev = &ranges[k - 1];
            let tail: Vec<&str> = deck.words[prev.end.saturating_sub(4).max(prev.start)..prev.end]
                .iter()
                .map(|w| w.text.as_str())
                .collect();
            let head: Vec<&str> = deck.words[r.start..(r.start + 4).min(r.end)]
                .iter()
                .map(|w| w.text.as_str())
                .collect();
            println!(
                "#{:<2} …{:<40} | #{:<2} {}…",
                k,
                tail.join(" "),
                k + 1,
                head.join(" ")
            );
        }
        return;
    }
    for (c, s) in candidates(&deck) {
        let tag = if s == Strength::Strong {
            "fix "
        } else {
            "maybe"
        };
        println!("{tag}  {:<14} -> {:<14} {:?}", c.heard, c.correct, c.parts);
    }
}
