//! Overlays: regions that keep changing whatever the slide does, such as a
//! webcam, burned-in captions or a speaker panel. Found per stretch by how
//! often each block changed between thumbnails a few seconds apart.

use serde::{Deserialize, Serialize};

use crate::frames::{FrameFeatures, FPS, GH, GW, PIXEL_DELTA, TH, TW, thumb_block};
use crate::settings::Settings;

/// Regions smaller than this many blocks are noise.
const MIN_BLOCKS: usize = 4;

/// A box in block units, end-exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockBox {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl BlockBox {
    /// "x 77-100%, y 63-100%".
    pub fn describe(&self) -> String {
        let p = |v: usize, n: usize| (v * 100) / n;
        format!("x {}-{}%, y {}-{}%", p(self.x0, GW), p(self.x1, GW), p(self.y0, GH), p(self.y1, GH))
    }

    /// x, y, w, h as fractions of the frame.
    pub fn fractions(&self) -> [f32; 4] {
        let (gw, gh) = (GW as f32, GH as f32);
        [self.x0 as f32 / gw, self.y0 as f32 / gh, (self.x1 - self.x0) as f32 / gw, (self.y1 - self.y0) as f32 / gh]
    }

    /// Parses "x,y,w,h" in fractions of the frame, e.g. "0.78,0.66,0.22,0.34".
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

    pub fn grown(&self, by: usize) -> Self {
        BlockBox {
            x0: self.x0.saturating_sub(by),
            y0: self.y0.saturating_sub(by),
            x1: (self.x1 + by).min(GW),
            y1: (self.y1 + by).min(GH),
        }
    }

    fn share(&self) -> f32 {
        ((self.x1 - self.x0) * (self.y1 - self.y0)) as f32 / (GW * GH) as f32
    }
}

pub struct Found {
    pub boxes: Vec<BlockBox>,
    /// Regions too large to be overlays: the stretch is live.
    pub too_large: Vec<BlockBox>,
    /// Comparisons made; under `Settings::overlay_min_pairs` nothing is reported.
    pub pairs: usize,
}

/// Overlays within frames `first..=last`.
pub fn find(features: &FrameFeatures, first: usize, last: usize, s: &Settings) -> Found {
    let step = ((s.overlay_step * FPS).round() as usize).max(1);
    let samples: Vec<usize> = (first..=last).step_by(step).collect();
    let pairs = samples.len().saturating_sub(1);
    let mut found = Found { boxes: Vec::new(), too_large: Vec::new(), pairs };
    if pairs < s.overlay_min_pairs {
        return found;
    }
    let mut counts = vec![0u32; GW * GH];
    for w in samples.windows(2) {
        let (a, b) = (features.thumb(w[0]), features.thumb(w[1]));
        let mut moved = vec![false; GW * GH];
        for ty in 0..TH {
            for tx in 0..TW {
                let i = ty * TW + tx;
                if (a[i] as i16 - b[i] as i16).abs() > PIXEL_DELTA {
                    moved[thumb_block(tx, ty)] = true;
                }
            }
        }
        for (c, m) in counts.iter_mut().zip(moved) {
            *c += m as u32;
        }
    }
    let hot: Vec<bool> = counts.iter().map(|&c| c as f32 / pairs as f32 > s.overlay_frequency).collect();
    for b in regions(&hot) {
        // One block of padding: overlay edges blur into their surroundings.
        let b = b.grown(1);
        if b.share() > s.overlay_max_share { found.too_large.push(b) } else { found.boxes.push(b) }
    }
    found
}

/// Bounding boxes of the connected groups of hot blocks.
fn regions(hot: &[bool]) -> Vec<BlockBox> {
    let mut seen = vec![false; GW * GH];
    let mut out = Vec::new();
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
            if x > 0 {
                push(i - 1)
            }
            if x + 1 < GW {
                push(i + 1)
            }
            if y > 0 {
                push(i - GW)
            }
            if y + 1 < GH {
                push(i + GW)
            }
        }
        if n >= MIN_BLOCKS {
            out.push(b);
        }
    }
    out
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
