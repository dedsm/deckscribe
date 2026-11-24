//! Regions that keep changing whatever the slide does: a webcam picture,
//! burned-in captions, a speaker panel. Found per stretch of video by how
//! often each block moved between frames 2 s apart.

use serde::Serialize;

use crate::frames::{Frames, GH, GW, PIXEL_DELTA, TH, TW};

/// A block belongs to an overlay if it moved in more than this share of pairs.
pub const OVERLAY_FREQ: f32 = 0.4;
/// Regions smaller than this many blocks are noise.
const MIN_BLOCKS: usize = 4;
/// A "region" covering more than this share of the frame is not an overlay
/// (a talking-head video, constant motion); it is reported and not masked.
const MAX_SHARE: f32 = 0.4;

/// A box in block units, end-exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BlockBox {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl BlockBox {
    pub fn describe(&self) -> String {
        let p = |v: usize, n: usize| (v * 100) / n;
        format!(
            "x {}-{}%, y {}-{}%",
            p(self.x0, GW),
            p(self.x1, GW),
            p(self.y0, GH),
            p(self.y1, GH)
        )
    }

    /// x, y, w, h as fractions of the frame.
    pub fn fractions(&self) -> [f32; 4] {
        let (gw, gh) = (GW as f32, GH as f32);
        [self.x0 as f32 / gw, self.y0 as f32 / gh, (self.x1 - self.x0) as f32 / gw, (self.y1 - self.y0) as f32 / gh]
    }

    /// Parses "x,y,w,h" in fractions of the frame (e.g. 0.78,0.66,0.22,0.34).
    pub fn parse(s: &str) -> Result<Self, String> {
        let v: Vec<f32> = s
            .split(',')
            .map(|p| p.trim().parse::<f32>().map_err(|e| format!("{s}: {e}")))
            .collect::<Result<_, _>>()?;
        let [x, y, w, h] = v[..] else {
            return Err(format!("{s}: expected x,y,w,h as fractions"));
        };
        let gx = |f: f32| ((f.clamp(0.0, 1.0) * GW as f32) as usize).min(GW);
        let gy = |f: f32| ((f.clamp(0.0, 1.0) * GH as f32) as usize).min(GH);
        Ok(BlockBox { x0: gx(x), y0: gy(y), x1: gx(x + w).max(gx(x) + 1), y1: gy(y + h).max(gy(y) + 1) })
    }
}

pub struct Found {
    pub boxes: Vec<BlockBox>,
    pub ignored: Vec<BlockBox>,
    /// Sample pairs measured; under `MIN_PAIRS` nothing is reported.
    pub pairs: usize,
}

/// Frames compared for overlays are this many frames apart (2 s).
const SAMPLE_STEP: usize = 10;
/// Fewer sample pairs than this (a stretch under about 8 s) say nothing.
pub const MIN_PAIRS: usize = 4;

/// Overlays within frames `first..=last`, from thumbnails 2 s apart: a block
/// (2x2 thumbnail pixels) moved if any of its pixels changed.
pub fn find(frames: &Frames, first: usize, last: usize) -> Found {
    let samples: Vec<usize> = (first..=last).step_by(SAMPLE_STEP).collect();
    let pairs = samples.len().saturating_sub(1);
    let mut found = Found { boxes: Vec::new(), ignored: Vec::new(), pairs };
    if pairs < MIN_PAIRS {
        return found;
    }
    let mut counts = vec![0u32; GW * GH];
    for w in samples.windows(2) {
        let (a, b) = (frames.thumb(w[0]), frames.thumb(w[1]));
        let mut moved = vec![false; GW * GH];
        for ty in 0..TH {
            for tx in 0..TW {
                let i = ty * TW + tx;
                if (a[i] as i16 - b[i] as i16).abs() > PIXEL_DELTA {
                    moved[(ty / 2) * GW + tx / 2] = true;
                }
            }
        }
        for (c, m) in counts.iter_mut().zip(moved) {
            *c += m as u32;
        }
    }
    let hot: Vec<bool> = counts.iter().map(|&c| c as f32 / pairs as f32 > OVERLAY_FREQ).collect();
    let mut seen = vec![false; GW * GH];
    for start in 0..GW * GH {
        if !hot[start] || seen[start] {
            continue;
        }
        let (mut stack, mut n) = (vec![start], 0);
        seen[start] = true;
        let mut b = BlockBox { x0: GW, y0: GH, x1: 0, y1: 0 };
        while let Some(i) = stack.pop() {
            n += 1;
            let (x, y) = (i % GW, i / GW);
            b = BlockBox { x0: b.x0.min(x), y0: b.y0.min(y), x1: b.x1.max(x + 1), y1: b.y1.max(y + 1) };
            let mut push = |j: usize| {
                if hot[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 { push(i - 1) }
            if x + 1 < GW { push(i + 1) }
            if y > 0 { push(i - GW) }
            if y + 1 < GH { push(i + GW) }
        }
        if n < MIN_BLOCKS {
            continue;
        }
        // One block of padding: overlay edges blur into their surroundings.
        let b = BlockBox { x0: b.x0.saturating_sub(1), y0: b.y0.saturating_sub(1), x1: (b.x1 + 1).min(GW), y1: (b.y1 + 1).min(GH) };
        let share = ((b.x1 - b.x0) * (b.y1 - b.y0)) as f32 / (GW * GH) as f32;
        if share > MAX_SHARE { found.ignored.push(b) } else { found.boxes.push(b) }
    }
    found
}

/// Per block: true if it is inside any box.
pub fn mask(boxes: &[BlockBox]) -> Vec<bool> {
    let mut m = vec![false; GW * GH];
    for b in boxes {
        for y in b.y0..b.y1 {
            for x in b.x0..b.x1 {
                m[y * GW + x] = true;
            }
        }
    }
    m
}
