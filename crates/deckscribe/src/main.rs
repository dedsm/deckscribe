//! deckscribe: a recorded talk in; for every slide an image, the words spoken
//! over it and, optionally, a description out.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use deckscribe::adapters::{ffmpeg::Ffmpeg, output};
use deckscribe::app::{self, Adapters, Request};
use deckscribe_core::Settings;
use deckscribe_core::ports::DeckWriter;
use deckscribe_core::segmentation::overlays::BlockBox;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Md,
    Json,
}

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The recording.
    recording: PathBuf,
    /// Output folder (default: <recording name>-deck next to the recording).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Which files to write.
    #[arg(long, value_enum, value_delimiter = ',', default_values = ["md", "json"])]
    format: Vec<Format>,

    /// Transcript only: do not look for slides.
    #[arg(long)]
    no_slides: bool,
    /// Slides only: no transcript, no model download.
    #[arg(long)]
    no_transcript: bool,
    /// Speech model file (default: Parakeet TDT 0.6B v2, downloaded once).
    #[arg(long)]
    model: Option<PathBuf>,

    /// Describe each slide with this OpenAI-compatible server, e.g. http://127.0.0.1:8012.
    #[arg(long)]
    describe_url: Option<String>,
    /// Model name to ask the description server for.
    #[arg(long, default_value = "gemma")]
    describe_model: String,
    /// Ask a llama.cpp server to answer without reasoning first (faster).
    #[arg(long)]
    no_think: bool,

    /// Do not look for webcam, caption or speaker-panel overlays.
    #[arg(long)]
    no_mask: bool,
    /// Also mask this region: x,y,w,h as fractions of the frame (repeatable).
    #[arg(long, value_parser = BlockBox::parse)]
    mask: Vec<BlockBox>,
    /// Use one mask for the whole recording instead of one per stretch between cuts.
    #[arg(long)]
    no_split: bool,
    /// Fold camera and video stretches into the next slide instead of keeping them as parts.
    #[arg(long)]
    no_live_parts: bool,
    /// Shortest still stretch that counts as a slide, in seconds.
    #[arg(long, default_value_t = 6.0)]
    min_slide: f64,
    /// Share of the unmasked picture that must move to end a slide.
    #[arg(long, default_value_t = 0.05)]
    change: f32,
    /// Neighbouring slides that differ in less than this share of the picture are merged.
    #[arg(long, default_value_t = 0.02)]
    same: f32,
    /// Longest piece of audio given to the speech model at once, in seconds.
    #[arg(long, default_value_t = 30)]
    chunk: usize,
    /// Also write debug.json (per-frame motion, stretches, settings).
    #[arg(long)]
    debug: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let out = cli.out.clone().unwrap_or_else(|| app::default_out(&cli.recording));
    let settings = Settings {
        find_overlays: !cli.no_mask,
        split_at_cuts: !cli.no_split,
        live_parts: !cli.no_live_parts,
        extra_masks: cli.mask.clone(),
        min_hold: cli.min_slide,
        hold_change: cli.change,
        same_slide: cli.same,
        piece_seconds: cli.chunk,
        ..Settings::default()
    };

    let media = Ffmpeg::new(&cli.recording);
    let recognizer = make_recognizer(&cli)?;
    #[cfg(feature = "describe")]
    let describer = cli
        .describe_url
        .as_deref()
        .map(|url| deckscribe::adapters::openai_vision::OpenAiVision::new(url, &cli.describe_model, cli.no_think));
    #[cfg(not(feature = "describe"))]
    if cli.describe_url.is_some() {
        anyhow::bail!("this build has no descriptions; rebuild with the \"describe\" feature");
    }
    let mut writers: Vec<Box<dyn DeckWriter>> = Vec::new();
    if cli.format.contains(&Format::Md) {
        writers.push(Box::new(output::Markdown));
    }
    if cli.format.contains(&Format::Json) {
        writers.push(Box::new(output::Json));
    }

    app::scribe(
        Request { recording: &cli.recording, out: &out, settings: &settings, slides: !cli.no_slides, debug: cli.debug },
        Adapters {
            frames: &media,
            audio: &media,
            stills: &media,
            recognizer,
            #[cfg(feature = "describe")]
            describer: describer.as_ref().map(|d| d as &dyn deckscribe_core::ports::Describer),
            #[cfg(not(feature = "describe"))]
            describer: None,
            writers,
        },
    )?;
    Ok(())
}

#[cfg(feature = "transcribe")]
fn make_recognizer(cli: &Cli) -> Result<Option<app::MakeRecognizer<'static>>> {
    use deckscribe::adapters::{model_store::HuggingFaceStore, transcribe_cpp::TranscribeCpp};
    use deckscribe_core::ports::{ModelStore, Recognizer};
    if cli.no_transcript {
        return Ok(None);
    }
    // Resolve (and download) the model before any decoding starts.
    let path = HuggingFaceStore { given: cli.model.clone() }.speech_model()?;
    Ok(Some(Box::new(move || -> Result<Box<dyn Recognizer>> {
        let r = TranscribeCpp::load(&path)?;
        eprintln!("speech: {}", r.description);
        Ok(Box::new(r))
    })))
}

#[cfg(not(feature = "transcribe"))]
fn make_recognizer(cli: &Cli) -> Result<Option<app::MakeRecognizer<'static>>> {
    if !cli.no_transcript {
        anyhow::bail!("this build has no transcription; pass --no-transcript or rebuild with the \"transcribe\" feature");
    }
    Ok(None)
}
