//! One pass over the analysis frames collects everything the picture steps
//! need, so the video is decoded once:
//! - for every pair of consecutive frames (0.2 s apart), the share of changed
//!   pixels in each 10x10 block (slide holds, hard cuts);
//! - a 96x60 thumbnail of every frame (overlays per stretch, comparing holds).

use crate::media::{FH, FW};

pub const BLOCK: usize = 10;
pub const GW: usize = FW / BLOCK; // 48
pub const GH: usize = FH / BLOCK; // 30
pub const TW: usize = 96;
pub const TH: usize = 60;
/// A pixel changed if its gray level moved by more than this.
pub const PIXEL_DELTA: i16 = 12;
/// A block moved if more than this share of its pixels changed.
pub const BLOCK_MOVED: f32 = 0.03;

pub struct Frames {
    pub count: usize,
    /// `GW*GH` bytes per frame: changed share vs the previous frame, 0..=255.
    /// Frame 0 is all zeros.
    pub pair_change: Vec<u8>,
    /// `TW*TH` bytes per frame.
    pub thumbs: Vec<u8>,
    prev: Vec<u8>,
}

/// Share of changed pixels per block between two frames, 0..=255.
fn block_change(a: &[u8], b: &[u8], out: &mut [u8]) {
    let mut counts = [0u16; GW * GH];
    for y in 0..FH {
        let row = y * FW;
        let brow = (y / BLOCK) * GW;
        for x in 0..FW {
            let d = a[row + x] as i16 - b[row + x] as i16;
            if d.abs() > PIXEL_DELTA {
                counts[brow + x / BLOCK] += 1;
            }
        }
    }
    let full = (BLOCK * BLOCK) as u32;
    for (o, c) in out.iter_mut().zip(counts) {
        *o = (c as u32 * 255 / full) as u8;
    }
}

pub fn moved(share: u8) -> bool {
    share as f32 / 255.0 > BLOCK_MOVED
}

impl Frames {
    pub fn new() -> Self {
        Frames { count: 0, pair_change: Vec::new(), thumbs: Vec::new(), prev: Vec::new() }
    }

    pub fn add(&mut self, frame: &[u8]) {
        let start = self.pair_change.len();
        self.pair_change.resize(start + GW * GH, 0);
        if !self.prev.is_empty() {
            block_change(&self.prev, frame, &mut self.pair_change[start..]);
        }
        // 96x60 thumbnail: mean of each 5x5 cell.
        for ty in 0..TH {
            for tx in 0..TW {
                let mut sum = 0u32;
                for y in ty * 5..ty * 5 + 5 {
                    for x in tx * 5..tx * 5 + 5 {
                        sum += frame[y * FW + x] as u32;
                    }
                }
                self.thumbs.push((sum / 25) as u8);
            }
        }
        self.prev.clear();
        self.prev.extend_from_slice(frame);
        self.count += 1;
    }

    pub fn pair(&self, i: usize) -> &[u8] {
        &self.pair_change[i * GW * GH..(i + 1) * GW * GH]
    }

    pub fn thumb(&self, i: usize) -> &[u8] {
        &self.thumbs[i * TW * TH..(i + 1) * TW * TH]
    }

    /// Share of blocks outside `mask` that moved since the previous frame.
    pub fn motion(&self, i: usize, mask: &[bool]) -> f32 {
        if i == 0 {
            return 0.0;
        }
        let open = mask.iter().filter(|m| !**m).count().max(1) as f32;
        let n = self.pair(i).iter().zip(mask).filter(|(s, m)| !**m && moved(**s)).count();
        n as f32 / open
    }
}
