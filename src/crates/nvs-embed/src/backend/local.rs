use super::EmbeddingBackend;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::fs::File;
use std::sync::Arc;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert;
#[cfg(feature = "hf-hub")]
use hf_hub::api::sync::Api;
use tokenizers::parallelism::set_parallelism;
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

#[cfg(feature = "embed-model")]
const EMBED_TOKENIZER_BYTES: &[u8] = include_bytes!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/gte-small/tokenizer.json")
);
#[cfg(feature = "embed-model")]
const EMBED_CONFIG_BYTES: &[u8] = include_bytes!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/gte-small/config.json")
);
#[cfg(feature = "embed-model")]
const EMBED_WEIGHTS_BYTES: &[u8] = include_bytes!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/gte-small/model.safetensors")
);

#[cfg(feature = "embed-model")]
fn write_embedded_model_to_cache() -> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    use std::fs;
    let root = std::env::var_os("NVS_EMBED_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("nvs_embed_gte_small"));
    fs::create_dir_all(&root)?;
    let tok = root.join("tokenizer.json");
    let cfg = root.join("config.json");
    let wts = root.join("model.safetensors");
    if !tok.exists() { fs::write(&tok, EMBED_TOKENIZER_BYTES)?; }
    if !cfg.exists() { fs::write(&cfg, EMBED_CONFIG_BYTES)?; }
    if !wts.exists() { fs::write(&wts, EMBED_WEIGHTS_BYTES)?; }
    Ok((tok, wts, cfg))
}

#[derive(Debug, Clone)]
pub struct LocalGTEBackendBuilder {
    model_id: String,
    max_len: usize,
}

impl LocalGTEBackendBuilder {
    pub fn new() -> Self {
        Self {
            model_id: "thenlper/gte-small".to_string(),
            max_len: 256,
        }
    }
    pub fn model_id(mut self, id: impl Into<String>) -> Self {
        self.model_id = id.into();
        self
    }
    pub fn max_len(mut self, len: usize) -> Self {
        self.max_len = len;
        self
    }
    pub fn build(self) -> Result<LocalGTEBackend> {
        let device = Device::Cpu;
        // Enable parallel tokenization
        set_parallelism(true);
        // Allow offline override via env dir containing tokenizer.json, model.safetensors, config.json
        let (tokenizer_path, weights_path, config_path) = {
            // 0) Embedded model (highest priority when enabled)
            #[cfg(feature = "embed-model")]
            {
                if let Ok(paths) = write_embedded_model_to_cache() {
                    eprintln!("  · using embedded GTE-small model bytes (cached to temp dir)");
                    return Ok(LocalGTEBackend {
                        inner: {
                            let (tokenizer_path, weights_path, config_path) = paths.clone();
                            // Load tokenizer directly from file path for compatibility
                            let mut tokenizer = Tokenizer::from_file(tokenizer_path)
                                .map_err(|e| anyhow!("failed to load tokenizer.json: {}", e))?;
                            tokenizer.with_padding(Some(PaddingParams::default()));
                            if let Err(e) = tokenizer.with_truncation(Some(TruncationParams {
                                max_length: self.max_len,
                                ..Default::default()
                            })) {
                                return Err(anyhow!("failed to enable truncation: {}", e));
                            }
                            let config_file = File::open(&config_path)?;
                            let config: bert::Config = serde_json::from_reader(config_file)
                                .context("bad BERT config.json")?;
                            let vb = unsafe {
                                VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)?
                            };
                            let model = bert::BertModel::load(vb, &config)?;
                            Arc::new(Inner { tokenizer, model, device })
                        },
                        max_len: self.max_len,
                    });
                }
            }
            // 1) Env override
            if let Ok(dir) = std::env::var("NVS_LOCAL_EMBED_MODEL_DIR") {
                let p = std::path::PathBuf::from(dir);
                if let Some(paths) = find_model_files_in(&p) {
                    eprintln!("  · using local model dir: {}", p.display());
                    paths
                } else {
                    anyhow::bail!(
                        "NVS_LOCAL_EMBED_MODEL_DIR set but required files not found under {} (need tokenizer.json, config.json, model.safetensors)",
                        p.display()
                    )
                }
            } else {
                // 2) Well-known repo locations
                let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let candidates = vec![
                    manifest_dir.join("../../models/gte-small"), // src/models/gte-small
                    std::path::PathBuf::from("models/gte-small"),
                    std::path::PathBuf::from("src/models/gte-small"),
                    std::path::PathBuf::from("../models/gte-small"),
                ];
                if let Some((dir, paths)) = candidates
                    .into_iter()
                    .find_map(|d| find_model_files_in(&d).map(|p| (d, p)))
                {
                    eprintln!("  · found local model files in: {}", dir.display());
                    paths
                } else {
                    // 3) Fetch/cached files from HF Hub if enabled
                    #[cfg(feature = "hf-hub")]
                    {
                        eprintln!(
                            "  · local model files not found; falling back to HF Hub fetch for {}",
                            self.model_id
                        );
                        let api = Api::new()?;
                        let repo = api.model(self.model_id.clone());
                        let tok = repo
                            .get("tokenizer.json")
                            .with_context(|| format!("missing tokenizer.json in {}", self.model_id))?;
                        let wts = repo
                            .get("model.safetensors")
                            .with_context(|| format!("missing model.safetensors in {}", self.model_id))?;
                        let cfg = repo
                            .get("config.json")
                            .with_context(|| format!("missing config.json in {}", self.model_id))?;
                        (tok, wts, cfg)
                    }
                    #[cfg(not(feature = "hf-hub"))]
                    {
                        anyhow::bail!(
                            "local model files not found and HF Hub fetch is disabled. Set NVS_LOCAL_EMBED_MODEL_DIR or place tokenizer.json, config.json, model.safetensors under src/models/gte-small, or enable feature 'hub-fetch'."
                        );
                    }
                }
            }
        };
        // Load tokenizer
        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow!("failed to load tokenizer.json: {}", e))?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        if let Err(e) = tokenizer.with_truncation(Some(TruncationParams {
            max_length: self.max_len,
            ..Default::default()
        })) {
            return Err(anyhow!("failed to enable truncation: {}", e));
        }

        // Load config & weights
        let config_file = File::open(&config_path)?;
        let config: bert::Config = serde_json::from_reader(config_file).context("bad BERT config.json")?;
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)? };
        let model = bert::BertModel::load(vb, &config)?;

        Ok(LocalGTEBackend {
            inner: Arc::new(Inner { tokenizer, model, device }),
            max_len: self.max_len,
        })
    }
}

struct Inner {
    tokenizer: Tokenizer,
    model: bert::BertModel,
    device: Device,
}

#[derive(Clone)]
pub struct LocalGTEBackend {
    inner: Arc<Inner>,
    max_len: usize,
}

#[async_trait]
impl EmbeddingBackend for LocalGTEBackend {
    async fn embed_batch(&self, inputs: &[&str]) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        // Run heavy CPU work off the async executor.
        let inner = self.inner.clone();
        let max_len = self.max_len;
        let texts: Vec<String> = inputs.iter().map(|s| s.to_string()).collect();
        tokio::task::spawn_blocking(move || embed_batch_cpu(&inner, &texts, max_len))
            .await
            .unwrap_or_else(|e| Err(anyhow::anyhow!("join error: {}", e)))
    }
}

fn embed_batch_cpu(inner: &Inner, texts: &[String], _max_len: usize) -> Result<Vec<Vec<f32>>> {
    // Tokenize
    let encodings = inner
        .tokenizer
        .encode_batch(texts.to_vec(), true)
        .map_err(|e| anyhow!("tokenize batch: {}", e))?;
    let max_len = encodings.iter().map(|e| e.len()).max().unwrap_or(0);
    let bs = encodings.len();
    // Prepare input_ids and attention_mask as i64 tensors
    let mut ids: Vec<i64> = Vec::with_capacity(bs * max_len);
    let mut mask: Vec<i64> = Vec::with_capacity(bs * max_len);
    for enc in &encodings {
        let pad = max_len - enc.len();
        ids.extend(enc.get_ids().iter().map(|&id| id as i64));
        mask.extend(enc.get_attention_mask().iter().map(|&m| m as i64));
        ids.extend(std::iter::repeat(0).take(pad));
        mask.extend(std::iter::repeat(0).take(pad));
    }
    let input_ids = Tensor::from_slice(&ids, (bs, max_len), &inner.device)?;
    let attention_mask = Tensor::from_slice(&mask, (bs, max_len), &inner.device)?;

    // Forward pass (B,S,H) using API available in candle 0.8.x
    // Build token_type_ids as zeros (B,S)
    let zeros_tt: Vec<i64> = vec![0; bs * max_len];
    let token_type_ids = Tensor::from_slice(&zeros_tt, (bs, max_len), &inner.device)?;
    let hidden = inner
        .model
        .forward(&input_ids, &token_type_ids, Some(&attention_mask))
        .context("bert forward")?;

    // Mean pool with attention mask
    let mask_f = attention_mask.to_dtype(DType::F32)?; // (B,S)
    let mask_3d = mask_f.unsqueeze(2)?; // (B,S,1)
    let masked_hidden = hidden.broadcast_mul(&mask_3d)?; // (B,S,H)
    let sum_hidden = masked_hidden.sum(1)?; // (B,H)
    let counts = mask_f.sum(1)?.clamp(1e-9f32, f32::MAX)?.unsqueeze(1)?; // (B,1)
    let mean = sum_hidden.broadcast_div(&counts)?; // (B,H)

    // L2 normalize row-wise
    let norms = mean.sqr()?.sum(1)?.sqrt()?.unsqueeze(1)?; // (B,1)
    let normed = mean.broadcast_div(&norms)?; // (B,H)

    Ok(normed.to_vec2::<f32>()?)
}

fn find_model_files_in(
    dir: &std::path::Path,
) -> Option<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    let tok = dir.join("tokenizer.json");
    let wts = dir.join("model.safetensors");
    let cfg = dir.join("config.json");
    if tok.exists() && wts.exists() && cfg.exists() {
        Some((tok, wts, cfg))
    } else {
        // Helpful hints during discovery failures
        if dir.exists() {
            if !tok.exists() {
                eprintln!("  · missing file in {}: tokenizer.json", dir.display());
            }
            if !cfg.exists() {
                eprintln!("  · missing file in {}: config.json (note: tokenizer_config.json is not the same)", dir.display());
            }
            if !wts.exists() {
                eprintln!("  · missing file in {}: model.safetensors", dir.display());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Helper to locate a sample chunks file under the repo.
    fn find_sample_chunks() -> Option<std::path::PathBuf> {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidates = vec![
            manifest.join("../../../samples/json-src"),
            manifest.join("../../../../samples/json-src"),
            std::path::PathBuf::from("samples/json-src"),
        ];
        for dir in candidates {
            if dir.is_dir() {
                for entry in fs::read_dir(dir).ok()?.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("json") {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn local_backend_embeds_sample_json() {
        // Try to build local backend; skip if unavailable (e.g., model files missing).
        let backend = match LocalGTEBackendBuilder::new().build() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("skipping test: LocalGTEBackend build failed — {}", e);
                return;
            }
        };
        let sample = match find_sample_chunks() {
            Some(p) => p,
            None => {
                eprintln!("skipping test: no sample chunks found");
                return;
            }
        };
        let data = fs::read_to_string(&sample).expect("read sample json");
        let arr: Vec<serde_json::Value> = serde_json::from_str(&data).expect("parse json");
        let mut texts: Vec<&str> = Vec::new();
        for v in arr.iter().take(8) {
            if let Some(t) = v.get("text").and_then(|x| x.as_str()) {
                if !t.trim().is_empty() {
                    texts.push(t);
                }
            }
        }
        assert!(!texts.is_empty(), "no non-empty texts found in sample");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let embs = rt.block_on(async {
            let arc: Arc<dyn EmbeddingBackend> = Arc::new(backend);
            arc.embed_batch(&texts).await
        });
        let embs = embs.expect("embed batch");
        assert_eq!(embs.len(), texts.len());
        for e in &embs {
            assert_eq!(e.len(), 384, "expected 384-d embeddings");
        }
    }
}
