//! The one use case: scribe a recording into a deck.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use deckscribe_core::description::{self, ASK_ABOVE, IMAGE_WIDTH, PROMPT, SAME_PROMPT};
use deckscribe_core::frames::{FPS, FrameFeatures};
use deckscribe_core::ports::{
    AudioSource, DeckWriter, Describer, FrameSource, Recognizer, StillSource, TextModel,
};
use deckscribe_core::segmentation::{self, Found, Segmentation, overlays};
use deckscribe_core::speech::{self, AUDIO_RATE};
use deckscribe_core::time::clock;
use deckscribe_core::{
    About, Deck, DeckPart, PartKind, Settings, Span, Word, alignment, corrections, summary,
};
use serde_json::json;

use crate::adapters::bundle::CONTACT_SHEET;

pub struct Request<'a> {
    pub recording: &'a Path,
    pub out: &'a Path,
    pub settings: &'a Settings,
    /// Look for slides (the picture lane).
    pub slides: bool,
    /// Write debug.json with per-frame motion and the stretches.
    pub debug: bool,
}

/// Makes the recognizer on the sound lane's own thread.
pub type MakeRecognizer<'a> = Box<dyn FnOnce() -> Result<Box<dyn Recognizer>> + Send + 'a>;

pub struct Adapters<'a> {
    pub frames: &'a dyn FrameSource,
    pub audio: &'a dyn AudioSource,
    pub stills: &'a dyn StillSource,
    /// None: no transcript (the sound lane is off).
    pub recognizer: Option<MakeRecognizer<'a>>,
    /// None: slides are not read.
    pub describer: Option<&'a dyn Describer>,
    /// None: no summary.
    pub text_model: Option<&'a dyn TextModel>,
    pub writers: Vec<Box<dyn DeckWriter + 'a>>,
}

struct Speech {
    words: Vec<Word>,
    seconds: f64,
    model: String,
}

/// A part while the deck is being put together.
struct Draft {
    part: DeckPart,
    /// The image as sent to the vision model, kept to compare neighbours.
    seen: Option<Vec<u8>>,
    /// The saved still, under a working name until parts are final.
    still: Option<PathBuf>,
}

/// Seconds either side of a slide's image in which an overlay counts as active.
const ACTIVE_WINDOW: f64 = 2.0;

pub fn scribe(req: Request, ad: Adapters) -> Result<Deck> {
    if !req.slides && ad.recognizer.is_none() {
        bail!("nothing to do: both slides and the transcript are turned off");
    }
    fs::create_dir_all(req.out).with_context(|| format!("creating {}", req.out.display()))?;
    let started = Instant::now();
    let settings = req.settings;
    let audio = ad.audio;

    // Picture and sound in parallel: the picture lane is bound by decoding on
    // the CPU, the sound lane by the recognizer on the GPU.
    let (picture, sound) = std::thread::scope(|s| {
        let sound = ad.recognizer.map(|make| {
            s.spawn(move || -> Result<Speech> {
                let pcm = audio.audio()?;
                let seconds = pcm.len() as f64 / AUDIO_RATE as f64;
                let t = Instant::now();
                let mut recognizer = make()?;
                let words = speech::transcribe(recognizer.as_mut(), &pcm, settings.piece_seconds)?;
                eprintln!(
                    "transcribed {} words in {:.0}s",
                    words.len(),
                    t.elapsed().as_secs_f64()
                );
                Ok(Speech {
                    words,
                    seconds,
                    model: recognizer.name(),
                })
            })
        });
        let picture = req.slides.then(|| -> Result<FrameFeatures> {
            let t = Instant::now();
            let mut features = FrameFeatures::new();
            ad.frames.frames(&mut |f| features.push(f))?;
            eprintln!(
                "read {} frames in {:.0}s",
                features.count,
                t.elapsed().as_secs_f64()
            );
            Ok(features)
        });
        (
            picture,
            sound.map(|h| h.join().expect("the sound lane panicked")),
        )
    });
    let features = picture.transpose()?;
    let speech = sound.transpose()?;
    let duration = speech
        .as_ref()
        .map_or(0.0, |s| s.seconds)
        .max(features.as_ref().map_or(0.0, |f| f.seconds()));
    let mut about = About {
        tool: format!("deckscribe {}", env!("CARGO_PKG_VERSION")),
        created_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        speech_model: speech
            .as_ref()
            .map(|s| s.model.clone())
            .filter(|m| !m.is_empty()),
        vision_model: ad.describer.map(|d| d.name()).filter(|m| !m.is_empty()),
        masked: Vec::new(),
    };
    let words = speech.map(|s| s.words).unwrap_or_default();

    let mut drafts: Vec<Draft> = Vec::new();
    let mut segmentation: Option<Segmentation> = None;
    if let Some(features) = &features {
        let seg = segmentation::segment(features, settings, duration);
        report(&seg);
        about.masked = seg
            .stretches
            .overlay_summary()
            .iter()
            .map(|(b, secs)| {
                let when = if *secs >= duration * 0.95 {
                    "throughout".to_string()
                } else {
                    format!("for {} in all", clock(*secs))
                };
                format!("{} ({when})", b.describe())
            })
            .collect();
        let work = req.out.join("slides");
        fs::create_dir_all(&work)?;
        for (k, p) in seg.parts.iter().enumerate() {
            let still = work.join(format!(".part-{k:04}.jpg"));
            ad.stills.save_still(p.image_at, &still)?;
            let kind = if p.kind == Found::Slide {
                PartKind::Slide
            } else {
                PartKind::Live
            };
            let part = DeckPart {
                number: k + 1,
                kind,
                span: p.span,
                image_at: Some(p.image_at),
                image: None,
                narration: String::new(),
                words: 0..0,
                slide_text: None,
                same_as: None,
            };
            drafts.push(Draft {
                part,
                seen: None,
                still: Some(still),
            });
        }

        if let Some(describer) = ad.describer {
            read_slides(&mut drafts, &seg, features, settings, ad.stills, describer)?;
            merge_repeats(&mut drafts, describer)?;
        }
        segmentation = Some(seg);
    } else {
        let part = DeckPart {
            number: 1,
            kind: PartKind::Whole,
            span: Span::new(0.0, duration.max(f64::MIN_POSITIVE)),
            image_at: None,
            image: None,
            narration: String::new(),
            words: 0..0,
            slide_text: None,
            same_as: None,
        };
        drafts.push(Draft {
            part,
            seen: None,
            still: None,
        });
    }

    // Final numbering, image names, returns to earlier slides, contact sheet.
    let digits = drafts.len().to_string().len().max(2);
    let mut sheet: Vec<(PathBuf, String)> = Vec::new();
    for (k, d) in drafts.iter_mut().enumerate() {
        d.part.number = k + 1;
        if let Some(still) = &d.still {
            let image = format!("slides/slide-{:0digits$}.jpg", k + 1);
            fs::rename(still, req.out.join(&image))?;
            let label = if d.part.kind == PartKind::Slide {
                format!("#{}", k + 1)
            } else {
                format!("#{} live", k + 1)
            };
            sheet.push((req.out.join(&image), label));
            d.part.image = Some(image);
        }
    }
    for k in 0..drafts.len() {
        let Some(text) = drafts[k].part.slide_text.as_ref() else {
            continue;
        };
        let earlier = drafts[..k.saturating_sub(1)]
            .iter()
            .find(|e| {
                e.part.same_as.is_none()
                    && e.part
                        .slide_text
                        .as_ref()
                        .is_some_and(|t| description::same_slide(t, text))
            })
            .map(|e| e.part.number);
        drafts[k].part.same_as = earlier;
        if earlier.is_some() {
            // The earlier part's reading stands for both.
            drafts[k].part.slide_text = None;
        }
    }
    if !sheet.is_empty() {
        ad.stills
            .contact_sheet(&sheet, 5, &req.out.join(CONTACT_SHEET))?;
    }
    // Leftover working stills of merged parts.
    if let Ok(dir) = fs::read_dir(req.out.join("slides")) {
        for e in dir.flatten() {
            if e.file_name().to_string_lossy().starts_with(".part-") {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    let mut parts: Vec<DeckPart> = drafts.into_iter().map(|d| d.part).collect();
    let spans: Vec<Span> = parts.iter().map(|p| p.span).collect();
    for (p, r) in parts
        .iter_mut()
        .zip(alignment::assign(&words, &spans, alignment::SENTENCE_SNAP))
    {
        p.narration = speech::readable(
            &words[r.clone()]
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>(),
        );
        p.words = r;
    }

    let mut deck = Deck {
        recording: req
            .recording
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        duration,
        about,
        parts,
        words,
        summary: None,
        corrected: Vec::new(),
        possible: Vec::new(),
    };
    deck.check().context("the deck is inconsistent")?;

    for (c, strength) in corrections::candidates(&deck) {
        match strength {
            corrections::Strength::Strong => deck.corrected.push(c),
            corrections::Strength::Weak => deck.possible.push(c),
        }
    }
    let fixed = deck.corrected.clone();
    corrections::apply(&mut deck, &fixed);
    eprintln!(
        "{} mishearings corrected from the slides, {} listed as possible",
        deck.corrected.len(),
        deck.possible.len()
    );
    if let Some(model) = ad.text_model {
        let t = Instant::now();
        deck.summary = Some(summary::parse(&model.complete(
            &summary::prompt(&deck),
            6000,
            true,
        )?));
        eprintln!("summarized in {:.0}s", t.elapsed().as_secs_f64());
    }

    for w in &ad.writers {
        w.write(&deck, req.out)?;
    }
    if let (true, Some(seg)) = (req.debug, &segmentation) {
        let dbg = json!({"stretches": seg.stretches.list, "settings": settings,
                         "fps": FPS, "motion": seg.motion});
        fs::write(req.out.join("debug.json"), serde_json::to_string(&dbg)?)?;
    }
    eprintln!(
        "wrote {} in {:.0}s",
        req.out.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(deck)
}

/// Reads every slide with the vision model. Only the overlay blocks that were
/// changing around the image's moment are painted out, so slide text sharing
/// an overlay's region stays readable. Then drops window chrome.
fn read_slides(
    drafts: &mut [Draft],
    seg: &Segmentation,
    features: &FrameFeatures,
    settings: &Settings,
    stills: &dyn StillSource,
    describer: &dyn Describer,
) -> Result<()> {
    let total = drafts
        .iter()
        .filter(|d| d.part.kind == PartKind::Slide)
        .count();
    let mut done = 0;
    let window = (ACTIVE_WINDOW * FPS).round() as usize;
    for d in drafts.iter_mut().filter(|d| d.part.kind == PartKind::Slide) {
        let at = d.part.image_at.expect("slides have an image instant");
        let frame = ((at * FPS).round() as usize).min(features.count.saturating_sub(1));
        let mut paint = seg.stretches.active_mask(features, frame, window);
        for (m, e) in paint.iter_mut().zip(overlays::mask(&settings.extra_masks)) {
            *m |= e;
        }
        let t = Instant::now();
        let still = d.still.as_ref().expect("parts with images have stills");
        let jpeg = stills.prepared(still, IMAGE_WIDTH, &overlays::rects(&paint))?;
        d.part.slide_text = Some(description::parse(&describer.ask(&[&jpeg], PROMPT)?));
        d.seen = Some(jpeg);
        done += 1;
        eprintln!(
            "read slide {done}/{total} in {:.0}s",
            t.elapsed().as_secs_f64()
        );
    }
    let mut texts: Vec<&mut deckscribe_core::SlideText> = drafts
        .iter_mut()
        .filter_map(|d| d.part.slide_text.as_mut())
        .collect();
    description::drop_chrome(&mut texts);
    Ok(())
}

/// Merges neighbouring slides that are the same slide. Clear cases are
/// settled by their words; doubtful ones (some shared words) are put to the
/// model with both images.
fn merge_repeats(drafts: &mut Vec<Draft>, describer: &dyn Describer) -> Result<()> {
    let mut asked = 0;
    let mut k = 1;
    while k < drafts.len() {
        let (a, b) = (&drafts[k - 1], &drafts[k]);
        let same = match (&a.part.slide_text, &b.part.slide_text) {
            (Some(ta), Some(tb))
                if a.part.kind == PartKind::Slide && b.part.kind == PartKind::Slide =>
            {
                let sim = description::similarity(ta, tb);
                if description::same_slide(ta, tb) {
                    true
                } else if sim >= ASK_ABOVE {
                    asked += 1;
                    let (ia, ib) = (
                        a.seen.as_deref().unwrap_or_default(),
                        b.seen.as_deref().unwrap_or_default(),
                    );
                    description::parse_same(&describer.ask(&[ia, ib], SAME_PROMPT)?)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            _ => false,
        };
        if !same {
            k += 1;
            continue;
        }
        let b = drafts.remove(k);
        let a = &mut drafts[k - 1];
        a.part.span = Span::new(a.part.span.start, b.part.span.end);
        // Keep the reading with more on it, and its image.
        let richer = |d: &Draft| {
            d.part
                .slide_text
                .as_ref()
                .map_or(0, |t| description::words(t).len())
        };
        if richer(&b) > richer(a) {
            if let Some(old) = a.still.take() {
                let _ = fs::remove_file(old);
            }
            a.still = b.still;
            a.seen = b.seen;
            a.part.slide_text = b.part.slide_text;
            a.part.image_at = b.part.image_at;
        } else if let Some(old) = b.still {
            let _ = fs::remove_file(old);
        }
    }
    eprintln!(
        "{} parts after merging repeats ({asked} pairs checked with the model)",
        drafts.len()
    );
    Ok(())
}

fn report(seg: &Segmentation) {
    eprintln!(
        "{} stretches between cuts; {:.0}s live",
        seg.stretches.list.len(),
        seg.stretches.live_seconds()
    );
    let summary = seg.stretches.overlay_summary();
    for (b, secs) in summary.iter().take(8) {
        eprintln!("masking overlay: {} ({secs:.0}s)", b.describe());
    }
    if summary.len() > 8 {
        eprintln!("masking {} more overlay boxes", summary.len() - 8);
    }
    let slides = seg.parts.iter().filter(|p| p.kind == Found::Slide).count();
    eprintln!(
        "found {slides} slides and {} live parts",
        seg.parts.len() - slides
    );
}

/// `<recording name>.deck` next to the recording.
pub fn default_out(recording: &Path) -> PathBuf {
    let stem = recording.file_stem().unwrap_or_default().to_string_lossy();
    recording.with_file_name(format!("{stem}.deck"))
}
