//! Frames and what is kept of them. Each frame is seen once, in order, and
//! reduced to:
//! - a change map against the previous frame (share of changed pixels in each
//!   10x10 block), for cuts and holds;
//! - a 96x60 thumbnail, for overlays per stretch and for comparing holds.

/// Analysis frame size and rate. Frames are squashed to this size whatever the
/// recording's aspect ratio.
pub const FW: usize = 480;
pub const FH: usize = 300;
pub const FPS: f64 = 5.0;

pub const BLOCK: usize = 10;
/// Block grid: 48 x 30.
pub const GW: usize = FW / BLOCK;
pub const GH: usize = FH / BLOCK;
/// Thumbnail size: one pixel per 5x5 frame pixels, so two per block each way.
pub const TW: usize = 96;
pub const TH: usize = 60;

/// A pixel changed if its gray level moved by more than this.
pub const PIXEL_DELTA: i16 = 12;
/// A block moved if more than this share of its pixels changed.
pub const BLOCK_MOVED: f32 = 0.03;

/// Everything kept from the frames of one recording.
#[derive(Default)]
pub struct FrameFeatures {
    pub count: usize,
    /// `GW*GH` bytes per frame, changed share 0..=255; frame 0 is all zeros.
    change: Vec<u8>,
    /// `TW*TH` bytes per frame.
    thumbs: Vec<u8>,
    prev: Vec<u8>,
}

/// Share of changed pixels per block between two frames, 0..=255.
fn change_map(a: &[u8], b: &[u8], out: &mut [u8]) {
    let mut counts = [0u16; GW * GH];
    for y in 0..FH {
        let row = y * FW;
        let brow = (y / BLOCK) * GW;
        for x in 0..FW {
            if (a[row + x] as i16 - b[row + x] as i16).abs() > PIXEL_DELTA {
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

/// The block a thumbnail pixel falls in.
pub fn thumb_block(tx: usize, ty: usize) -> usize {
    (ty / 2) * GW + tx / 2
}

impl FrameFeatures {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the next frame: `FW*FH` gray bytes.
    pub fn push(&mut self, frame: &[u8]) {
        assert_eq!(frame.len(), FW * FH, "frames are {FW}x{FH} gray");
        let start = self.change.len();
        self.change.resize(start + GW * GH, 0);
        if !self.prev.is_empty() {
            change_map(&self.prev, frame, &mut self.change[start..]);
        }
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

    pub fn change(&self, i: usize) -> &[u8] {
        &self.change[i * GW * GH..(i + 1) * GW * GH]
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
        let n = self.change(i).iter().zip(mask).filter(|(s, m)| !**m && moved(**s)).count();
        n as f32 / open
    }

    pub fn seconds(&self) -> f64 {
        self.count as f64 / FPS
    }
}

pub fn instant(frame: usize) -> f64 {
    frame as f64 / FPS
}
