//! The one use case: scribe a recording into a deck.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use deckscribe_core::description::{IMAGE_WIDTH, PROMPT};
use deckscribe_core::frames::FrameFeatures;
use deckscribe_core::ports::{AudioSource, DeckWriter, Describer, FrameSource, Recognizer, StillSource};
use deckscribe_core::segmentation::{self, Found, Segmentation};
use deckscribe_core::speech::{self, AUDIO_RATE};
use deckscribe_core::{Deck, DeckPart, PartKind, Settings, Span, Word, alignment};
use serde_json::json;

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
    pub describer: Option<&'a dyn Describer>,
    pub writers: Vec<Box<dyn DeckWriter + 'a>>,
}

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
            s.spawn(move || -> Result<(Vec<Word>, f64)> {
                let pcm = audio.audio()?;
                let seconds = pcm.len() as f64 / AUDIO_RATE as f64;
                let t = Instant::now();
                let mut recognizer = make()?;
                let words = speech::transcribe(recognizer.as_mut(), &pcm, settings.piece_seconds)?;
                eprintln!("transcribed {} words in {:.0}s", words.len(), t.elapsed().as_secs_f64());
                Ok((words, seconds))
            })
        });
        let picture = req.slides.then(|| -> Result<FrameFeatures> {
            let t = Instant::now();
            let mut features = FrameFeatures::new();
            ad.frames.frames(&mut |f| features.push(f))?;
            eprintln!("read {} frames in {:.0}s", features.count, t.elapsed().as_secs_f64());
            Ok(features)
        });
        (picture, sound.map(|h| h.join().expect("the sound lane panicked")))
    });
    let features = picture.transpose()?;
    let (words, audio_seconds) = sound.transpose()?.unwrap_or_default();
    let duration = audio_seconds.max(features.as_ref().map_or(0.0, |f| f.seconds()));

    let mut parts: Vec<DeckPart> = Vec::new();
    let mut segmentation: Option<Segmentation> = None;
    if let Some(features) = &features {
        let seg = segmentation::segment(features, settings, duration);
        report(&seg);
        fs::create_dir_all(req.out.join("slides"))?;
        for (k, p) in seg.parts.iter().enumerate() {
            let image = format!("slides/slide_{:03}.jpg", k + 1);
            ad.stills.save_still(p.image_at, &req.out.join(&image))?;
            let kind = if p.kind == Found::Slide { PartKind::Slide } else { PartKind::Live };
            parts.push(DeckPart {
                number: k + 1,
                kind,
                span: p.span,
                image_at: Some(p.image_at),
                image: Some(image),
                narration: String::new(),
                description: None,
            });
        }
        segmentation = Some(seg);
    } else {
        parts.push(DeckPart {
            number: 1,
            kind: PartKind::Whole,
            span: Span::new(0.0, duration.max(f64::MIN_POSITIVE)),
            image_at: None,
            image: None,
            narration: String::new(),
            description: None,
        });
    }

    let spans: Vec<Span> = parts.iter().map(|p| p.span).collect();
    for (p, n) in parts.iter_mut().zip(alignment::narrate(&words, &spans)) {
        p.narration = n;
    }

    if let (Some(describer), Some(seg)) = (ad.describer, &segmentation) {
        let total = parts.len();
        for (p, found) in parts.iter_mut().zip(&seg.parts) {
            if p.kind != PartKind::Slide {
                continue;
            }
            let mut boxes = seg.stretches.list[found.stretch].overlays.clone();
            boxes.extend_from_slice(&settings.extra_masks);
            let blank: Vec<[f32; 4]> = boxes.iter().map(|b| b.fractions()).collect();
            let image = req.out.join(p.image.as_deref().expect("slides have images"));
            let t = Instant::now();
            let jpeg = ad.stills.prepared(&image, IMAGE_WIDTH, &blank)?;
            p.description = Some(describer.describe(&jpeg, PROMPT)?);
            eprintln!("described part {}/{total} in {:.0}s", p.number, t.elapsed().as_secs_f64());
        }
    }

    let deck = Deck {
        recording: req.recording.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        duration,
        parts,
        words,
    };
    deck.check().context("the deck is inconsistent")?;
    for w in &ad.writers {
        w.write(&deck, req.out)?;
    }
    if let (true, Some(seg)) = (req.debug, &segmentation) {
        let dbg = json!({"stretches": seg.stretches.list, "settings": settings,
                         "fps": deckscribe_core::frames::FPS, "motion": seg.motion});
        fs::write(req.out.join("debug.json"), serde_json::to_string(&dbg)?)?;
    }
    eprintln!("wrote {} in {:.0}s", req.out.display(), started.elapsed().as_secs_f64());
    Ok(deck)
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
    eprintln!("found {slides} slides and {} live parts", seg.parts.len() - slides);
}

/// `<recording name>-deck` next to the recording.
pub fn default_out(recording: &Path) -> PathBuf {
    let stem = recording.file_stem().unwrap_or_default().to_string_lossy();
    recording.with_file_name(format!("{stem}-deck"))
}
