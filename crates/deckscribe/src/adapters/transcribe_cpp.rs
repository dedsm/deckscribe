//! transcribe.cpp as the recognizer: a Parakeet model (or any model family it
//! reads) on the GPU, returning every word with its start and end.

use std::path::Path;

use anyhow::{Context, Result};
use deckscribe_core::Word;
use deckscribe_core::ports::Recognizer;
use transcribe_cpp::{Model, RunOptions, Session, TimestampKind};

pub struct TranscribeCpp {
    session: Session,
    pub description: String,
}

impl TranscribeCpp {
    pub fn load(model_path: &Path) -> Result<Self> {
        transcribe_cpp::init_backends_default().context("starting the speech engine")?;
        let model = Model::load(model_path).with_context(|| format!("loading the speech model {}", model_path.display()))?;
        let description = format!("{} on {}", model.arch(), model.backend());
        // The session keeps the model alive.
        let session = model.session()?;
        Ok(TranscribeCpp { session, description })
    }
}

impl Recognizer for TranscribeCpp {
    fn recognize(&mut self, pcm: &[f32]) -> Result<Vec<Word>> {
        let opts = RunOptions { timestamps: TimestampKind::Word, ..Default::default() };
        let result = self.session.run(pcm, &opts)?;
        Ok(result
            .words
            .into_iter()
            .map(|w| Word { start: w.t0_ms as f64 / 1000.0, end: w.t1_ms as f64 / 1000.0, text: w.text })
            .collect())
    }
}
