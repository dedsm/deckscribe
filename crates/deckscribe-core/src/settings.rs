//! Every tuning number in one place. The CLI fills these from flags; the
//! benchmark harness can sweep them.

use serde::{Deserialize, Serialize};

use crate::segmentation::overlays::BlockBox;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    // Visual segmentation switches.
    /// Look for overlays (webcam, captions, speaker panel) and mask them.
    pub find_overlays: bool,
    /// Split the recording at cuts, so each stretch gets its own mask.
    /// Off: one stretch for the whole recording.
    pub split_at_cuts: bool,
    /// Keep live stretches (camera, demo, video) as their own parts.
    /// Off: they fold into the next slide.
    pub live_parts: bool,
    /// Boxes masked everywhere in addition to the ones found.
    pub extra_masks: Vec<BlockBox>,

    // Visual segmentation thresholds.
    /// A frame pair is a cut if more than this share of the whole picture moved.
    pub cut: f32,
    /// A block is part of an overlay if it moved in more than this share of
    /// comparisons within a stretch.
    pub overlay_frequency: f32,
    /// A region larger than this share of the frame is not an overlay; its
    /// stretch is live.
    pub overlay_max_share: f32,
    /// Seconds between the thumbnails compared when finding overlays.
    pub overlay_step: f64,
    /// Fewer comparisons than this (a stretch under about 8 s) borrow the
    /// neighbours' overlays.
    pub overlay_min_pairs: usize,
    /// A frame pair ends a hold if more than this share of the unmasked picture moved.
    pub hold_change: f32,
    /// Shortest hold that is a slide, and shortest live part, in seconds.
    pub min_hold: f64,
    /// Neighbouring holds that differ in less than this share of the picture
    /// are the same slide.
    pub same_slide: f32,
    /// Blocks added around each mask when comparing holds: an overlay's quiet
    /// edges change too when its camera switches.
    pub mask_widen: usize,
    /// A slide's image is taken this many seconds before its hold ends.
    pub image_lead: f64,

    // Speech.
    /// Longest audio piece given to the recognizer at once, in seconds.
    pub piece_seconds: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            find_overlays: true,
            split_at_cuts: true,
            live_parts: true,
            extra_masks: Vec::new(),
            cut: 0.6,
            overlay_frequency: 0.4,
            overlay_max_share: 0.4,
            overlay_step: 2.0,
            overlay_min_pairs: 4,
            hold_change: 0.05,
            min_hold: 6.0,
            same_slide: 0.02,
            mask_widen: 4,
            image_lead: 1.0,
            piece_seconds: 30,
        }
    }
}
