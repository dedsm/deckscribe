//! The domain of deckscribe: a recorded talk in, a deck out. A deck is the
//! slides that were shown, what was said over each, and optionally a
//! description of each slide.
//!
//! This crate does no I/O. Decoding, speech recognition, image description and
//! writing files are reached through the traits in [`ports`]; the `deckscribe`
//! crate provides the adapters. See `docs/design.md` for the vocabulary.

pub mod alignment;
pub mod corrections;
pub mod deck;
pub mod description;
pub mod frames;
pub mod ports;
pub mod segmentation;
pub mod settings;
pub mod speech;
pub mod summary;
pub mod time;

pub use deck::{About, Deck, DeckPart, PartKind};
pub use description::SlideText;
pub use settings::Settings;
pub use speech::Word;
pub use time::Span;
