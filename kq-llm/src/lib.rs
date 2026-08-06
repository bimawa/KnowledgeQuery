use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn ask_stream(&self, prompt: &str, context: &[String], tx: mpsc::UnboundedSender<String>) -> Result<()>;
}

pub mod anthropic;
pub mod ollama;
pub mod openai;
