use anyhow::{anyhow, Result};
use async_trait::async_trait;

pub mod openai;

#[async_trait]
pub trait EmbeddingBackend: Send + Sync {
    async fn embed_batch(&self, inputs: &[&str]) -> Result<Vec<Vec<f32>>>;

    async fn embed(&self, input: &str) -> Result<Vec<f32>> {
        let mut out = self
            .embed_batch(std::slice::from_ref(&input))
            .await?
            .into_iter();
        out.next()
            .ok_or_else(|| anyhow!("empty embedding response"))
    }
}
