use anyhow::{anyhow, Result};
use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::task;

static CACHE: OnceCell<StoreCache> = OnceCell::new();

struct StoreCache {
    inner: RwLock<HashMap<String, Arc<nvs_core::VectorStore>>>,
}

impl StoreCache {
    fn new() -> Self {
        Self { inner: RwLock::new(HashMap::new()) }
    }
    fn get(&self, key: &str) -> Option<Arc<nvs_core::VectorStore>> {
        self.inner.read().ok()?.get(key).cloned()
    }
    fn put(&self, key: String, store: Arc<nvs_core::VectorStore>) {
        if let Ok(mut w) = self.inner.write() {
            if w.len() >= 4 && !w.contains_key(&key) {
                if let Some(first) = w.keys().next().cloned() { let _ = w.remove(&first); }
            }
            w.insert(key, store);
        }
    }
}

#[derive(Deserialize)]
struct HybridQuery {
    embedding: Vec<f32>,
    q: String,
    k: usize,
    vector_weight: Option<f32>,
}

#[derive(Deserialize)]
struct Event {
    // Name of the bundle under BUNDLES_ROOT (e.g., "1" or "2").
    bundle: String,
    query: HybridQuery,
}

#[derive(Serialize)]
struct ScoredDocument {
    id: String,
    text: String,
    metadata: JsonValue,
    score: f32,
}

fn resolve_bundle_dir(bundle_name: &str) -> Result<PathBuf> {
    let root = std::env::var("BUNDLES_ROOT").unwrap_or_else(|_| "./bundles".to_string());
    let p = PathBuf::from(root).join(bundle_name);
    if !p.join("manifest.json").exists() {
        return Err(anyhow!("bundle '{}' not found at {}", bundle_name, p.display()));
    }
    Ok(p)
}

async fn load_or_get(bundle: &str) -> Result<Arc<nvs_core::VectorStore>> {
    let cache = CACHE.get_or_init(StoreCache::new);
    if let Some(s) = cache.get(bundle) { return Ok(s); }
    let dir = resolve_bundle_dir(bundle)?;
    let dir_s = dir.to_string_lossy().to_string();
    let store = task::spawn_blocking(move || nvs_core::VectorStore::open(dir_s)).await??;
    let store = Arc::new(store);
    cache.put(bundle.to_string(), store.clone());
    Ok(store)
}

async fn handler(ev: LambdaEvent<serde_json::Value>) -> Result<serde_json::Value, Error> {
    // Optional: configure rayon threads via env
    if let Some(n) = std::env::var("RAYON_NUM_THREADS").ok().and_then(|s| s.parse::<usize>().ok()) {
        let _ = rayon::ThreadPoolBuilder::new().num_threads(n).build_global();
    }

    let event: Event = serde_json::from_value(ev.payload)
        .map_err(|e| anyhow!("invalid event JSON: {}", e))?;
    let bundle = event.bundle.clone();
    let store = load_or_get(&bundle).await?;

    let q = event.query;
    let hits = store.search_hybrid(&q.embedding, &q.q, q.k, q.vector_weight.unwrap_or(0.5));
    let mut docs: Vec<ScoredDocument> = Vec::with_capacity(hits.len());
    for (id, score) in hits.into_iter() {
        if let Some(doc) = store.get_document_parsed(id) {
            docs.push(ScoredDocument { id: doc.id, text: doc.text, metadata: doc.metadata, score });
        }
    }
    Ok(serde_json::to_value(serde_json::json!({
        "bundle": bundle,
        "docs": docs,
    }))?)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(service_fn(handler)).await
}
