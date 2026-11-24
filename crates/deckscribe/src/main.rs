//! deckscribe: a recorded talk in; for every slide an image, the words spoken
//! over it and, optionally, what is on it out, as a folder written for agents.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use deckscribe::adapters::{bundle::Bundle, ffmpeg::Ffmpeg};
use deckscribe::app::{self, Adapters, Request};
use deckscribe_core::Settings;
use deckscribe_core::segmentation::overlays::BlockBox;

/// The guide for agents reading a deck or running deckscribe.
const SKILL: &str = include_str!("../../../skills/deckscribe/SKILL.md");

#[derive(Parser)]
#[command(version, about, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// The recording.
    #[arg(required = true)]
    recording: Option<PathBuf>,
    /// Output folder (default: <recording name>.deck next to the recording).
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Transcript only: do not look for slides.
    #[arg(long)]
    no_slides: bool,
    /// Slides only: no transcript, no model download.
    #[arg(long)]
    no_transcript: bool,
    /// Speech model file (default: Parakeet TDT 0.6B v2, downloaded once).
    #[arg(long)]
    model: Option<PathBuf>,

    /// Read each slide with this OpenAI-compatible server, e.g. http://127.0.0.1:8012.
    #[arg(long)]
    describe_url: Option<String>,
    /// Model name to ask the server for.
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

#[derive(Subcommand)]
enum Command {
    /// Print the guide for agents: how to read a deck and run deckscribe.
    Skill,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Skill) = cli.command {
        print!("{SKILL}");
        return Ok(());
    }
    let recording = cli.recording.clone().expect("clap requires a recording");
    let out = cli
        .out
        .clone()
        .unwrap_or_else(|| app::default_out(&recording));
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

    let media = Ffmpeg::new(&recording);
    let recognizer = make_recognizer(&cli)?;
    #[cfg(feature = "describe")]
    let describer = cli.describe_url.as_deref().map(|url| {
        deckscribe::adapters::openai_vision::OpenAiVision::new(
            url,
            &cli.describe_model,
            cli.no_think,
        )
    });
    #[cfg(not(feature = "describe"))]
    if cli.describe_url.is_some() {
        anyhow::bail!("this build cannot read slides; rebuild with the \"describe\" feature");
    }

    app::scribe(
        Request {
            recording: &recording,
            out: &out,
            settings: &settings,
            slides: !cli.no_slides,
            debug: cli.debug,
        },
        Adapters {
            frames: &media,
            audio: &media,
            stills: &media,
            recognizer,
            #[cfg(feature = "describe")]
            describer: describer
                .as_ref()
                .map(|d| d as &dyn deckscribe_core::ports::Describer),
            #[cfg(not(feature = "describe"))]
            describer: None,
            writers: vec![Box::new(Bundle)],
        },
    )?;
    println!(
        "{}",
        out.join(deckscribe::adapters::bundle::ENTRY).display()
    );
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
    let path = HuggingFaceStore {
        given: cli.model.clone(),
    }
    .speech_model()?;
    Ok(Some(Box::new(move || -> Result<Box<dyn Recognizer>> {
        let r = TranscribeCpp::load(&path)?;
        eprintln!("speech: {}", r.description);
        Ok(Box::new(r))
    })))
}

#[cfg(not(feature = "transcribe"))]
fn make_recognizer(cli: &Cli) -> Result<Option<app::MakeRecognizer<'static>>> {
    if !cli.no_transcript {
        anyhow::bail!(
            "this build has no transcription; pass --no-transcript or rebuild with the \"transcribe\" feature"
        );
    }
    Ok(None)
}
