use async_trait::async_trait;
use anyhow::Result;
use tokio::sync::mpsc;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn ask_stream(&self, prompt: &str, context: &[String], tx: mpsc::UnboundedSender<String>) -> Result<()>;
}

pub mod ollama;
pub mod openai;
pub mod anthropic;
