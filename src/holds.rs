//! Slides as "holds": stretches where the masked picture stays still. A pan,
//! zoom, cut or slide flip moves many blocks at once and ends a hold; a cursor
//! or a pen stroke moves a few and does not. Stretches classed as video
//! (camera, demos) become their own parts instead of slides.

use serde::Serialize;

use crate::frames::{Frames, GW, TH, TW};
use crate::media::FPS;
use crate::segments::{Kind, Split};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// A frame pair is a change if more than this share of unmasked blocks moved.
    pub change: f32,
    /// Shortest hold that counts as a slide (and shortest video part), in seconds.
    pub min_slide: f64,
    /// Neighbouring holds whose pictures differ in less than this share of
    /// unmasked thumbnail pixels are one slide (something brief passed over it).
    pub same: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Part {
    pub kind: Kind,
    /// Start and end of the stretch this part owns, seconds. Parts tile the
    /// whole video: the motion before a hold belongs to the slide it leads to.
    pub start: f64,
    pub end: f64,
    /// The frame used as the image: 1 s before a slide's hold ends, a video
    /// part's middle frame.
    pub frame: f64,
    /// Index of the segment the image comes from (for its overlay mask).
    #[serde(skip)]
    pub segment: usize,
}

/// Per frame: share of blocks outside its segment's mask that moved.
pub fn motion(frames: &Frames, split: &Split) -> Vec<f32> {
    (0..frames.count).map(|i| frames.motion(i, &split.segments[split.of_frame[i]].mask)).collect()
}

fn thumb_difference(frames: &Frames, split: &Split, a: usize, b: usize) -> f32 {
    let (ma, mb) = (&split.segments[split.of_frame[a]].wide, &split.segments[split.of_frame[b]].wide);
    let (ta, tb) = (frames.thumb(a), frames.thumb(b));
    let (mut open, mut changed) = (0u32, 0u32);
    for ty in 0..TH {
        for tx in 0..TW {
            // A thumbnail pixel is 5x5 analysis pixels: half a block each way.
            let block = (ty / 2) * GW + tx / 2;
            if ma[block] || mb[block] {
                continue;
            }
            open += 1;
            let i = ty * TW + tx;
            changed += ((ta[i] as i16 - tb[i] as i16).abs() > 12) as u32;
        }
    }
    changed as f32 / open.max(1) as f32
}

pub fn find(frames: &Frames, split: &Split, motion: &[f32], p: Params, duration: f64) -> Vec<Part> {
    let min_frames = (p.min_slide * FPS).round() as usize;
    let is_video = |i: usize| split.segments[split.of_frame[i]].kind == Kind::Video;
    // (first, last) frame of each still stretch long enough to be a slide.
    let mut holds: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for i in 1..=frames.count {
        let ends = i == frames.count || motion[i] > p.change || is_video(i) != is_video(start);
        if ends {
            if !is_video(start) && i - start >= min_frames {
                holds.push((start, i - 1));
            }
            start = i;
        }
    }
    // Merge neighbours that show the same picture.
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for h in holds {
        if std::env::var_os("DECKSCRIBE_TRACE").is_some() {
            if let Some(last) = merged.last() {
                eprintln!("trace: hold ending {:.1}s vs {:.1}s differ {:.3}", last.1 as f64 / FPS, h.1 as f64 / FPS, thumb_difference(frames, split, last.1, h.1));
            }
        }
        match merged.last_mut() {
            Some(last) if thumb_difference(frames, split, last.1, h.1) < p.same => last.1 = h.1,
            _ => merged.push(h),
        }
    }
    // (first, last, kind) of every part, in time order.
    let mut parts: Vec<(usize, usize, Kind)> = merged.into_iter().map(|(a, b)| (a, b, Kind::Slides)).collect();
    for s in &split.segments {
        if s.kind == Kind::Video && s.last + 1 - s.first >= min_frames {
            parts.push((s.first, s.last, Kind::Video));
        }
    }
    parts.sort_by_key(|p| p.0);
    if parts.is_empty() {
        parts.push((0, frames.count.saturating_sub(1), Kind::Slides));
    }
    let t = |f: usize| f as f64 / FPS;
    let mut out = Vec::with_capacity(parts.len());
    for (k, &(first, last, kind)) in parts.iter().enumerate() {
        let start = if k == 0 { 0.0 } else { t(parts[k - 1].1 + 1) };
        let end = parts.get(k + 1).map_or(duration, |_| t(last + 1));
        // A slide's image is taken 1 s before its hold ends: the analysis
        // frames and ffmpeg's seek can disagree by a fraction of a second, and
        // the very last still frame may come back as the next slide.
        let image = if kind == Kind::Video { (first + last) / 2 } else { last.saturating_sub(FPS as usize).max(first) };
        out.push(Part { kind, start, end, frame: t(image), segment: split.of_frame[image] });
    }
    out
}
