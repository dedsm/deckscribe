//! deckscribe: a presentation video in; for every slide an image, the words
//! spoken over it and (optionally) a description out.

mod describe;
mod frames;
mod holds;
mod media;
mod model;
mod overlays;
mod segments;
mod transcribe;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::json;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// The presentation video.
    video: PathBuf,
    /// Output folder (default: <video name>-slides next to the video).
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Speech model file (default: Parakeet TDT 0.6B v2, downloaded once).
    #[arg(long)]
    model: Option<PathBuf>,
    /// Describe each slide with this OpenAI-compatible server, e.g. http://127.0.0.1:8012.
    #[arg(long)]
    describe_url: Option<String>,
    /// Model name to ask the description server for.
    #[arg(long, default_value = "gemma")]
    describe_model: String,
    /// Do not look for webcam or caption overlays.
    #[arg(long)]
    no_mask: bool,
    /// Mask this region too: x,y,w,h as fractions of the frame (repeatable).
    #[arg(long, value_parser = overlays::BlockBox::parse)]
    mask: Vec<overlays::BlockBox>,
    /// Shortest still stretch that counts as a slide, in seconds.
    #[arg(long, default_value_t = 6.0)]
    min_slide: f64,
    /// Share of the (unmasked) picture that must move to end a slide.
    #[arg(long, default_value_t = 0.05)]
    change: f32,
    /// Neighbouring slides that differ in less than this share of the picture are merged.
    #[arg(long, default_value_t = 0.02)]
    same: f32,
    /// Longest piece of audio given to the speech model at once, in seconds.
    #[arg(long, default_value_t = 30)]
    chunk: usize,
    /// Also write debug.json (per-frame motion, overlay boxes).
    #[arg(long)]
    debug: bool,
}

fn clock(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let out = cli.out.clone().unwrap_or_else(|| {
        let stem = cli.video.file_stem().unwrap_or_default().to_string_lossy();
        cli.video.with_file_name(format!("{stem}-slides"))
    });
    fs::create_dir_all(out.join("slides")).with_context(|| format!("creating {}", out.display()))?;
    let model_path = model::resolve(cli.model.as_deref())?;
    let started = Instant::now();

    // Picture and sound in parallel: the frame pass is bound by decoding on the
    // CPU, transcription by the GPU.
    let (picture, sound) = std::thread::scope(|s| {
        let sound = s.spawn(|| -> Result<(Vec<transcribe::Word>, f64)> {
            let pcm = media::audio_16k(&cli.video)?;
            let seconds = pcm.len() as f64 / media::AUDIO_RATE as f64;
            let t = Instant::now();
            let words = transcribe::words(&model_path, &pcm, cli.chunk)?;
            eprintln!("transcribed {} words in {:.0}s", words.len(), t.elapsed().as_secs_f64());
            Ok((words, seconds))
        });
        let picture = (|| -> Result<frames::Frames> {
            let t = Instant::now();
            let mut f = frames::Frames::new();
            media::gray_frames(&cli.video, |_, frame| f.add(frame))?;
            eprintln!("read {} frames in {:.0}s", f.count, t.elapsed().as_secs_f64());
            Ok(f)
        })();
        (picture, sound.join().expect("the transcription thread panicked"))
    });
    let frames = picture?;
    let (words, audio_seconds) = sound?;
    let duration = audio_seconds.max(frames.count as f64 / media::FPS);

    let split = segments::split(&frames, &cli.mask, !cli.no_mask);
    report_segments(&split);
    let motion = holds::motion(&frames, &split);
    let params = holds::Params { change: cli.change, min_slide: cli.min_slide, same: cli.same };
    let parts = holds::find(&frames, &split, &motion, params, duration);
    let n_slides = parts.iter().filter(|p| p.kind == segments::Kind::Slides).count();
    eprintln!("found {n_slides} slides and {} speaker/video parts", parts.len() - n_slides);

    let images: Vec<PathBuf> = (1..=parts.len()).map(|k| out.join("slides").join(format!("slide_{k:03}.jpg"))).collect();
    for (p, img) in parts.iter().zip(&images) {
        media::save_frame(&cli.video, p.frame, img)?;
    }
    let ranges: Vec<(f64, f64)> = parts.iter().map(|p| (p.start, p.end)).collect();
    let spoken = transcribe::per_slide(&words, &ranges);

    let mut descriptions = vec![None; parts.len()];
    if let Some(url) = &cli.describe_url {
        let d = describe::Describer::new(url, &cli.describe_model);
        for (k, (p, img)) in parts.iter().zip(&images).enumerate() {
            if p.kind != segments::Kind::Slides {
                continue;
            }
            let mut boxes = split.segments[p.segment].boxes.clone();
            boxes.extend(cli.mask.iter().copied());
            let blank: Vec<[f32; 4]> = boxes.iter().map(|b| b.fractions()).collect();
            let t = Instant::now();
            descriptions[k] = Some(d.describe(img, &blank)?);
            eprintln!("described part {}/{} in {:.0}s", k + 1, parts.len(), t.elapsed().as_secs_f64());
        }
    }

    write_output(&out, &cli.video, &parts, &spoken, &descriptions)?;
    fs::write(out.join("words.json"), serde_json::to_string_pretty(&words)?)?;
    let parts_json: Vec<_> = parts
        .iter()
        .enumerate()
        .map(|(k, p)| {
            json!({"slide": k + 1, "kind": p.kind, "start": p.start, "end": p.end, "frame": p.frame,
                   "image": format!("slides/slide_{:03}.jpg", k + 1),
                   "transcript": spoken[k], "description": descriptions[k]})
        })
        .collect();
    fs::write(out.join("slides.json"), serde_json::to_string_pretty(&parts_json)?)?;
    if cli.debug {
        let dbg = json!({"segments": split.segments, "extra_masks": cli.mask, "fps": media::FPS, "motion": motion});
        fs::write(out.join("debug.json"), serde_json::to_string(&dbg)?)?;
    }
    eprintln!("wrote {} in {:.0}s", out.display(), started.elapsed().as_secs_f64());
    Ok(())
}

/// One line per distinct overlay box, with how much of the video it covers.
fn report_segments(split: &segments::Split) {
    let secs = |s: &segments::Segment| (s.last + 1 - s.first) as f64 / media::FPS;
    let video: f64 = split.segments.iter().filter(|s| s.kind == segments::Kind::Video).map(secs).sum::<f64>() + 0.0;
    eprintln!(
        "{} stretches between hard cuts; {:.0}s classed as speaker/video",
        split.segments.len(),
        video
    );
    let mut seen: Vec<(String, f64)> = Vec::new();
    for s in split.segments.iter().filter(|s| s.kind == segments::Kind::Slides) {
        for b in &s.boxes {
            let d = b.describe();
            match seen.iter_mut().find(|(k, _)| *k == d) {
                Some(e) => e.1 += secs(s),
                None => seen.push((d, secs(s))),
            }
        }
    }
    seen.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (d, t) in seen.iter().take(8) {
        eprintln!("masking overlay: {d} ({t:.0}s)");
    }
    if seen.len() > 8 {
        eprintln!("masking {} more overlay boxes", seen.len() - 8);
    }
}

fn write_output(
    out: &Path,
    video: &Path,
    parts: &[holds::Part],
    spoken: &[String],
    descriptions: &[Option<String>],
) -> Result<()> {
    let name = video.file_name().unwrap_or_default().to_string_lossy();
    let n_slides = parts.iter().filter(|p| p.kind == segments::Kind::Slides).count();
    let mut index = format!("# {name}\n\n{n_slides} slides, {} speaker/video parts\n\n", parts.len() - n_slides);
    let mut all = format!("# {name}\n");
    for (k, p) in parts.iter().enumerate() {
        let n = k + 1;
        let times = format!("{}–{}", clock(p.start), clock(p.end));
        let label = match p.kind {
            segments::Kind::Slides => format!("Slide {n}"),
            segments::Kind::Video => format!("Part {n}: speaker or video, no slide"),
        };
        let mut page = format!(
            "---\nslide: {n}\nkind: {}\ntime_start: {:.1}\ntime_end: {:.1}\n---\n\n# {label} · {times}\n\n![{label}](slides/slide_{n:03}.jpg)\n",
            serde_json::to_value(p.kind)?.as_str().unwrap_or(""),
            p.start,
            p.end
        );
        if let Some(d) = &descriptions[k] {
            write!(page, "\n## Description\n\n{d}\n")?;
        }
        write!(page, "\n## Transcript\n\n{}\n", spoken[k])?;
        fs::write(out.join(format!("slide_{n:03}.md")), &page)?;
        writeln!(index, "- [{label}](slide_{n:03}.md) · {times}")?;
        let body = page.split_once("---\n\n").map_or(page.as_str(), |(_, rest)| rest).replacen("# ", "## ", 1);
        write!(all, "\n---\n{body}")?;
    }
    fs::write(out.join("index.md"), index)?;
    fs::write(out.join("slides.md"), all)?;
    Ok(())
}
