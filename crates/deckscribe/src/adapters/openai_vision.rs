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
    /// The server is llama.cpp's llama-server, which takes a per-request
    /// switch for reasoning. Other servers get no extra fields.
    llama_cpp: bool,
}

impl OpenAi {
    pub fn new(base_url: &str, model: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .build()
            .into();
        let base = base_url.trim_end_matches('/');
        // llama-server answers /props with its generation settings.
        let llama_cpp = agent
            .get(&format!("{base}/props"))
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_json::<Value>().ok())
            .is_some_and(|v| v.get("default_generation_settings").is_some());
        OpenAi {
            agent,
            url: format!("{base}/v1/chat/completions"),
            model: model.to_string(),
            llama_cpp,
        }
    }

    fn chat(&self, content: Value, max_tokens: u32, think: bool) -> Result<String> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "temperature": 0,
            "messages": [{"role": "user", "content": content}],
        });
        // llama.cpp reasons before answering by default: off for reading
        // slides, on where it helps.
        if self.llama_cpp {
            body["chat_template_kwargs"] = json!({"enable_thinking": think});
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
        self.chat(Value::Array(content), 700, false)
    }

    fn name(&self) -> String {
        let host = self.url.trim_end_matches("/v1/chat/completions");
        format!("{} at {host}", self.model)
    }
}

impl TextModel for OpenAi {
    fn complete(&self, prompt: &str, max_tokens: u32, think: bool) -> Result<String> {
        self.chat(json!(prompt), max_tokens, think)
    }
}
