use super::EmbeddingBackend;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OpenAIBackendBuilder {
    model: String,
    api_key: Option<String>,
    endpoint: String,
    timeout: Duration,
}

impl OpenAIBackendBuilder {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_key: None,
            endpoint: "https://api.openai.com/v1/embeddings".to_string(),
            timeout: Duration::from_secs(60),
        }
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    pub fn build(self) -> Result<OpenAIBackend> {
        let api_key = match self.api_key {
            Some(k) => k,
            None => std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?,
        };
        let client = Client::builder().timeout(self.timeout).build()?;
        Ok(OpenAIBackend {
            client,
            api_key,
            model: self.model,
            endpoint: self.endpoint,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpenAIBackend {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
}

impl OpenAIBackend {
    pub fn builder(model: impl Into<String>) -> OpenAIBackendBuilder {
        OpenAIBackendBuilder::new(model)
    }
}

#[async_trait]
impl EmbeddingBackend for OpenAIBackend {
    async fn embed_batch(&self, inputs: &[&str]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let req = EmbeddingRequest {
            model: &self.model,
            input: inputs.to_vec(),
        };
        let mut backoff = Duration::from_millis(500);
        let max_backoff = Duration::from_secs(10);
        let mut last_err: Option<anyhow::Error> = None;
        for _attempt in 0..5 {
            let res = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .json(&req)
                .send()
                .await;
            match res {
                Ok(rsp) => {
                    if rsp.status().is_success() {
                        let body: EmbeddingResponse = rsp.json().await?;
                        let mut out = vec![Vec::new(); body.data.len()];
                        for d in body.data {
                            out[d.index] = d.embedding;
                        }
                        return Ok(out);
                    } else if rsp.status().as_u16() == 429 || rsp.status().is_server_error() {
                        last_err = Some(anyhow!("HTTP {}", rsp.status()));
                    } else {
                        let status = rsp.status();
                        let txt = rsp.text().await.unwrap_or_default();
                        return Err(anyhow!("embedding error {}: {}", status, txt));
                    }
                }
                Err(e) => {
                    last_err = Some(e.into());
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
        }
        if let Some(err) = last_err {
            Err(err)
        } else {
            Err(anyhow!("embedding request failed after retries"))
        }
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    model: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    index: usize,
    embedding: Vec<f32>,
}
