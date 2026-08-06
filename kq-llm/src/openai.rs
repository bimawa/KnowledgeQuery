use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::LlmProvider;

pub struct OpenAiProvider {
    client: Client,
    model: String,
    endpoint: String,
    api_key: String,
}

#[derive(Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    delta: Option<ChatDelta>,
}

#[derive(Deserialize)]
struct ChatDelta {
    content: Option<String>,
}

impl OpenAiProvider {
    pub fn new(model: String, endpoint: Option<String>, api_key: Option<String>) -> Result<Self> {
        let key = api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .context("OpenAI API key not provided and OPENAI_API_KEY not set")?;

        Ok(Self {
            client: Client::new(),
            model,
            endpoint: endpoint.unwrap_or_else(|| "https://api.openai.com".to_string()),
            api_key: key,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn ask_stream(&self, prompt: &str, context: &[String], tx: mpsc::UnboundedSender<String>) -> Result<()> {
        let mut messages = Vec::new();

        if !context.is_empty() {
            let mut context_str = String::from("Context:\n");
            for ctx in context {
                context_str.push_str(ctx);
                context_str.push('\n');
            }
            messages.push(serde_json::json!({"role": "system", "content": context_str}));
        }

        messages.push(serde_json::json!({"role": "user", "content": prompt}));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        let url = format!("{}/v1/chat/completions", self.endpoint);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .context("Failed to connect to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API returned {}: {}", status, text);
        }

        let mut buffer = String::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = tokio_stream::StreamExt::next(&mut stream).await {
            let chunk = chunk_result.context("Failed to read OpenAI stream")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    return Ok(());
                }

                if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) {
                    if let Some(choice) = chunk.choices.into_iter().next() {
                        if let Some(delta) = choice.delta {
                            if let Some(content) = delta.content {
                                if tx.send(content).is_err() {
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
