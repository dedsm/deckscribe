//! Optional slide descriptions from any OpenAI-compatible chat server that
//! accepts images (llama.cpp's llama-server, Ollama, LM Studio, ...).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{Value, json};

use crate::media;

const PROMPT: &str = "This is a frame from a recorded presentation. \
Ignore any webcam or speaker video, burned-in captions, application toolbars and menus, and zoom indicators. \
Describe only the slide content, in this format:\n\
TITLE: the main heading, transcribed exactly (or 'None').\n\
TEXT: every other visible text, in reading order, one item per line.\n\
VISUALS: diagrams, icons, arrows and how they connect, in one or two sentences.\n\
Do not guess at anything you cannot read.";

pub struct Describer {
    agent: ureq::Agent,
    url: String,
    model: String,
}

impl Describer {
    pub fn new(base_url: &str, model: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into();
        Describer {
            agent,
            url: format!("{}/v1/chat/completions", base_url.trim_end_matches('/')),
            model: model.to_string(),
        }
    }

    /// `blank`: overlay regions to paint out first, so the model cannot read
    /// captions or describe the speaker as slide content.
    pub fn describe(&self, image: &Path, blank: &[[f32; 4]]) -> Result<String> {
        let jpeg = media::jpeg_at_width(image, 1280, blank)?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
        let body = json!({
            "model": self.model,
            "max_tokens": 700,
            "temperature": 0,
            // llama.cpp: answer directly instead of reasoning first.
            "chat_template_kwargs": {"enable_thinking": false},
            "messages": [{"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{b64}")}},
                {"type": "text", "text": PROMPT},
            ]}],
        });
        let reply: Value = self
            .agent
            .post(&self.url)
            .send_json(&body)
            .with_context(|| format!("asking {} to describe {}", self.url, image.display()))?
            .body_mut()
            .read_json()?;
        let text = reply["choices"][0]["message"]["content"]
            .as_str()
            .context("the description reply had no text")?;
        Ok(text.trim().to_string())
    }
}
