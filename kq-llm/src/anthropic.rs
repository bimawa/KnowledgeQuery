use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::LlmProvider;

pub struct AnthropicProvider {
    client: Client,
    model: String,
    api_key: String,
}

#[derive(Deserialize)]
struct ContentBlockDelta {
    delta: Option<DeltaContent>,
}

#[derive(Deserialize)]
struct DeltaContent {
    text: Option<String>,
}

impl AnthropicProvider {
    pub fn new(model: String, api_key: Option<String>) -> Result<Self> {
        let key = api_key
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .context("Anthropic API key not provided and ANTHROPIC_API_KEY not set")?;

        Ok(Self { client: Client::new(), model, api_key: key })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn ask_stream(&self, prompt: &str, context: &[String], tx: mpsc::UnboundedSender<String>) -> Result<()> {
        let mut content = String::new();

        if !context.is_empty() {
            content.push_str("Context:\n");
            for ctx in context {
                content.push_str(ctx);
                content.push('\n');
            }
            content.push('\n');
        }
        content.push_str(prompt);

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": [{"role": "user", "content": content}],
            "stream": true,
        });

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .context("Failed to connect to Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API returned {}: {}", status, text);
        }

        let mut buffer = String::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = tokio_stream::StreamExt::next(&mut stream).await {
            let chunk = chunk_result.context("Failed to read Anthropic stream")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.starts_with("event: message_stop") {
                    return Ok(());
                }

                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if let Ok(event) = serde_json::from_str::<ContentBlockDelta>(data)
                    && let Some(delta) = event.delta
                    && let Some(text) = delta.text
                    && tx.send(text).is_err()
                {
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}
