//! The video split into stretches between hard cuts, each with its own
//! overlay mask. One mask for the whole video fails when the layout changes:
//! a speaker inset that grows into a side panel, camera shots between slides.

use serde::Serialize;

use crate::frames::{Frames, GH, GW};
use crate::overlays::{self, BlockBox};

/// A frame pair where more than this share of the whole picture moved is a
/// hard cut (camera switch, layout change, full slide change, fast pan).
const CUT: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Slides, possibly with overlays.
    Slides,
    /// Most of the picture keeps changing: camera, demo, video clip.
    Video,
}

#[derive(Clone, Debug, Serialize)]
pub struct Segment {
    pub first: usize,
    pub last: usize,
    pub kind: Kind,
    pub boxes: Vec<BlockBox>,
    /// Whether the boxes were measured here (false: borrowed from neighbours,
    /// the stretch being too short to measure).
    pub measured: bool,
    #[serde(skip)]
    pub mask: Vec<bool>,
    /// The mask grown by `WIDEN` blocks, for deciding whether two holds show
    /// the same slide: an overlay's quiet edges (the dark frame around a
    /// speaker inset) change too when its camera switches.
    #[serde(skip)]
    pub wide: Vec<bool>,
}

const WIDEN: usize = 4;

pub struct Split {
    pub segments: Vec<Segment>,
    /// Per frame: index of its segment.
    pub of_frame: Vec<usize>,
}

/// `extra`: boxes given on the command line, masked everywhere.
/// `find_overlays`: false for --no-mask.
pub fn split(frames: &Frames, extra: &[BlockBox], find_overlays: bool) -> Split {
    let open = vec![false; GW * GH];
    let mut starts = vec![0];
    let mut in_cut = false;
    for i in 1..frames.count {
        let cut = frames.motion(i, &open) > CUT;
        if cut && !in_cut {
            starts.push(i);
        }
        in_cut = cut;
    }
    let mut segments: Vec<Segment> = starts
        .iter()
        .enumerate()
        .map(|(k, &first)| {
            let last = starts.get(k + 1).map_or(frames.count.saturating_sub(1), |&n| n - 1);
            let (boxes, measured, kind) = if find_overlays {
                let found = overlays::find(frames, first, last);
                let kind = if found.ignored.is_empty() { Kind::Slides } else { Kind::Video };
                (found.boxes, found.pairs >= overlays::MIN_PAIRS, kind)
            } else {
                (Vec::new(), true, Kind::Slides)
            };
            Segment { first, last, kind, boxes, measured, mask: Vec::new(), wide: Vec::new() }
        })
        .collect();

    // Short stretches borrow the boxes of the nearest measured stretches on
    // both sides, and are video only if both neighbours are.
    let measured: Vec<usize> = (0..segments.len()).filter(|&k| segments[k].measured).collect();
    for k in 0..segments.len() {
        if segments[k].measured {
            continue;
        }
        let before = measured.iter().rev().find(|&&m| m < k).copied();
        let after = measured.iter().find(|&&m| m > k).copied();
        let mut boxes = Vec::new();
        for m in [before, after].into_iter().flatten() {
            for b in &segments[m].boxes {
                if !boxes.contains(b) {
                    boxes.push(*b);
                }
            }
        }
        let video = [before, after].into_iter().flatten().all(|m| segments[m].kind == Kind::Video)
            && (before.is_some() || after.is_some());
        segments[k].boxes = boxes;
        segments[k].kind = if video { Kind::Video } else { Kind::Slides };
    }
    for s in &mut segments {
        let mut all = s.boxes.clone();
        all.extend_from_slice(extra);
        s.mask = overlays::mask(&all);
        let grown: Vec<BlockBox> = all
            .iter()
            .map(|b| BlockBox {
                x0: b.x0.saturating_sub(WIDEN),
                y0: b.y0.saturating_sub(WIDEN),
                x1: (b.x1 + WIDEN).min(GW),
                y1: (b.y1 + WIDEN).min(GH),
            })
            .collect();
        s.wide = overlays::mask(&grown);
    }
    let mut of_frame = vec![0; frames.count];
    for (k, s) in segments.iter().enumerate() {
        for f in s.first..=s.last {
            of_frame[f] = k;
        }
    }
    Split { segments, of_frame }
}
