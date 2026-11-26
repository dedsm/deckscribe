//! Adapters: the outside world behind the core crate's ports.

pub mod bundle;
pub mod ffmpeg;
#[cfg(feature = "transcribe")]
pub mod model_store;
#[cfg(feature = "describe")]
pub mod openai_vision;
pub mod page;
#[cfg(feature = "transcribe")]
pub mod transcribe_cpp;
