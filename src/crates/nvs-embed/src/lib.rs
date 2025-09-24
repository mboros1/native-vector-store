pub mod backend;

use anyhow::{Context, Result};
use futures::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub use backend::openai::{OpenAIBackend, OpenAIBackendBuilder};
pub use backend::EmbeddingBackend;
#[cfg(feature = "local-embed")]
pub use backend::local::{LocalGTEBackend, LocalGTEBackendBuilder};

#[derive(Debug, Clone)]
pub struct EmbedOptions {
    pub concurrency: usize,
    pub batch_size: usize,
    pub file_concurrency: usize,
    pub total_concurrency: usize,
}

impl Default for EmbedOptions {
    fn default() -> Self {
        Self {
            concurrency: 8,
            batch_size: 16,
            file_concurrency: 8,
            total_concurrency: 16,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChunkItemIn {
    text: String,
    #[allow(dead_code)]
    meta: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct DocOut<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    text: &'a str,
    metadata: serde_json::Value,
}

async fn embed_chunks_file_inner(
    backend: Arc<dyn EmbeddingBackend>,
    input: &Path,
    output: &Path,
    opts: &EmbedOptions,
    show_progress: bool,
    sem: Option<Arc<Semaphore>>,
) -> Result<()> {
    let mut s = String::new();
    {
        let f = File::open(input)?;
        let mut br = BufReader::new(f);
        br.read_to_string(&mut s)?;
    }
    let arr: Vec<serde_json::Value> = serde_json::from_str(&s).context("parse chunk JSON array")?;
    let mut items: Vec<ChunkItemIn> = Vec::with_capacity(arr.len());
    for v in arr {
        let item: ChunkItemIn = serde_json::from_value(v).context("invalid chunk item")?;
        items.push(item);
    }

    let texts: Vec<&str> = items.iter().map(|it| it.text.as_str()).collect();
    let batch_size = std::cmp::max(1, opts.batch_size);
    let mut batches: Vec<(usize, Vec<&str>)> = Vec::new();
    let mut i = 0usize;
    while i < texts.len() {
        let end = std::cmp::min(texts.len(), i + batch_size);
        batches.push((i, texts[i..end].to_vec()));
        i = end;
    }

    let pb = if show_progress {
        let pb = indicatif::ProgressBar::new(batches.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::with_template("{spinner:.green} {pos}/{len} batches")
                .unwrap(),
        );
        Some(pb)
    } else {
        None
    };

    let backend_for_batches = backend.clone();
    let sem_clone = sem.clone();
    let results = stream::iter(batches)
        .map(move |(start_idx, chunk)| {
            let backend = backend_for_batches.clone();
            let sem = sem_clone.clone();
            async move {
                let _permit = if let Some(ref s) = sem {
                    Some(s.clone().acquire_owned().await?)
                } else {
                    None
                };
                let embs = backend.embed_batch(&chunk).await?;
                drop(_permit);
                Ok::<(usize, Vec<Vec<f32>>), anyhow::Error>((start_idx, embs))
            }
        })
        .buffer_unordered(opts.concurrency)
        .inspect(|_| {
            if let Some(ref p) = pb {
                p.inc(1);
            }
        })
        .collect::<Vec<_>>()
        .await;
    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    // Flatten back into original order
    let mut out_embs: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    for r in results {
        let (start, vecs) = r?;
        for (j, e) in vecs.into_iter().enumerate() {
            out_embs[start + j] = Some(e);
        }
    }

    let mut docs_out = Vec::with_capacity(items.len());
    for (idx, it) in items.iter().enumerate() {
        let emb = out_embs[idx].clone().context("missing embedding result")?;
        // metadata object with embedding; also attach chunk meta under "chunk_meta" for traceability
        let mut meta_map = serde_json::Map::new();
        meta_map.insert(
            "embedding".into(),
            serde_json::Value::Array(
                emb.into_iter()
                    .map(|f| serde_json::Value::from(f))
                    .collect(),
            ),
        );
        meta_map.insert("chunk_meta".into(), it.meta.clone());
        let doc = DocOut {
            id: None,
            text: &it.text,
            metadata: serde_json::Value::Object(meta_map),
        };
        docs_out.push(doc);
    }

    let mut f = File::create(output)?;
    serde_json::to_writer_pretty(&mut f, &docs_out)?;
    f.flush()?;
    Ok(())
}

pub async fn embed_chunks_file(
    backend: Arc<dyn EmbeddingBackend>,
    input: &Path,
    output: &Path,
    opts: &EmbedOptions,
) -> Result<()> {
    embed_chunks_file_inner(backend, input, output, opts, true, None).await
}

pub async fn embed_chunks_file_quiet(
    backend: Arc<dyn EmbeddingBackend>,
    input: &Path,
    output: &Path,
    opts: &EmbedOptions,
) -> Result<()> {
    embed_chunks_file_inner(backend, input, output, opts, false, None).await
}

pub async fn embed_chunks_dir(
    backend: Arc<dyn EmbeddingBackend>,
    input_dir: &Path,
    output_dir: &Path,
    opts: &EmbedOptions,
) -> Result<usize> {
    use walkdir::WalkDir;
    std::fs::create_dir_all(output_dir)?;
    let mut jobs: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            let rel = entry.path().strip_prefix(input_dir).unwrap_or(entry.path());
            let stem = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            let mut out_rel = rel
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::new());
            out_rel.push(format!("{}.docs.json", stem));
            let out_path = output_dir.join(out_rel);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            jobs.push((entry.path().to_path_buf(), out_path));
        }
    }
    let pb = indicatif::ProgressBar::new(jobs.len() as u64);
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.cyan} {pos}/{len} files").unwrap(),
    );
    let sem = Arc::new(Semaphore::new(std::cmp::max(1, opts.total_concurrency)));
    let backend_for_jobs = backend.clone();
    let results = stream::iter(jobs)
        .map(move |(inp, out)| {
            let opts = opts.clone();
            let sem = sem.clone();
            let backend = backend_for_jobs.clone();
            async move {
                let r = embed_chunks_file_inner(backend, &inp, &out, &opts, false, Some(sem)).await;
                (inp, out, r)
            }
        })
        .buffer_unordered(std::cmp::max(1, opts.file_concurrency))
        .inspect(|_| pb.inc(1))
        .collect::<Vec<_>>()
        .await;
    pb.finish_and_clear();
    let mut ok = 0usize;
    for (inp, out, res) in results {
        match res {
            Ok(_) => {
                ok += 1;
            }
            Err(e) => eprintln!("failed: {} -> {} — {}", inp.display(), out.display(), e),
        }
    }
    Ok(ok)
}

pub async fn embed_chunks_jobs(
    backend: Arc<dyn EmbeddingBackend>,
    jobs: &[(PathBuf, PathBuf)],
    opts: &EmbedOptions,
) -> Result<Vec<(PathBuf, PathBuf, Result<()>)>> {
    use futures::stream::{self, StreamExt};
    let sem = Arc::new(Semaphore::new(std::cmp::max(1, opts.total_concurrency)));
    let results = stream::iter(jobs.iter().cloned())
        .map(|(inp, out)| {
            let opts = opts.clone();
            let backend = backend.clone();
            let sem = sem.clone();
            async move {
                let _permit = sem.clone().acquire_owned().await.ok();
                let res =
                    embed_chunks_file_inner(backend, &inp, &out, &opts, false, Some(sem)).await;
                (inp, out, res)
            }
        })
        .buffer_unordered(std::cmp::max(1, opts.file_concurrency))
        .collect::<Vec<_>>()
        .await;
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transforms_chunk_items_to_docs_schema() {
        let item = ChunkItemIn {
            text: "Hello".into(),
            meta: json!({"start_page": 0}),
        };
        let emb = vec![0.1f32, 0.2, 0.3];
        let mut meta = serde_json::Map::new();
        meta.insert("embedding".into(), json!(emb));
        meta.insert("chunk_meta".into(), json!({"start_page": 0}));
        let out = DocOut {
            id: None,
            text: &item.text,
            metadata: serde_json::Value::Object(meta),
        };
        let s = serde_json::to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["text"], "Hello");
        assert!(v["metadata"]["embedding"].is_array());
    }
}
