//! Visual segmentation, the core of deckscribe: frames → cuts → stretches →
//! overlays → holds → slides and live parts.

pub mod holds;
pub mod overlays;
pub mod stretches;

use crate::frames::FrameFeatures;
use crate::settings::Settings;

pub use holds::{Found, FoundPart};
pub use stretches::{Layout, Stretch, Stretches};

/// Everything visual segmentation produces, including what the debug output shows.
pub struct Segmentation {
    pub stretches: Stretches,
    pub motion: Vec<f32>,
    pub parts: Vec<FoundPart>,
}

/// Runs the whole visual segmentation over the features of a recording.
/// `duration` is the recording's length; the last part runs to it.
pub fn segment(features: &FrameFeatures, settings: &Settings, duration: f64) -> Segmentation {
    let stretches = stretches::split(features, settings);
    let motion = holds::motion(features, &stretches);
    let parts = holds::find(features, &stretches, &motion, settings, duration);
    Segmentation {
        stretches,
        motion,
        parts,
    }
}
