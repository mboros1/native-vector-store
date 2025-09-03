use anyhow::{Context, Result};
use clap::Parser;
use dashmap::DashMap;
use rustc_hash::FxHashMap;
use serde::{ser::{SerializeMap, Serializer}, Deserialize};
use serde_json::{self, Map as JsonMap, Value as JsonValue};
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use xxhash_rust::xxh64::xxh64;

#[derive(Parser, Debug)]
#[command(name = "nvs-packer")]
#[command(about = "Pack JSON docs into a native-vector-store bundle", long_about = None)]
struct Cli {
    /// Input directory containing JSON files (expects docs.json array by default)
    input: PathBuf,
    /// Output directory (created if missing)
    #[arg(short = 'o', long = "out", default_value = "./nvs-bundle")]
    out: PathBuf,
    /// Metadata block size in bytes
    #[arg(long = "block-size", default_value_t = 131072)]
    block_size: usize,
    /// Embedding model name for manifest
    #[arg(long = "model", default_value = "unknown")]
    model: String,
    /// Output vector dtype: f32 (default) or f16
    #[arg(long = "quantize", value_parser = ["f16", "f32"], default_value = "f32")]
    quantize: String,
    /// Compress metadata blocks: none (default) or zstd
    #[arg(long = "compress", value_parser = ["none", "zstd"], default_value = "none")]
    compress: String,
    /// Zstd compression level (1-22), used when --compress=zstd
    #[arg(long = "zstd-level", default_value_t = 3)]
    zstd_level: i32,
    /// Include embeddings in meta.blocks JSON (defaults to false to avoid duplication)
    #[arg(long = "meta-include-embeddings", default_value_t = false)]
    meta_include_embeddings: bool,
    /// Use fast adaptive JSON loader (mmap small files, parallel consumers, streaming arrays)
    #[arg(long = "fast-loader", default_value_t = false)]
    fast_loader: bool,
    /// Threshold in bytes below which JSON files are mmapped (used with --fast-loader)
    #[arg(long = "mmap-threshold", default_value_t = 5_000_000)]
    mmap_threshold: usize,
    /// Parallel BM25 merge buckets (0=auto, recommend 16-32)
    #[arg(long = "bm25-buckets", default_value_t = 0)]
    bm25_buckets: usize,
    // Pipeline removed: sequential flow only
}

#[derive(Deserialize)]
struct InputDocRaw {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    metadata: Option<JsonValue>,
}

#[derive(Clone)]
struct Doc {
    id: String,
    text: String,
    embedding: Vec<f32>,
    meta: Option<JsonMap<String, JsonValue>>,
}

fn read_docs(input_dir: &Path) -> Result<(Vec<Doc>, Vec<(String, usize)>)> {
    use walkdir::WalkDir;
    let mut docs = Vec::new();
    let mut receipts: Vec<(String, usize)> = Vec::new();
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(indicatif::ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
    pb.set_message("Scanning JSON files...");
    let mut _total = 0usize;
    let mut skipped = 0usize;
    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
        {
            let path = entry.path();
            pb.set_message(format!("Reading {}", path.display()));
            let mut s = String::new();
            File::open(path)
                .with_context(|| format!("open {}", path.display()))?
                .read_to_string(&mut s)?;
            // Try array first
            if s.trim_start().starts_with('[') {
                let before = docs.len();
                let arr: Vec<serde_json::Value> = serde_json::from_str(&s)
                    .with_context(|| format!("parse array in {}", path.display()))?;
                for (i, v) in arr.into_iter().enumerate() {
                    _total += 1;
                    match serde_json::from_value::<InputDocRaw>(v) {
                        Ok(r) => {
                            let text = r.text.or(r.content).unwrap_or_default();
                            if let Some(mv) = r.metadata {
                                if let Some((embedding, meta_other)) = extract_embedding_and_meta(mv) {
                                    if !embedding.is_empty() {
                                        let id = r.id.unwrap_or_else(|| {
                                            let h = xxh64(text.as_bytes(), 0) ^ (i as u64);
                                            format!("doc-{h:016x}")
                                        });
                                        docs.push(Doc { id, text, embedding, meta: meta_other });
                                    } else {
                                        skipped += 1;
                                        eprintln!(
                                            "{} skipping doc without embedding ({}:#{})",
                                            console::style("! ").yellow(),
                                            path.display(),
                                            i
                                        );
                                    }
                                } else {
                                    skipped += 1;
                                    eprintln!(
                                        "{} skipping doc without embedding ({}:#{})",
                                        console::style("! ").yellow(),
                                        path.display(),
                                        i
                                    );
                                }
                            } else {
                                    skipped += 1;
                                    eprintln!(
                                        "{} skipping doc without metadata ({}:#{})",
                                        console::style("! ").yellow(),
                                        path.display(),
                                        i
                                    );
                            }
                        },
                        Err(e) => {
                            skipped += 1;
                            eprintln!(
                                "{} skipping invalid doc ({}:#{}) — {}",
                                console::style("! ").yellow(),
                                path.display(),
                                i,
                                e
                            );
                        }
                    }
                }
                let produced = docs.len() - before;
                let fname = path.display().to_string();
                receipts.push((fname, produced));
            } else {
                _total += 1;
                match serde_json::from_str::<InputDocRaw>(&s) {
                    Ok(r) => {
                        let text = r.text.or(r.content).unwrap_or_default();
                        if let Some(mv) = r.metadata {
                            if let Some((embedding, meta_other)) = extract_embedding_and_meta(mv) {
                                if !embedding.is_empty() {
                                    let id = r.id.unwrap_or_else(|| {
                                        let h = xxh64(text.as_bytes(), 0);
                                        format!("doc-{h:016x}")
                                    });
                                    docs.push(Doc { id, text, embedding, meta: meta_other });
                                    let fname = path.display().to_string();
                                    receipts.push((fname, 1));
                                } else {
                                    skipped += 1;
                                    eprintln!(
                                        "{} skipping doc without embedding ({})",
                                        console::style("! ").yellow(),
                                        path.display()
                                    );
                                    let fname = path.display().to_string();
                                    receipts.push((fname, 0));
                                }
                            } else {
                                skipped += 1;
                                eprintln!(
                                    "{} skipping doc without embedding ({})",
                                    console::style("! ").yellow(),
                                    path.display()
                                );
                                let fname = path.display().to_string();
                                receipts.push((fname, 0));
                            }
                        } else {
                            skipped += 1;
                            eprintln!(
                                "{} skipping doc without metadata ({})",
                                console::style("! ").yellow(),
                                path.display()
                            );
                            let fname = path.display().to_string();
                            receipts.push((fname, 0));
                        }
                    }
                    Err(e) => {
                        skipped += 1;
                        eprintln!(
                            "{} skipping invalid doc ({}) — {}",
                            console::style("! ").yellow(),
                            path.display(),
                            e
                        );
                        let fname = path.display().to_string();
                        receipts.push((fname, 0));
                    }
                }
            }
        }
    }
    pb.finish_with_message(format!("Loaded {} docs (skipped {})", docs.len(), skipped));
    receipts.sort_by(|a,b| a.0.cmp(&b.0));
    Ok((docs, receipts))
}

// Fast adaptive loader: mmap small JSON files, parallel parse with streaming arrays
fn read_docs_fast(input_dir: &Path, mmap_threshold: usize) -> Result<(Vec<Doc>, Vec<(String, usize)>)> {
    use walkdir::WalkDir;
    use crossbeam_channel as chan;
    use std::thread;
    use memmap2::Mmap;
    use serde::de::{self, SeqAccess, Visitor, Deserializer as _};

    #[derive(Debug)]
    enum Buf { Mmap(Mmap), Vec(Vec<u8>) }
    impl Buf { fn as_slice(&self) -> &[u8] { match self { Buf::Mmap(m) => &m, Buf::Vec(v) => v } } }
    #[derive(Debug)]
    struct Job { _path: PathBuf, buf: Buf }

    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(indicatif::ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
    pb.set_message("Scanning JSON files (fast)...");

    let (tx, rx) = chan::bounded::<Job>(64);
    let producer = {
        let tx = tx.clone();
        let input_dir = input_dir.to_path_buf();
        thread::spawn(move || {
            for entry in WalkDir::new(&input_dir).into_iter().filter_map(|e| e.ok()) {
                if !(entry.file_type().is_file() && entry.path().extension().map(|e| e == "json").unwrap_or(false)) {
                    continue;
                }
                let path = entry.path().to_path_buf();
                let md = match std::fs::metadata(&path) { Ok(m) => m, Err(_) => continue };
                let job = if md.len() as usize <= mmap_threshold {
                    // mmap
                    match File::open(&path).and_then(|f| unsafe { Mmap::map(&f) }.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))) {
                        Ok(m) => Job { _path: path, buf: Buf::Mmap(m) },
                        Err(_) => {
                            // fallback to Vec
                            match std::fs::read(&path) { Ok(v) => Job { _path: path, buf: Buf::Vec(v) }, Err(_) => continue }
                        }
                    }
                } else {
                    match std::fs::read(&path) { Ok(v) => Job { _path: path, buf: Buf::Vec(v) }, Err(_) => continue }
                };
                if tx.send(job).is_err() { break; }
            }
            // drop tx to close
        })
    };

    let nthreads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let mut handles = Vec::new();
    for _ in 0..nthreads {
        let rx = rx.clone();
        handles.push(thread::spawn(move || -> (Vec<Doc>, usize, Vec<(String, usize)>) {
            let mut out: Vec<Doc> = Vec::with_capacity(1024);
            let mut skipped: usize = 0;
            let mut receipts: Vec<(String, usize)> = Vec::with_capacity(128);

            while let Ok(job) = rx.recv() {
                let file_name = job._path.display().to_string();
                let bytes = job.buf.as_slice();
                // Streaming parse: support array or single object
                let mut de = serde_json::Deserializer::from_slice(bytes);
                // Try array streaming first
                struct StreamVisitor<'a> { out: &'a mut Vec<Doc>, skipped: &'a mut usize }
                impl<'de, 'a> Visitor<'de> for StreamVisitor<'a> {
                    type Value = ();
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "array or object of docs") }
                    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error> where A: SeqAccess<'de> {
                        while let Some(raw) = seq.next_element::<InputDocRaw>()? {
                            if let Some((embedding, meta_other)) = raw.metadata.and_then(extract_embedding_and_meta) {
                                let text = raw.text.or(raw.content).unwrap_or_default();
                                if !embedding.is_empty() {
                                    let id = raw.id.unwrap_or_else(|| {
                                        let h = xxh64(text.as_bytes(), 0);
                                        format!("doc-{h:016x}")
                                    });
                                    self.out.push(Doc { id, text, embedding, meta: meta_other });
                                } else { *self.skipped += 1; }
                            } else { *self.skipped += 1; }
                        }
                        Ok(())
                    }
                    fn visit_map<M>(self, mut map: M) -> Result<(), M::Error> where M: de::MapAccess<'de> {
                        // Reconstruct InputDocRaw from map streaming
                        let mut id: Option<String> = None; let mut text: Option<String> = None; let mut content: Option<String> = None; let mut metadata: Option<serde_json::Value> = None;
                        while let Some(k) = map.next_key::<String>()? {
                            match k.as_str() {
                                "id" => { id = map.next_value()?; }
                                "text" => { text = map.next_value()?; }
                                "content" => { content = map.next_value()?; }
                                "metadata" => { metadata = map.next_value()?; }
                                _ => { let _ = map.next_value::<serde_json::Value>()?; }
                            }
                        }
                        let raw = InputDocRaw { id, text, content, metadata };
                        if let Some((embedding, meta_other)) = raw.metadata.and_then(extract_embedding_and_meta) {
                            let text = raw.text.or(raw.content).unwrap_or_default();
                            if !embedding.is_empty() {
                                let id = raw.id.unwrap_or_else(|| {
                                    let h = xxh64(text.as_bytes(), 0);
                                    format!("doc-{h:016x}")
                                });
                                self.out.push(Doc { id, text, embedding, meta: meta_other });
                            } else { *self.skipped += 1; }
                        } else { *self.skipped += 1; }
                        Ok(())
                    }
                }
                let mut skipped_local = 0usize;
                let before = out.len();
                let vis = StreamVisitor { out: &mut out, skipped: &mut skipped_local };
                let res = de.deserialize_any(vis);
                let produced_for_file = if res.is_err() { skipped += 1; 0 } else { skipped += skipped_local; out.len() - before };
                receipts.push((file_name, produced_for_file));
            }
            (out, skipped, receipts)
        }));
    }
    drop(tx);
    let _ = producer.join();
    let mut docs: Vec<Doc> = Vec::new();
    let mut skipped = 0usize;
    let mut receipts: Vec<(String, usize)> = Vec::new();
    for h in handles { let (mut v, s, mut r) = h.join().unwrap_or_default(); docs.append(&mut v); skipped += s; receipts.append(&mut r); }
    pb.finish_with_message(format!("Loaded {} docs (skipped {})", docs.len(), skipped));
    receipts.sort_by(|a,b| a.0.cmp(&b.0));
    Ok((docs, receipts))
}

// Extract the embedding array from metadata JSON and return the remaining object fields
fn extract_embedding_and_meta(meta: JsonValue) -> Option<(Vec<f32>, Option<JsonMap<String, JsonValue>>)> {
    match meta {
        JsonValue::Object(mut map) => {
            let emb = map.remove("embedding")?;
            let embedding = match emb {
                JsonValue::Array(arr) => {
                    let mut v = Vec::with_capacity(arr.len());
                    for val in arr {
                        if let JsonValue::Number(n) = val {
                            if let Some(f) = n.as_f64() { v.push(f as f32); } else { return None; }
                        } else { return None; }
                    }
                    v
                }
                _ => return None,
            };
            let meta_other = if map.is_empty() { None } else { Some(map) };
            Some((embedding, meta_other))
        }
        _ => None,
    }
}

fn write_vectors(docs: &[Doc], dim: usize, out: &Path, dtype: &str) -> Result<()> {
    match dtype {
        "f16" => {
            use half::f16;
            let row_bytes = dim * 2;
            let aligned = ((row_bytes + 63) / 64) * 64;
            let mut data = vec![0u8; docs.len() * aligned];
            for (i, d) in docs.iter().enumerate() {
                anyhow::ensure!(
                    d.embedding.len() == dim,
                    "dimension mismatch for doc {}",
                    d.id
                );
                for j in 0..dim {
                    let off = i * aligned + j * 2;
                    let h = f16::from_f32(d.embedding[j]);
                    data[off..off + 2].copy_from_slice(&h.to_le_bytes());
                }
            }
            let mut f = File::create(out.join("vectors.f16"))?;
            f.write_all(&data)?;
            Ok(())
        }
        _ => {
            let row_bytes = dim * 4;
            let aligned = ((row_bytes + 63) / 64) * 64;
            let mut data = vec![0u8; docs.len() * aligned];
            for (i, d) in docs.iter().enumerate() {
                anyhow::ensure!(
                    d.embedding.len() == dim,
                    "dimension mismatch for doc {}",
                    d.id
                );
                for j in 0..dim {
                    let off = i * aligned + j * 4;
                    data[off..off + 4].copy_from_slice(&d.embedding[j].to_le_bytes());
                }
            }
            let mut f = File::create(out.join("vectors.f32"))?;
            f.write_all(&data)?;
            Ok(())
        }
    }
}

struct Bm25Stats { tf: std::time::Duration, local: std::time::Duration, merge: std::time::Duration, write: std::time::Duration }

fn write_bm25_and_terms(docs: &[Doc], out: &Path, bm25_buckets: usize) -> Result<(f64, Vec<String>, usize, usize, Bm25Stats)> {
    use rayon::prelude::*;
    // Phase 1: per-doc tokenization and TF maps in parallel; record doc lengths
    let doc_lens: Vec<AtomicUsize> = (0..docs.len()).map(|_| AtomicUsize::new(0)).collect();
    let mut doc_tfs: Vec<FxHashMap<String, u32>> = (0..docs.len()).map(|_| FxHashMap::default()).collect();

    let t_tf_start = std::time::Instant::now();
    doc_tfs
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, tfmap)| {
            let d = &docs[i];
            let clean = nvs_core::tokenizer::preprocess_bm25(&d.text);
            let kept = tokenize_bm25_into(&clean, tfmap);
            doc_lens[i].store(kept, Ordering::Relaxed);
        });
    let t_tf = t_tf_start.elapsed();

    // Write doc lengths
    {
        let mut f = File::create(out.join("doclen.u32"))?;
        for len in &doc_lens {
            let v = len.load(Ordering::Relaxed) as u32;
            f.write_all(&v.to_le_bytes())?;
        }
    }

    let total_tokens: usize = doc_lens.iter().map(|x| x.load(Ordering::Relaxed)).sum();

    // Phase 2: build per-thread postings maps over contiguous doc ranges
    let t_local_start = std::time::Instant::now();
    let n = docs.len();
    let threads = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(4);
    let chunks = std::cmp::max(threads, 1);
    let chunk_size = (n + chunks - 1) / chunks;
    let mut local_maps: Vec<FxHashMap<String, Vec<(usize, u32)>>> = Vec::new();
    local_maps.resize_with(chunks, FxHashMap::default);
    local_maps
        .par_iter_mut()
        .enumerate()
        .for_each(|(ci, local)| {
            let start = ci * chunk_size;
            if start >= n { return; }
            let end = std::cmp::min(n, start + chunk_size);
            for i in start..end {
                for (term, count) in doc_tfs[i].iter() {
                    local.entry(term.clone()).or_default().push((i, *count));
                }
            }
        });
    let t_local = t_local_start.elapsed();

    // Phase 3: bucketed k-way merge and write outputs
    let buckets = if bm25_buckets > 0 { bm25_buckets } else { std::cmp::max(1, std::cmp::min(32, threads * 2)) };
    struct BucketOut { terms: Vec<String>, postings: Vec<u8>, lex: Vec<(u64, u32, u32)> }
    let bucket_out: Vec<std::sync::Mutex<Option<BucketOut>>> = (0..buckets).map(|_| std::sync::Mutex::new(None)).collect();
    let t_merge_start = std::time::Instant::now();
    (0..buckets).into_par_iter().for_each(|b| {
        let mask = buckets.next_power_of_two() - 1;
        let use_mask = (mask + 1) == buckets;
        let mut uniq: FxHashMap<String, ()> = FxHashMap::default();
        for loc in &local_maps {
            for k in loc.keys() {
                let h = fxhash::hash64(k);
                let bi = if use_mask { (h as usize) & mask } else { (h as usize) % buckets };
                if bi == b { uniq.entry(k.clone()).or_insert(()); }
            }
        }
        let mut terms_b: Vec<String> = uniq.into_keys().collect();
        terms_b.sort();
        let mut postings_b: Vec<u8> = Vec::new();
        let mut lex_b: Vec<(u64, u32, u32)> = Vec::with_capacity(terms_b.len());
        for term in &terms_b {
            // collect slices
            let mut slices: Vec<&[(usize, u32)]> = Vec::new();
            let mut pos: Vec<usize> = Vec::new();
            for loc in &local_maps {
                if let Some(vec) = loc.get(term) { slices.push(vec); pos.push(0); }
            }
            let mut prev = 0usize; let mut len: u32 = 0; let start = postings_b.len() as u64;
            loop {
                let mut best = usize::MAX; let mut which = usize::MAX;
                for i in 0..slices.len() {
                    if pos[i] < slices[i].len() {
                        let d = slices[i][pos[i]].0;
                        if d < best { best = d; which = i; }
                    }
                }
                if which == usize::MAX { break; }
                let (doc, tf) = slices[which][pos[which]]; pos[which] += 1;
                let delta = (doc - prev) as u32; prev = doc;
                postings_b.extend_from_slice(&delta.to_le_bytes()); postings_b.extend_from_slice(&tf.to_le_bytes()); len += 1;
            }
            let df = len; lex_b.push((start, len, df));
        }
        let mut g = bucket_out[b].lock().unwrap();
        *g = Some(BucketOut { terms: terms_b, postings: postings_b, lex: lex_b });
    });
    let t_merge = t_merge_start.elapsed();

    // Move buckets out and compute capacities
    let mut buckets_vec: Vec<BucketOut> = Vec::with_capacity(buckets);
    let mut total_terms = 0usize; let mut total_post_bytes = 0usize;
    for b in 0..buckets { if let Some(outb) = bucket_out[b].lock().unwrap().take() { total_terms += outb.terms.len(); if let Some((off, len, _)) = outb.lex.last().copied() { total_post_bytes += (off as usize) + (len as usize)*8; } buckets_vec.push(outb); } else { buckets_vec.push(BucketOut{terms:Vec::new(), postings:Vec::new(), lex:Vec::new()}); } }

    // Assemble: final merge, coalesced copies, stream terms
    let t_assemble_start = std::time::Instant::now();
    let mut heads = vec![0usize; buckets];
    let mut postings = Vec::<u8>::with_capacity(total_post_bytes);
    let mut lexicon = Vec::<u8>::with_capacity(total_terms * 16);
    let mut terms_writer = std::io::BufWriter::new(File::create(out.join("terms.dict"))?);
    let mut global_off: u64 = 0;
    let mut run_bucket: Option<usize> = None; let mut run_start = 0usize; let mut run_bytes = 0usize; let mut run_expected_next_off = 0usize;
    loop {
        let mut best_b = usize::MAX; let mut best_term: Option<&str> = None;
        for b in 0..buckets { let h = heads[b]; let outb = &buckets_vec[b]; if h < outb.terms.len() { let t = &outb.terms[h]; if best_term.map_or(true, |cur| t.as_str() < cur) { best_term = Some(t.as_str()); best_b = b; } } }
        if best_b == usize::MAX { break; }
        let outb = &buckets_vec[best_b]; let idx = heads[best_b]; let (off_rel, len, df) = outb.lex[idx]; let start = off_rel as usize; let bytes = (len as usize)*8;
        // write term
        let term = outb.terms[idx].as_str(); let l = term.len() as u32; terms_writer.write_all(&l.to_le_bytes())?; terms_writer.write_all(term.as_bytes())?;
        // lex entry
        lexicon.extend_from_slice(&global_off.to_le_bytes()); lexicon.extend_from_slice(&len.to_le_bytes()); lexicon.extend_from_slice(&df.to_le_bytes());
        // coalesce copy
        if run_bucket == Some(best_b) && start == run_expected_next_off { run_bytes += bytes; run_expected_next_off += bytes; } else { if let Some(rb) = run_bucket { let src = &buckets_vec[rb].postings[run_start..run_start+run_bytes]; postings.extend_from_slice(src); } run_bucket = Some(best_b); run_start = start; run_bytes = bytes; run_expected_next_off = start + bytes; }
        global_off += bytes as u64; heads[best_b] += 1;
    }
    if let Some(rb) = run_bucket { let src = &buckets_vec[rb].postings[run_start..run_start+run_bytes]; postings.extend_from_slice(src); }
    terms_writer.flush()?;
    let t_assemble = t_assemble_start.elapsed();

    let t_io_start = std::time::Instant::now();
    { let mut pf = File::create(out.join("postings.bin"))?; pf.write_all(&postings)?; let mut lf = File::create(out.join("lexicon.bin"))?; lf.write_all(&lexicon)?; }
    let t_io = t_io_start.elapsed();
    let postings_entries_count: usize = postings.len()/8;
    let avgdl = if docs.is_empty() {
        0.0
    } else {
        (total_tokens as f64) / (docs.len() as f64)
    };
    let postings_entries: usize = postings_entries_count;
    // For signature compatibility, flatten terms if needed
    let terms: Vec<String> = buckets_vec.into_iter().flat_map(|b| b.terms).collect();
    Ok((avgdl, terms, postings_entries, total_tokens, Bm25Stats { tf: t_tf, local: t_local, merge: t_merge, write: t_assemble + t_io }))
}

// Fast BM25 tokenizer: lowercases ASCII, splits on whitespace and most punctuation,
// keeps internal hyphens, and normalizes tokens via bm25_normalize_token.
fn tokenize_bm25_into(text: &str, tf: &mut FxHashMap<String, u32>) -> usize {
    use nvs_core::tokenizer::bm25_normalize_token;
    let mut buf = String::with_capacity(32);
    let mut kept = 0usize;
    let mut flush = |buf: &mut String| {
        if buf.is_empty() { return; }
        // Lowercase ASCII in-place
        for b in unsafe { buf.as_bytes_mut() } { if (b'A'..=b'Z').contains(b) { *b = *b + 32; } }
        if let Some(norm) = bm25_normalize_token(&buf) {
            if !nvs_core::tokenizer::is_stopword(&norm) {
                *tf.entry(norm).or_insert(0) += 1; kept += 1;
            }
        }
        buf.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // whitespace and control
            '\r' | '\t' | '\n' | '\x0C' => { flush(&mut buf); },
            // remove soft hyphen/zero-width/BOM
            '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => { /* skip */ }
            '-' => {
                // dehyphenate: - followed by optional ws and newline
                let mut it = chars.clone();
                let mut consumed = 0; let mut is_break = false;
                while let Some(nc) = it.next() {
                    if nc == '\n' { is_break = true; consumed += 1; break; }
                    else if nc == '\r' || nc == '\t' || nc == ' ' { consumed += 1; continue; }
                    else { break; }
                }
                if is_break { for _ in 0..consumed { let _ = chars.next(); } flush(&mut buf); }
                else { buf.push('-'); }
            }
            c if c.is_alphanumeric() || c == '_' || c >= '\u{80}' => { buf.push(c); }
            // allowed internal punct: keep as part of token
            '\'' | '/' | '&' | '.' => { buf.push(ch); }
            _ => { flush(&mut buf); }
        }
    }
    flush(&mut buf);
    kept
}

fn write_meta_and_index(
    docs: &[Doc],
    block_size: usize,
    out: &Path,
    compress: &str,
    zstd_level: i32,
    include_embeddings: bool,
) -> Result<usize> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut headers: Vec<(u32, u32, u32, u32)> = Vec::new();
    let mut idx: Vec<u8> = Vec::new();
    let mut cur = Vec::<u8>::with_capacity(block_size);
    let mut cur_usize = 0u32;
    let mut cur_docs = 0u32;
    let mut block_id = 0u32;
    for d in docs {
        let mut wrote = false;
        for attempt in 0..2 {
            let rec_offset = cur_usize;
            let cur_len0 = cur.len();
            // id
            cur.extend_from_slice(&(d.id.len() as u32).to_le_bytes());
            cur.extend_from_slice(d.id.as_bytes());
            // text
            cur.extend_from_slice(&(d.text.len() as u32).to_le_bytes());
            cur.extend_from_slice(d.text.as_bytes());
            // meta
            let len_pos = cur.len();
            cur.extend_from_slice(&0u32.to_le_bytes());
            let meta_start = cur.len();
            if include_embeddings {
                // Stream a merged object: existing metadata fields + embedding
                let mut ser = serde_json::Serializer::new(&mut cur);
                let mut map = ser.serialize_map(None)?;
                if let Some(ref m) = d.meta {
                    for (k, v) in m.iter() { map.serialize_entry(k, v)?; }
                }
                map.serialize_entry("embedding", &d.embedding)?;
                map.end()?;
            } else {
                if let Some(ref m) = d.meta {
                    // Write the remaining metadata object (may be empty)
                    let mut ser = serde_json::Serializer::new(&mut cur);
                    let mut map = ser.serialize_map(Some(m.len()))?;
                    for (k, v) in m.iter() { map.serialize_entry(k, v)?; }
                    map.end()?;
                } else {
                    cur.extend_from_slice(b"{}");
                }
            }
            let meta_written = (cur.len() - meta_start) as u32;
            cur[len_pos..len_pos + 4].copy_from_slice(&meta_written.to_le_bytes());
            let rec_size = (cur.len() - cur_len0) as u32;

            if cur_docs > 0 && (cur_usize as usize + rec_size as usize) > block_size {
                // overflow: rollback and start a new block
                cur.truncate(cur_len0);
                if attempt == 0 {
                    headers.push((block_id, cur_usize, cur_docs, 0));
                    blocks.push(std::mem::take(&mut cur));
                    cur = Vec::with_capacity(block_size);
                    cur_usize = 0;
                    cur_docs = 0;
                    block_id += 1;
                    continue;
                } else {
                    anyhow::bail!("record larger than block size");
                }
            }

            // idx entry (after confirming fit)
            idx.extend_from_slice(&block_id.to_le_bytes());
            idx.extend_from_slice(&rec_offset.to_le_bytes());
            idx.extend_from_slice(&rec_size.to_le_bytes());
            idx.extend_from_slice(&0u32.to_le_bytes());

            cur_usize += rec_size;
            cur_docs += 1;
            wrote = true;
            break;
        }
        if !wrote { anyhow::bail!("failed to write record after rollover"); }
    }
    if cur_docs > 0 {
        headers.push((block_id, cur_usize, cur_docs, 0));
        blocks.push(cur);
    }
    // meta.blocks with optional zstd compression per block (still padded to fixed block_size)
    {
        use rayon::prelude::*;
        let codec_flag = if compress == "zstd" { 1u32 } else { 0u32 };
        // Pre-compress blocks in parallel to maintain performance
        let comp: Vec<(Vec<u8>, u32, u32)> = if codec_flag == 1 {
            blocks
                .par_iter()
                .map(|b| {
                    let decomp_len = b.len() as u32;
                    let compressed =
                        zstd::bulk::compress(b, zstd_level).unwrap_or_else(|_| b.clone());
                    (compressed, decomp_len, 1u32)
                })
                .collect()
        } else {
            blocks
                .iter()
                .map(|b| (b.clone(), b.len() as u32, 0u32))
                .collect()
        };

        let mut f = File::create(out.join("meta.blocks"))?;
        f.write_all(&(comp.len() as u32).to_le_bytes())?;
        // Write headers: (comp_size, decomp_size, doc_count, codec)
        for (i, (bytes, decomp_len, cod)) in comp.iter().enumerate() {
            let comp_size = bytes.len() as u32;
            let dcount = headers.get(i).map(|h| h.2).unwrap_or(0);
            let codec = if *cod == 1 { 1u32 } else { 0u32 };
            // If compressed size overflows block_size, fallback: write uncompressed later and mark codec=0
            let final_comp_size = if comp_size as usize > block_size {
                *decomp_len
            } else {
                comp_size
            };
            let final_codec = if comp_size as usize > block_size {
                0u32
            } else {
                codec
            };
            f.write_all(&final_comp_size.to_le_bytes())?;
            f.write_all(&decomp_len.to_le_bytes())?;
            f.write_all(&dcount.to_le_bytes())?;
            f.write_all(&final_codec.to_le_bytes())?;
        }
        // Write block payloads padded to block_size
        // Reusable padding buffer
        let pad = vec![0u8; block_size];
        for (i, (bytes, _decomp_len, cod)) in comp.into_iter().enumerate() {
            let use_comp = if bytes.len() > block_size {
                false
            } else {
                cod == 1
            };
            if use_comp {
                f.write_all(&bytes)?;
                if bytes.len() < block_size {
                    let need = block_size - bytes.len();
                    f.write_all(&pad[..need])?;
                }
            } else {
                // write original uncompressed block
                let b = &blocks[i];
                f.write_all(b)?;
                if b.len() < block_size {
                    let need = block_size - b.len();
                    f.write_all(&pad[..need])?;
                }
            }
        }
    }
    // meta.idx (buffered)
    {
        let f = File::create(out.join("meta.idx"))?;
        let mut bw = BufWriter::new(f);
        bw.write_all(&idx)?;
        bw.flush()?;
    }
    Ok(headers.len())
}

fn write_manifest(
    out: &Path,
    n: usize,
    dim: usize,
    block_size: usize,
    avgdl: f64,
    model: &str,
    dtype: &str,
    compress: &str,
) -> Result<()> {
    use nvs_core::manifest as m;
    let files = m::ManifestFiles {
        vectors: m::ManifestFilesEntry {
            path: format!("vectors.{}", dtype),
            dtype: Some(dtype.to_string()),
            rows: Some(n as u64),
            cols: Some(dim as u64),
            schema: None,
        },
        doclen: m::ManifestFilesEntry {
            path: "doclen.u32".into(),
            dtype: Some("u32".into()),
            rows: Some(n as u64),
            cols: None,
            schema: None,
        },
        lexicon: m::ManifestFilesEntry {
            path: "lexicon.bin".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: None,
        },
        postings: m::ManifestFilesEntry {
            path: "postings.bin".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: None,
        },
        terms: m::ManifestFilesEntry {
            path: "terms.dict".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: None,
        },
        meta_idx: m::ManifestFilesEntry {
            path: "meta.idx".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: Some("u32 block_id, u32 offset, u32 doc_size".into()),
        },
        meta: m::ManifestFilesMeta {
            path: "meta.blocks".into(),
            block_size: Some(block_size as u32),
            doc_aligned: Some(true),
            compression: if compress == "zstd" {
                Some("zstd".into())
            } else {
                None
            },
        },
    };
    let manifest = m::Manifest {
        format: "nvs.v1".into(),
        num_docs: n as u64,
        dim: dim as u64,
        embedding: m::ManifestEmbedding {
            model: model.into(),
            dtype: dtype.into(),
        },
        bm25: m::ManifestBm25 {
            avgdl,
            k1: 1.2,
            b: 0.75,
        },
        files,
    };
    let f = File::create(out.join("manifest.json"))?;
    let mut bw = BufWriter::new(f);
    serde_json::to_writer_pretty(&mut bw, &manifest)?;
    bw.flush()?;
    Ok(())
}

fn write_checksums(out: &Path) -> Result<()> {
    let candidates = [
        "manifest.json",
        "vectors.f32",
        "vectors.f16",
        "doclen.u32",
        "lexicon.bin",
        "postings.bin",
        "terms.dict",
        "meta.idx",
        "meta.blocks",
    ];
    let mut s = String::new();
    for name in candidates {
        let path = out.join(name);
        if path.exists() {
            let mut buf = Vec::new();
            File::open(&path)?.read_to_end(&mut buf)?;
            let h = xxh64(&buf, 0);
            s.push_str(&format!("{h:016x}  {name}\n"));
        }
    }
    let mut f = File::create(out.join("checksums.xxhash64"))?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

fn write_receipts(out: &Path, receipts: &[(String, usize)]) -> Result<()> {
    use std::io::Write as _;
    let mut f = File::create(out.join("receipts.txt"))?;
    for (name, count) in receipts.iter() {
        writeln!(f, "{}\t{}", name, count)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    fs::create_dir_all(&cli.out).context("create output dir")?;
    let pb = indicatif::ProgressBar::new_spinner();
    let style = indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap();
    pb.set_style(style);
    let t0 = std::time::Instant::now();
    let (docs, receipts) = if cli.fast_loader { read_docs_fast(&cli.input, cli.mmap_threshold)? } else { read_docs(&cli.input)? };
    anyhow::ensure!(!docs.is_empty(), "no documents found in input");
    let dim = docs[0].embedding.len();
    anyhow::ensure!(dim > 0, "embedding dimension must be > 0");
    // Filter out mismatched dimensions to be resilient
    let docs: Vec<Doc> = docs
        .into_iter()
        .filter(|d| d.embedding.len() == dim)
        .collect();
    anyhow::ensure!(
        !docs.is_empty(),
        "no valid documents with consistent dimension"
    );
    let docs_len = docs.len();
    let dtype = if cli.quantize == "f16" { "f16" } else { "f32" };
    let t_read = t0.elapsed();
    pb.set_message(format!("Writing vectors ({})...", dtype));
    let t1 = std::time::Instant::now();
    write_vectors(&docs, dim, &cli.out, dtype)?;
    let t_vec = t1.elapsed();
    // Sequential writer (pipeline removed)
    let lvl = cli.zstd_level.clamp(1, 22);
    pb.set_message("Building BM25 index...");
    let t2 = std::time::Instant::now();
    let (avgdl, terms, postings_entries, total_tokens, bm_stats) = write_bm25_and_terms(&docs, &cli.out, cli.bm25_buckets)?;
    let t_bm25 = t2.elapsed();
    let unique_terms = terms.len();
    pb.set_message("Writing metadata blocks...");
    let t3 = std::time::Instant::now();
    let block_count = write_meta_and_index(&docs, cli.block_size, &cli.out, &cli.compress, lvl, cli.meta_include_embeddings)?;
    let t_meta = t3.elapsed();
    pb.set_message("Writing manifest...");
    let t4 = std::time::Instant::now();
    write_manifest(
        &cli.out,
        docs_len,
        dim,
        cli.block_size,
        avgdl,
        &cli.model,
        dtype,
        &cli.compress,
    )?;
    let t_manifest = t4.elapsed();
    pb.set_message("Computing checksums...");
    let t5 = std::time::Instant::now();
    write_checksums(&cli.out)?;
    write_receipts(&cli.out, &receipts)?;
    let t_checksums = t5.elapsed();
    pb.finish_and_clear();

    let row_bytes = dim * if dtype == "f16" { 2 } else { 4 };
    let aligned = ((row_bytes + 63) / 64) * 64;
    let bundle_size: u64 = [
        "manifest.json",
        "vectors.f32",
        "vectors.f16",
        "doclen.u32",
        "lexicon.bin",
        "postings.bin",
        "terms.dict",
        "meta.idx",
        "meta.blocks",
    ]
    .iter()
    .filter_map(|name| {
        let p = cli.out.join(name);
        fs::metadata(&p).ok().map(|m| m.len())
    })
    .sum();
    // Allocated (physical) size on disk
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    #[cfg(unix)]
    let allocated_size: u64 = [
        "manifest.json",
        "vectors.f32",
        "vectors.f16",
        "doclen.u32",
        "lexicon.bin",
        "postings.bin",
        "terms.dict",
        "meta.idx",
        "meta.blocks",
    ]
    .iter()
    .filter_map(|name| {
        let p = cli.out.join(name);
        fs::metadata(&p).ok().map(|m| m.blocks() * 512)
    })
    .sum();
    #[cfg(not(unix))]
    let allocated_size: u64 = 0;
    println!(
        "{} {}",
        console::style("✔").green(),
        console::style("Bundle created").bold()
    );
    println!("  Output: {}", console::style(cli.out.display()).bold());
    println!("  Docs: {}  Tokens: {}", docs_len, total_tokens);
    println!("  Dim: {}  DType: {}", dim, dtype);
    println!(
        "  AvgDL: {:.2} tokens  Unique terms: {}  Postings: {}",
        avgdl, unique_terms, postings_entries
    );
    println!(
        "  Blocks: {}  Block size: {} bytes",
        block_count, cli.block_size
    );
    println!("  Vectors: rows={} stride={}B", docs_len, aligned);
    println!(
        "  Bundle size: {:.2} MB",
        (bundle_size as f64) / (1024.0 * 1024.0)
    );
    #[cfg(unix)]
    println!(
        "  Allocated size: {:.2} MB",
        (allocated_size as f64) / (1024.0 * 1024.0)
    );
    println!(
        "  Time: read {:?}  vectors {:?}  bm25 {:?}  bm25_tokenize {:?}  bm25_local {:?}  bm25_merge {:?}  bm25_write {:?}  meta {:?}  manifest {:?}  checksums {:?}",
        t_read, t_vec, t_bm25, bm_stats.tf, bm_stats.local, bm_stats.merge, bm_stats.write, t_meta, t_manifest, t_checksums
    );
    Ok(())
}
