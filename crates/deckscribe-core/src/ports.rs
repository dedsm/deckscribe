//! What the domain needs from the outside world. Adapters in the `deckscribe`
//! crate implement these; tests use in-memory ones.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::deck::Deck;
use crate::speech::Word;

/// The recording's frames, in order: `frames::FW * frames::FH` gray bytes
/// each, `frames::FPS` per second. Returns how many were produced.
pub trait FrameSource: Sync {
    fn frames(&self, each: &mut dyn FnMut(&[u8])) -> Result<usize>;
}

/// The recording's sound as 16 kHz mono samples in [-1, 1].
pub trait AudioSource: Sync {
    fn audio(&self) -> Result<Vec<f32>>;
}

/// Full-resolution images from the recording.
pub trait StillSource: Sync {
    /// Saves the picture at `at` seconds as a JPEG at `out`.
    fn save_still(&self, at: f64, out: &Path) -> Result<()>;
    /// `image` re-encoded as JPEG at most `width` wide, with `blank` regions
    /// (x, y, w, h as fractions of the frame) painted black.
    fn prepared(&self, image: &Path, width: u32, blank: &[[f32; 4]]) -> Result<Vec<u8>>;
    /// One image of all `images` in order, `columns` across, each marked with
    /// its label.
    fn contact_sheet(&self, images: &[(PathBuf, String)], columns: usize, out: &Path)
    -> Result<()>;
}

/// Speech to words, for one piece of 16 kHz audio. Times are from the
/// piece's start.
pub trait Recognizer {
    fn recognize(&mut self, pcm: &[f32]) -> Result<Vec<Word>>;
    /// The model and where it runs, for the deck's record.
    fn name(&self) -> String {
        String::new()
    }
}

/// The speech model as a local file, fetched first if needed.
pub trait ModelStore {
    fn speech_model(&self) -> Result<PathBuf>;
}

/// A vision model that answers a prompt about one or more JPEG images, given
/// in order.
pub trait Describer {
    fn ask(&self, images: &[&[u8]], prompt: &str) -> Result<String>;
    /// The model and server, for the deck's record.
    fn name(&self) -> String {
        String::new()
    }
}

/// A text model that answers a prompt, for the optional summary.
pub trait TextModel {
    fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String>;
}

/// Persists a deck in one format inside an output folder.
pub trait DeckWriter {
    fn write(&self, deck: &Deck, out: &Path) -> Result<()>;
}
