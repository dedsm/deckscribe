//! Slides as holds: spans where the masked picture stays still. A pan, zoom,
//! cut or slide flip moves many blocks at once and ends a hold; a cursor or a
//! pen stroke moves a few and does not. Live stretches become live parts.

use serde::Serialize;

use super::stretches::{Layout, Stretches};
use crate::frames::{FPS, FrameFeatures, PIXEL_DELTA, TH, TW, instant, thumb_block};
use crate::settings::Settings;
use crate::time::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Found {
    Slide,
    Live,
}

/// A slide or live part as found in the picture, before words and images.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct FoundPart {
    pub kind: Found,
    /// Parts tile the recording: the motion before a hold belongs to the
    /// slide it leads to.
    pub span: Span,
    /// The instant the part's image is taken from.
    pub image_at: f64,
    /// Index of the stretch the image comes from, for its overlays.
    pub stretch: usize,
}

/// Per frame: share of blocks outside its stretch's mask that moved.
pub fn motion(features: &FrameFeatures, stretches: &Stretches) -> Vec<f32> {
    (0..features.count)
        .map(|i| features.motion(i, &stretches.at(i).mask))
        .collect()
}

/// Share of thumbnail pixels that differ between frames `a` and `b`, outside
/// both frames' widened masks.
fn difference(features: &FrameFeatures, stretches: &Stretches, a: usize, b: usize) -> f32 {
    let (ma, mb) = (&stretches.at(a).wide_mask, &stretches.at(b).wide_mask);
    let (ta, tb) = (features.thumb(a), features.thumb(b));
    let (mut open, mut changed) = (0u32, 0u32);
    for ty in 0..TH {
        for tx in 0..TW {
            let block = thumb_block(tx, ty);
            if ma[block] || mb[block] {
                continue;
            }
            open += 1;
            let i = ty * TW + tx;
            changed += ((ta[i] as i16 - tb[i] as i16).abs() > PIXEL_DELTA) as u32;
        }
    }
    changed as f32 / open.max(1) as f32
}

pub fn find(
    features: &FrameFeatures,
    stretches: &Stretches,
    motion: &[f32],
    s: &Settings,
    duration: f64,
) -> Vec<FoundPart> {
    let min_frames = (s.min_hold * FPS).round() as usize;
    let live = |i: usize| s.live_parts && stretches.at(i).layout == Layout::Live;
    // (first, last) frame of each still span long enough to be a slide.
    let mut holds: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for i in 1..=features.count {
        let ends = i == features.count
            || motion.get(i).is_some_and(|&m| m > s.hold_change)
            || live(i) != live(start);
        if ends {
            if !live(start) && i - start >= min_frames {
                holds.push((start, i - 1));
            }
            start = i;
        }
    }
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for h in holds {
        match merged.last_mut() {
            // Never across a live stretch: the merged slide would overlap it.
            Some(last)
                if !(last.1 + 1..h.0).any(live)
                    && difference(features, stretches, last.1, h.1) < s.same_slide =>
            {
                last.1 = h.1
            }
            _ => merged.push(h),
        }
    }
    let mut parts: Vec<(usize, usize, Found)> = merged
        .into_iter()
        .map(|(a, b)| (a, b, Found::Slide))
        .collect();
    if s.live_parts {
        for st in &stretches.list {
            if st.layout == Layout::Live && st.last + 1 - st.first >= min_frames {
                parts.push((st.first, st.last, Found::Live));
            }
        }
    }
    parts.sort_by_key(|p| p.0);
    if parts.is_empty() {
        parts.push((0, features.count.saturating_sub(1), Found::Slide));
    }
    let lead = (s.image_lead * FPS).round() as usize;
    let mut out = Vec::with_capacity(parts.len());
    for (k, &(first, last, kind)) in parts.iter().enumerate() {
        let start = if k == 0 {
            0.0
        } else {
            instant(parts[k - 1].1 + 1)
        };
        let end = if k + 1 == parts.len() {
            duration.max(instant(last + 1))
        } else {
            instant(last + 1)
        };
        // Analysis frames and seeking in the recording can disagree by a
        // fraction of a second; the very last still frame may come back as the
        // next slide.
        let image = match kind {
            Found::Live => (first + last) / 2,
            Found::Slide => last.saturating_sub(lead).max(first),
        };
        out.push(FoundPart {
            kind,
            span: Span::new(start, end),
            image_at: instant(image),
            stretch: stretches.of_frame[image],
        });
    }
    out
}
