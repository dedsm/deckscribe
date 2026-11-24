//! Pieces, alignment and the deck's invariants.

use anyhow::Result;
use deckscribe_core::alignment::{assign, narrate};
use deckscribe_core::description::parse;
use deckscribe_core::ports::Recognizer;
use deckscribe_core::speech::{AUDIO_RATE, pieces, transcribe};
use deckscribe_core::{Deck, DeckPart, PartKind, SlideText, Span, Word};

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
        slide_text: None,
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
    };
    deck.parts[0].narration = "hi".into();
    deck.check().unwrap();

    let mut gap = deck.clone();
    gap.parts[1].span.start = 11.0;
    assert!(gap.check().is_err());

    let mut lost = deck.clone();
    lost.parts[0].narration.clear();
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
