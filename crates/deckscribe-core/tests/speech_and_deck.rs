//! Pieces, alignment and the deck's invariants.

use anyhow::Result;
use deckscribe_core::alignment::{assign, narrate};
use deckscribe_core::description::{
    drop_chrome, join_wrapped, parse, parse_same, same_slide, similarity,
};
use deckscribe_core::ports::Recognizer;
use deckscribe_core::speech::readable;
use deckscribe_core::speech::{AUDIO_RATE, pieces, transcribe};
use deckscribe_core::{Deck, DeckPart, PartKind, SlideText, Span, Word};
use deckscribe_core::{corrections, summary};

fn w(start: f64, end: f64, text: &str) -> Word {
    Word {
        start,
        end,
        text: text.into(),
    }
}

#[test]
fn pieces_respect_the_limit_and_cut_at_the_quiet_moment() {
    // 70 s of tone with silence at 28.0-28.3 s and 55.0-55.3 s.
    let mut pcm: Vec<f32> = (0..70 * AUDIO_RATE)
        .map(|i| (i as f32 * 0.05).sin() * 0.5)
        .collect();
    for (a, b) in [(28.0, 28.3), (55.0, 55.3)] {
        for s in &mut pcm[(a * AUDIO_RATE as f64) as usize..(b * AUDIO_RATE as f64) as usize] {
            *s = 0.0;
        }
    }
    let p = pieces(&pcm, 30 * AUDIO_RATE);
    assert_eq!(p.first().unwrap().0, 0);
    assert_eq!(p.last().unwrap().1, pcm.len());
    for w in p.windows(2) {
        assert_eq!(w[0].1, w[1].0, "pieces are contiguous");
    }
    for &(a, b) in &p {
        assert!(b - a <= 30 * AUDIO_RATE);
    }
    let first_cut = p[0].1 as f64 / AUDIO_RATE as f64;
    assert!(
        (28.0..28.3).contains(&first_cut),
        "first cut at {first_cut}"
    );
}

/// Says one word per piece, in the middle of it.
struct OneWord;
impl Recognizer for OneWord {
    fn recognize(&mut self, pcm: &[f32]) -> Result<Vec<Word>> {
        let mid = pcm.len() as f64 / AUDIO_RATE as f64 / 2.0;
        Ok(vec![w(mid - 0.1, mid + 0.1, " hello "), w(mid, mid, "  ")])
    }
}

#[test]
fn transcription_shifts_words_onto_the_timeline() {
    let pcm = vec![0.0f32; 65 * AUDIO_RATE];
    let words = transcribe(&mut OneWord, &pcm, 30).unwrap();
    assert_eq!(words.len(), 3, "blank words are dropped: {words:?}");
    assert!(words.iter().all(|w| w.text == "hello"));
    assert!(words.windows(2).all(|p| p[0].start < p[1].start));
    assert!(
        words[1].start > 30.0,
        "the second piece's word is offset: {:?}",
        words[1]
    );
}

#[test]
fn words_go_to_the_part_containing_their_midpoint() {
    let spans = [Span::new(0.0, 10.0), Span::new(10.0, 20.0)];
    let words = [
        w(1.0, 2.0, "a"),
        w(9.5, 10.1, "c"),
        w(9.8, 10.4, "b"),
        w(19.0, 25.0, "d"),
    ];
    assert_eq!(narrate(&words, &spans), ["a c", "b d"]);
}

fn part(number: usize, start: f64, end: f64) -> DeckPart {
    DeckPart {
        number,
        kind: PartKind::Slide,
        span: Span::new(start, end),
        image_at: Some(end - 1.0),
        image: None,
        narration: String::new(),
        words: 0..0,
        slide_text: None,
        same_as: None,
    }
}

#[test]
fn a_deck_with_a_gap_or_lost_words_is_rejected() {
    let mut deck = Deck {
        recording: "t.mp4".into(),
        about: Default::default(),
        duration: 20.0,
        parts: vec![part(1, 0.0, 10.0), part(2, 10.0, 20.0)],
        words: vec![w(1.0, 2.0, "hi")],
        summary: None,
        corrected: vec![],
        possible: vec![],
    };
    deck.parts[0].narration = "hi".into();
    deck.parts[0].words = 0..1;
    deck.parts[1].words = 1..1;
    deck.check().unwrap();

    let mut gap = deck.clone();
    gap.parts[1].span.start = 11.0;
    assert!(gap.check().is_err());

    let mut lost = deck.clone();
    lost.parts[0].words = 0..0;
    assert!(lost.check().is_err());

    let mut described_live = deck.clone();
    described_live.parts[1].kind = PartKind::Live;
    described_live.parts[1].slide_text = Some(SlideText::default());
    assert!(described_live.check().is_err());
}

#[test]
fn a_sentence_cut_by_a_slide_change_stays_with_its_slide() {
    let spans = [Span::new(0.0, 10.0), Span::new(10.0, 20.0)];
    let words = [
        w(8.0, 8.5, "I"),
        w(8.5, 9.5, "shipped"),
        w(9.5, 9.9, "two"),
        w(10.0, 10.5, "thousand"),
        w(10.5, 11.0, "PRs."),
        w(12.0, 12.5, "Next"),
        w(12.5, 13.0, "topic."),
    ];
    assert_eq!(
        narrate(&words, &spans),
        ["I shipped two thousand PRs.", "Next topic."]
    );
    // A sentence that runs on past the limit is cut where the slide changes.
    let long = [
        w(9.0, 9.5, "and"),
        w(10.0, 10.5, "then"),
        w(14.0, 14.5, "more."),
    ];
    assert_eq!(assign(&long, &spans, 3.0), [0..1, 1..3]);
}

#[test]
fn slide_text_is_read_into_fields() {
    let t = parse(
        "TITLE: High-quality verification\nTEXT: A feature map gives context.\n$ drive settings\n- run the real app\nVISUALS: A box connected to another\nby an arrow.",
    );
    assert_eq!(t.title.as_deref(), Some("High-quality verification"));
    assert_eq!(
        t.text,
        [
            "A feature map gives context.",
            "$ drive settings",
            "run the real app"
        ]
    );
    assert_eq!(
        t.visuals.as_deref(),
        Some("A box connected to another by an arrow.")
    );
    assert!(t.unparsed.is_none());

    let none = parse("TITLE: None\nTEXT: 1. trust\nVISUALS: None.");
    assert_eq!(none.title, None);
    assert_eq!(none.visuals, None);
    assert_eq!(none.text, ["1. trust"]);

    let free = parse("The slide shows a kitchen.");
    assert_eq!(free.unparsed.as_deref(), Some("The slide shows a kitchen."));
}

#[test]
fn fillers_leave_the_narration() {
    let words = [
        "Uh,", "so", "I", "um", "shipped", "it.", "You", "know,", "it", "worked,", "you", "know,",
        "really.",
    ];
    assert_eq!(readable(&words), "so I shipped it. it worked, really.");
    assert_eq!(readable(&["do", "you", "know", "it?"]), "do you know it?");
}

fn reading(title: &str, text: &[&str]) -> SlideText {
    SlideText {
        title: Some(title.into()),
        text: text.iter().map(|t| t.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn chrome_repeated_across_slides_is_dropped() {
    let mut a = reading(
        "cursor compile - lauren / Page 1",
        &["Share", "Trust your agents more"],
    );
    let mut b = reading(
        "Feature map",
        &["Share", "what exists", "cursor compile - lauren / Page 1"],
    );
    let mut c = reading(
        "CLI",
        &[
            "Share",
            "drive settings",
            "cursor compile - lauren / Page 1",
        ],
    );
    let mut d = reading("Automations", &["schedule agents"]);
    drop_chrome(&mut [&mut a, &mut b, &mut c, &mut d]);
    assert_eq!(a.title, None, "a document name is not a slide title");
    assert_eq!(a.text, ["Trust your agents more"]);
    assert_eq!(b.text, ["what exists"]);
    assert_eq!(d.text, ["schedule agents"]);
    assert_eq!(parse("TITLE: (good)\nTEXT: i shipped 2000 PRs").title, None);
}

#[test]
fn the_same_slide_read_twice_is_recognised() {
    let a = reading(
        "how do I trust my agents more?",
        &[
            "correctness",
            "skills that teach agents",
            "your codebase is memory",
        ],
    );
    let b = reading(
        "How do I trust my agents more?",
        &[
            "correctness",
            "skills that teach agents",
            "codebase is memory",
            "automations",
        ],
    );
    let c = reading(
        "Michelin kitchen",
        &["line cooks", "sous chefs", "equipment"],
    );
    assert!(same_slide(&a, &b), "{}", similarity(&a, &b));
    assert!(!same_slide(&a, &c));
    assert_eq!(parse_same("SAME"), Some(true));
    assert_eq!(parse_same("**DIFFERENT**."), Some(false));
    assert_eq!(parse_same("maybe"), None);
}

#[test]
fn mishearings_are_found_against_the_slide_on_screen() {
    let mut deck = Deck {
        recording: "t.mp4".into(),
        duration: 30.0,
        about: Default::default(),
        parts: vec![part(1, 0.0, 10.0), part(2, 10.0, 20.0), part(3, 20.0, 30.0)],
        words: vec![],
        summary: None,
        corrected: vec![],
        possible: vec![],
    };
    deck.parts[0].slide_text = Some(reading("Lauren Tan (@poteto)", &["Grok Bot at SpaceX"]));
    deck.parts[1].slide_text = Some(reading("Errors", &["Sentry", "pstack"]));
    deck.parts[2].slide_text = Some(reading("Kitchen", &["line cooks"]));
    deck.parts[0].narration = "You might know me as Potato. I work on Grogbot there.".into();
    deck.parts[1].narration =
        "Century catches it, and PSnack shows the threads. Their agents help.".into();
    deck.parts[2].narration = "The line cooks matter, and I will gloss over Grockbot again.".into();
    deck.parts[2].slide_text = Some(reading("Kitchen", &["line cooks", "Glass"]));
    deck.words = [
        "You", "might", "know", "me", "as", "Potato.", "I", "work", "on", "Grogbot", "there.",
        "Century", "catches", "it,", "and", "PSnack", "shows", "the", "threads.", "Their",
        "agents", "help.", "The", "line", "cooks", "matter,", "and", "Century", "again.",
    ]
    .iter()
    .map(|t| w(0.0, 0.1, t))
    .collect();
    let cands = corrections::candidates(&deck);
    let found: Vec<(&str, &str)> = cands
        .iter()
        .map(|(c, _)| (c.heard.as_str(), c.correct.as_str()))
        .collect();
    for want in [
        ("Potato", "poteto"),
        ("Grogbot", "Grok Bot"),
        ("PSnack", "pstack"),
        ("Grockbot", "Grok Bot"),
    ] {
        assert!(found.contains(&want), "{want:?} not in {found:?}");
    }
    // Ordinary English words are never changed, even when a slide shows a
    // word that sounds like them: "Century" stays, "gloss over" stays.
    assert!(
        found
            .iter()
            .all(|(h, _)| !["Century", "gloss", "Their", "agents"].contains(h)),
        "{found:?}"
    );
    let grog = cands
        .iter()
        .find(|(c, _)| c.heard == "Grogbot")
        .unwrap()
        .0
        .clone();
    corrections::apply(&mut deck, &[grog]);
    assert_eq!(
        deck.parts[0].narration,
        "You might know me as Potato. I work on Grok Bot there."
    );

    let s = summary::parse(
        "TITLE: Trusting your agents (proposed)\nSPEAKER: Lauren Tan\nKEY IDEAS:\n1. one\n- two\nthree",
    );
    assert_eq!(s.title.as_deref(), Some("Trusting your agents"));
    assert!(s.title_proposed);
    assert_eq!(s.speaker.as_deref(), Some("Lauren Tan"));
    assert_eq!(s.key_ideas, ["one", "two", "three"]);
}

#[test]
fn a_cut_inside_a_sentence_moves_to_the_nearest_sentence_end() {
    let spans = [Span::new(0.0, 10.0), Span::new(10.0, 20.0)];
    // "…a scale or | continuum …": the sentence ends 5 s after the change.
    let words = [
        w(7.0, 7.5, "Trust"),
        w(7.5, 8.0, "is"),
        w(8.0, 8.5, "a"),
        w(8.5, 9.0, "scale"),
        w(9.0, 9.5, "or"),
        w(10.5, 11.0, "continuum"),
        w(11.0, 11.5, "between"),
        w(14.5, 15.0, "pieces."),
        w(15.5, 16.0, "Next."),
    ];
    assert_eq!(assign(&words, &spans, 8.0), [0..8, 8..9]);
    // A sentence end just before the change is nearer: the cut moves back.
    let words = [
        w(8.5, 9.0, "Done."),
        w(9.2, 9.6, "So"),
        w(9.6, 9.9, "the"),
        w(10.1, 10.4, "last"),
        w(18.0, 18.5, "thing."),
    ];
    assert_eq!(assign(&words, &spans, 8.0), [0..1, 1..5]);
}

#[test]
fn wrapped_slide_lines_are_joined_and_lists_are_not() {
    let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        join_wrapped(lines(&["Architecture for", "agent-sized context"])),
        ["Architecture for agent-sized context"]
    );
    assert_eq!(
        join_wrapped(lines(&[
            "Don't show the confirmation dialog again even when I explicitly",
            "close it."
        ])),
        ["Don't show the confirmation dialog again even when I explicitly close it."]
    );
    assert_eq!(
        join_wrapped(lines(&["what exists", "how users reach it"])),
        ["what exists", "how users reach it"]
    );
    assert_eq!(
        join_wrapped(lines(&["1. trust", "2. how to trust your agents more"])).len(),
        2
    );
    assert_eq!(
        parse("TITLE: Thread # glass-oncall-assistant")
            .title
            .as_deref(),
        Some("Thread #glass-oncall-assistant")
    );
}

#[test]
fn separate_lowercase_items_are_not_joined() {
    let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        join_wrapped(lines(&[
            "A narrow local edit can stay correct for the whole Electron",
            "app."
        ]))
        .len(),
        1
    );
    let items = [
        "work like real software engineers: eg pstack",
        "refactoring/rewriting architecture to be (greenfield vs brownfield)",
        "agent friendly",
    ];
    assert_eq!(join_wrapped(lines(&items)).len(), 3);
    let wrapped = [
        "teach agents to work like real software engineers: eg",
        "pstack",
        "refactoring/rewriting architecture to be (greenfield vs",
        "brownfield)",
    ];
    assert_eq!(
        join_wrapped(lines(&wrapped)),
        [
            "teach agents to work like real software engineers: eg pstack",
            "refactoring/rewriting architecture to be (greenfield vs brownfield)",
        ]
    );
    let slack = [
        "reproduced on the prior commit, gone on the fix",
        "focusing another conversation leaves Apps closed",
    ];
    assert_eq!(join_wrapped(lines(&slack)).len(), 2);
    assert_eq!(
        parse("TEXT: • verification for correctness").text,
        ["verification for correctness"]
    );
}
