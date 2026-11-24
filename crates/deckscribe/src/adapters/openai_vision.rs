//! Any OpenAI-compatible chat server that accepts images (llama.cpp's
//! llama-server, Ollama, LM Studio, hosted APIs) as the describer.

use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use deckscribe_core::ports::Describer;
use serde_json::{Value, json};

pub struct OpenAiVision {
    agent: ureq::Agent,
    url: String,
    model: String,
    /// Ask llama.cpp to answer directly instead of reasoning first. Other
    /// servers may reject the extra field, so it is opt-in.
    no_think: bool,
}

impl OpenAiVision {
    pub fn new(base_url: &str, model: &str, no_think: bool) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(300)))
            .build()
            .into();
        OpenAiVision {
            agent,
            url: format!("{}/v1/chat/completions", base_url.trim_end_matches('/')),
            model: model.to_string(),
            no_think,
        }
    }
}

impl Describer for OpenAiVision {
    fn describe(&self, jpeg: &[u8], prompt: &str) -> Result<String> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
        let mut body = json!({
            "model": self.model,
            "max_tokens": 700,
            "temperature": 0,
            "messages": [{"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{b64}")}},
                {"type": "text", "text": prompt},
            ]}],
        });
        if self.no_think {
            body["chat_template_kwargs"] = json!({"enable_thinking": false});
        }
        let reply: Value = self
            .agent
            .post(&self.url)
            .send_json(&body)
            .with_context(|| format!("asking {} for a description", self.url))?
            .body_mut()
            .read_json()?;
        let text = reply["choices"][0]["message"]["content"]
            .as_str()
            .context("the description reply had no text")?;
        Ok(text.trim().to_string())
    }

    fn name(&self) -> String {
        format!("{} at {}", self.model, self.url)
    }
}
