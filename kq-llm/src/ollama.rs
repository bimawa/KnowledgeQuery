use std::io::{BufRead, BufReader};

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::LlmProvider;

pub struct OllamaProvider {
    client: Client,
    model: String,
    endpoint: String,
}

#[derive(Deserialize)]
struct OllamaChunk {
    response: Option<String>,
    done: bool,
}

impl OllamaProvider {
    pub fn new(model: String, endpoint: Option<String>) -> Self {
        Self {
            client: Client::new(),
            model,
            endpoint: endpoint.unwrap_or_else(|| "http://localhost:11434".to_string()),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn ask_stream(&self, prompt: &str, context: &[String], tx: mpsc::UnboundedSender<String>) -> Result<()> {
        let mut full_prompt = String::new();
        if !context.is_empty() {
            full_prompt.push_str("Context:\n");
            for ctx in context {
                full_prompt.push_str(ctx);
                full_prompt.push('\n');
            }
            full_prompt.push('\n');
        }
        full_prompt.push_str(prompt);

        let body = serde_json::json!({
            "model": self.model,
            "prompt": full_prompt,
            "stream": true,
        });

        let url = format!("{}/api/generate", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to connect to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned {}: {}", status, text);
        }

        let bytes = response.bytes().await.context("Failed to read Ollama response")?;
        let reader = BufReader::new(bytes.as_ref());

        for line in reader.lines() {
            let line = line.context("Failed to read line from Ollama response")?;
            if line.is_empty() {
                continue;
            }
            let chunk: OllamaChunk = serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse Ollama chunk: {}", line))?;
            if let Some(text) = chunk.response {
                if tx.send(text).is_err() {
                    break;
                }
            }
            if chunk.done {
                break;
            }
        }

        Ok(())
    }
}
