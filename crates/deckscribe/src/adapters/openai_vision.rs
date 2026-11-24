//! Any OpenAI-compatible chat server (llama.cpp's llama-server, Ollama, LM
//! Studio, hosted APIs) as the describer, and as the text model for the
//! optional summary.

use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use deckscribe_core::ports::{Describer, TextModel};
use serde_json::{Value, json};

pub struct OpenAi {
    agent: ureq::Agent,
    url: String,
    model: String,
    /// Ask llama.cpp to answer directly instead of reasoning first. Other
    /// servers may reject the extra field, so it is opt-in.
    no_think: bool,
}

impl OpenAi {
    pub fn new(base_url: &str, model: &str, no_think: bool) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .build()
            .into();
        OpenAi {
            agent,
            url: format!("{}/v1/chat/completions", base_url.trim_end_matches('/')),
            model: model.to_string(),
            no_think,
        }
    }

    fn chat(&self, content: Value, max_tokens: u32) -> Result<String> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "temperature": 0,
            "messages": [{"role": "user", "content": content}],
        });
        if self.no_think {
            body["chat_template_kwargs"] = json!({"enable_thinking": false});
        }
        let reply: Value = self
            .agent
            .post(&self.url)
            .send_json(&body)
            .with_context(|| format!("asking {}", self.url))?
            .body_mut()
            .with_config()
            .limit(16 << 20)
            .read_json()?;
        let text = reply["choices"][0]["message"]["content"]
            .as_str()
            .context("the model's reply had no text")?;
        Ok(text.trim().to_string())
    }
}

impl Describer for OpenAi {
    fn ask(&self, images: &[&[u8]], prompt: &str) -> Result<String> {
        let mut content: Vec<Value> = images
            .iter()
            .map(|jpeg| {
                let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
                json!({"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{b64}")}})
            })
            .collect();
        content.push(json!({"type": "text", "text": prompt}));
        self.chat(Value::Array(content), 700)
    }

    fn name(&self) -> String {
        let host = self.url.trim_end_matches("/v1/chat/completions");
        format!("{} at {host}", self.model)
    }
}

impl TextModel for OpenAi {
    fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.chat(json!(prompt), max_tokens)
    }
}
