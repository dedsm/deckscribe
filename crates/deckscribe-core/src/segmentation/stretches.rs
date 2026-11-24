//! The recording split at cuts into stretches, each with its own layout and
//! mask. One mask for the whole recording fails when the layout changes: a
//! speaker inset that grows into a side panel, camera shots between slides.

use serde::Serialize;

use super::overlays::{self, BlockBox};
use crate::frames::{FrameFeatures, GH, GW};
use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// A still picture, possibly with overlays.
    Slides,
    /// Most of the picture keeps changing: camera, demo, video clip.
    Live,
}

#[derive(Clone, Debug, Serialize)]
pub struct Stretch {
    pub first: usize,
    pub last: usize,
    pub layout: Layout,
    pub overlays: Vec<BlockBox>,
    /// False when the stretch was too short to measure and borrowed its
    /// neighbours' overlays.
    pub measured: bool,
    #[serde(skip)]
    pub mask: Vec<bool>,
    /// The mask grown by `Settings::mask_widen`, for comparing holds.
    #[serde(skip)]
    pub wide_mask: Vec<bool>,
}

pub struct Stretches {
    pub list: Vec<Stretch>,
    /// Per frame: index of its stretch.
    pub of_frame: Vec<usize>,
}

impl Stretches {
    pub fn at(&self, frame: usize) -> &Stretch {
        &self.list[self.of_frame[frame]]
    }

    /// Distinct overlay boxes of slide stretches, with seconds covered.
    pub fn overlay_summary(&self) -> Vec<(BlockBox, f64)> {
        let mut out: Vec<(BlockBox, f64)> = Vec::new();
        for s in self.list.iter().filter(|s| s.layout == Layout::Slides) {
            let secs = (s.last + 1 - s.first) as f64 / crate::frames::FPS;
            for b in &s.overlays {
                match out.iter_mut().find(|(k, _)| k == b) {
                    Some(e) => e.1 += secs,
                    None => out.push((*b, secs)),
                }
            }
        }
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }

    pub fn live_seconds(&self) -> f64 {
        self.list
            .iter()
            .filter(|s| s.layout == Layout::Live)
            .map(|s| (s.last + 1 - s.first) as f64 / crate::frames::FPS)
            .sum::<f64>()
            + 0.0
    }
}

pub fn split(features: &FrameFeatures, s: &Settings) -> Stretches {
    let open = vec![false; GW * GH];
    let mut starts = vec![0];
    if s.split_at_cuts {
        // A cut starts a stretch. A run of several cut frames (a long pan,
        // camera noise) is a stretch of its own: the still picture after it
        // starts another.
        let mut run_start: Option<usize> = None;
        for i in 1..features.count {
            let cut = features.motion(i, &open) > s.cut;
            match (cut, run_start) {
                (true, None) => {
                    starts.push(i);
                    run_start = Some(i);
                }
                (false, Some(r)) => {
                    if i - r >= 2 {
                        starts.push(i);
                    }
                    run_start = None;
                }
                _ => {}
            }
        }
    }
    let last_frame = features.count.saturating_sub(1);
    let mut list: Vec<Stretch> = starts
        .iter()
        .enumerate()
        .map(|(k, &first)| {
            let last = starts.get(k + 1).map_or(last_frame, |&n| n - 1);
            let (overlays, measured, layout) = if s.find_overlays {
                let found = overlays::find(features, first, last, s);
                let layout = if found.too_large.is_empty() {
                    Layout::Slides
                } else {
                    Layout::Live
                };
                (found.boxes, found.pairs >= s.overlay_min_pairs, layout)
            } else {
                (Vec::new(), true, Layout::Slides)
            };
            Stretch {
                first,
                last,
                layout,
                overlays,
                measured,
                mask: Vec::new(),
                wide_mask: Vec::new(),
            }
        })
        .collect();

    // Short stretches borrow the overlays of the nearest measured stretch on
    // each side, and are live only if both neighbours are.
    let measured: Vec<usize> = (0..list.len()).filter(|&k| list[k].measured).collect();
    for k in 0..list.len() {
        if list[k].measured {
            continue;
        }
        let before = measured.iter().rev().find(|&&m| m < k).copied();
        let after = measured.iter().find(|&&m| m > k).copied();
        let neighbours: Vec<usize> = [before, after].into_iter().flatten().collect();
        let mut boxes = Vec::new();
        for &m in &neighbours {
            for b in &list[m].overlays {
                if !boxes.contains(b) {
                    boxes.push(*b);
                }
            }
        }
        let live =
            !neighbours.is_empty() && neighbours.iter().all(|&m| list[m].layout == Layout::Live);
        list[k].overlays = boxes;
        list[k].layout = if live { Layout::Live } else { Layout::Slides };
    }
    for st in &mut list {
        let mut all = st.overlays.clone();
        all.extend_from_slice(&s.extra_masks);
        st.mask = overlays::mask(&all);
        let grown: Vec<BlockBox> = all.iter().map(|b| b.grown(s.mask_widen)).collect();
        st.wide_mask = overlays::mask(&grown);
    }
    let mut of_frame = vec![0; features.count];
    for (k, st) in list.iter().enumerate() {
        for f in st.first..=st.last {
            of_frame[f] = k;
        }
    }
    Stretches { list, of_frame }
}
