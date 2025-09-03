use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::fs::{self, File};
use std::io::{Read, Write, BufWriter};
use std::path::{Path, PathBuf};
use xxhash_rust::xxh64::xxh64;
use dashmap::DashMap;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

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
    // Pipeline removed: sequential flow only
}

#[derive(Deserialize)]
struct InputDocMeta { embedding: Vec<f32> }

#[derive(Deserialize)]
struct InputDocRaw {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    metadata: Option<InputDocMeta>,
}

#[derive(Clone)]
struct Doc { id: String, text: String, embedding: Vec<f32> }

fn read_docs(input_dir: &Path) -> Result<Vec<Doc>> {
    use walkdir::WalkDir;
    let mut docs = Vec::new();
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(indicatif::ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
    pb.set_message("Scanning JSON files...");
    let mut _total = 0usize; let mut skipped = 0usize;
    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().map(|e| e == "json").unwrap_or(false) {
            let path = entry.path();
            pb.set_message(format!("Reading {}", path.display()));
            let mut s = String::new();
            File::open(path).with_context(|| format!("open {}", path.display()))?.read_to_string(&mut s)?;
            // Try array first
            if s.trim_start().starts_with('[') {
                let arr: Vec<serde_json::Value> = serde_json::from_str(&s).with_context(|| format!("parse array in {}", path.display()))?;
                for (i, v) in arr.into_iter().enumerate() {
                    _total += 1;
                    match serde_json::from_value::<InputDocRaw>(v) {
                        Ok(r) => {
                            let text = r.text.or(r.content).unwrap_or_default();
                            match r.metadata {
                                Some(m) if !m.embedding.is_empty() => {
                                    let id = r.id.unwrap_or_else(|| {
                                        let h = xxh64(text.as_bytes(), 0) ^ (i as u64);
                                        format!("doc-{h:016x}")
                                    });
                                    docs.push(Doc { id, text, embedding: m.embedding });
                                }
                                _ => { skipped += 1; eprintln!("{} skipping doc without embedding ({}:#{})", console::style("! ").yellow(), path.display(), i); }
                            }
                        }
                        Err(e) => { skipped += 1; eprintln!("{} skipping invalid doc ({}:#{}) — {}", console::style("! ").yellow(), path.display(), i, e); }
                    }
                }
            } else {
                _total += 1;
                match serde_json::from_str::<InputDocRaw>(&s) {
                    Ok(r) => {
                        let text = r.text.or(r.content).unwrap_or_default();
                        match r.metadata {
                            Some(m) if !m.embedding.is_empty() => {
                                let id = r.id.unwrap_or_else(|| {
                                    let h = xxh64(text.as_bytes(), 0);
                                    format!("doc-{h:016x}")
                                });
                                docs.push(Doc { id, text, embedding: m.embedding });
                            }
                            _ => { skipped += 1; eprintln!("{} skipping doc without embedding ({})", console::style("! ").yellow(), path.display()); }
                        }
                    }
                    Err(e) => { skipped += 1; eprintln!("{} skipping invalid doc ({}) — {}", console::style("! ").yellow(), path.display(), e); }
                }
            }
        }
    }
    pb.finish_with_message(format!("Loaded {} docs (skipped {})", docs.len(), skipped));
    Ok(docs)
}

fn write_vectors(docs: &[Doc], dim: usize, out: &Path, dtype: &str) -> Result<()> {
    match dtype {
        "f16" => {
            use half::f16;
            let row_bytes = dim * 2; let aligned=((row_bytes + 63)/64)*64; let mut data = vec![0u8; docs.len()*aligned];
            for (i, d) in docs.iter().enumerate() {
                anyhow::ensure!(d.embedding.len()==dim, "dimension mismatch for doc {}", d.id);
                for j in 0..dim { let off = i*aligned + j*2; let h = f16::from_f32(d.embedding[j]); data[off..off+2].copy_from_slice(&h.to_le_bytes()); }
            }
            let mut f = File::create(out.join("vectors.f16"))?; f.write_all(&data)?; Ok(())
        }
        _ => {
            let row_bytes = dim * 4; let aligned=((row_bytes + 63)/64)*64; let mut data = vec![0u8; docs.len()*aligned];
            for (i, d) in docs.iter().enumerate() { anyhow::ensure!(d.embedding.len()==dim, "dimension mismatch for doc {}", d.id); for j in 0..dim { let off = i*aligned + j*4; data[off..off+4].copy_from_slice(&d.embedding[j].to_le_bytes()); } }
            let mut f = File::create(out.join("vectors.f32"))?; f.write_all(&data)?; Ok(())
        }
    }
}

fn write_bm25_and_terms(docs: &[Doc], out: &Path) -> Result<(f64, Vec<String>, usize, usize)> {
    use rayon::prelude::*;
    // Streaming reducer with concurrent postings and per-doc lengths
    let postings_map: Arc<DashMap<String, Vec<(usize, u32)>>> = Arc::new(DashMap::new());
    let doc_lens: Vec<AtomicUsize> = (0..docs.len()).map(|_| AtomicUsize::new(0)).collect();

    docs.par_iter().enumerate().for_each(|(i, d)| {
        let tok = nvs_core::tokenizer::SimpleTokenizer::new();
        let tokens = tok.split(&d.text);
        doc_lens[i].store(tokens.len(), Ordering::Relaxed);
        let mut tf: FxHashMap<String, u32> = FxHashMap::default();
        for t in tokens.into_iter() { *tf.entry(t).or_insert(0) += 1; }
        for (term, count) in tf.into_iter() {
            let mut v = postings_map.entry(term).or_default();
            v.push((i, count));
        }
    });

    // Write doc lengths
    {
        let mut f = File::create(out.join("doclen.u32"))?;
        for len in &doc_lens { let v = len.load(Ordering::Relaxed) as u32; f.write_all(&v.to_le_bytes())?; }
    }

    let total_tokens: usize = doc_lens.iter().map(|x| x.load(Ordering::Relaxed)).sum();

    // terms sorted
    let mut terms: Vec<String> = postings_map.iter().map(|e| e.key().clone()).collect();
    terms.sort();
    {
        let mut f = File::create(out.join("terms.dict"))?;
        for t in &terms { let len=t.len() as u32; f.write_all(&len.to_le_bytes())?; f.write_all(t.as_bytes())?; }
    }
    // postings + lexicon (deterministic)
    {
        let mut postings = Vec::<u8>::new(); let mut lexicon = Vec::<u8>::new(); let mut offset: u64 = 0;
        for t in &terms {
            if let Some(entry) = postings_map.get(t) {
                let mut list = entry.clone();
                drop(entry);
                list.sort_by_key(|&(doc, _)| doc);
                let mut prev = 0usize; let mut length = 0u32;
                for (doc, tf) in list.into_iter() {
                    let delta = (doc - prev) as u32; prev = doc; length += 1;
                    postings.extend_from_slice(&delta.to_le_bytes()); postings.extend_from_slice(&tf.to_le_bytes());
                }
                let df = length;
                lexicon.extend_from_slice(&offset.to_le_bytes()); lexicon.extend_from_slice(&length.to_le_bytes()); lexicon.extend_from_slice(&df.to_le_bytes());
                offset += (length as u64) * 8;
            }
        }
        let mut pf = File::create(out.join("postings.bin"))?; pf.write_all(&postings)?; let mut lf = File::create(out.join("lexicon.bin"))?; lf.write_all(&lexicon)?;
    }
    let avgdl = if docs.is_empty() { 0.0 } else { (total_tokens as f64) / (docs.len() as f64) };
    let postings_entries: usize = postings_map.iter().map(|e| e.value().len()).sum();
    Ok((avgdl, terms, postings_entries, total_tokens))
}

fn write_meta_and_index(docs: &[Doc], block_size: usize, out: &Path, compress: &str, zstd_level: i32) -> Result<usize> {
    let mut blocks: Vec<Vec<u8>> = Vec::new(); let mut headers: Vec<(u32,u32,u32,u32)> = Vec::new(); let mut idx: Vec<u8> = Vec::new();
    let mut cur = Vec::<u8>::with_capacity(block_size); let mut cur_usize=0u32; let mut cur_docs=0u32; let mut block_id=0u32;
    #[derive(Serialize)]
    struct Meta<'a> { embedding: &'a [f32] }

    // Lightweight counter to measure JSON length without allocating a buffer
    struct CountWriter { count: usize }
    impl CountWriter { fn new() -> Self { Self { count: 0 } } }
    impl Write for CountWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> { self.count += buf.len(); Ok(buf.len()) }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }

    for d in docs {
        let meta = Meta { embedding: &d.embedding };
        // First pass: count JSON bytes to determine record size and block fit
        let mut cw = CountWriter::new();
        serde_json::to_writer(&mut cw, &meta)?;
        let meta_len = cw.count;

        let rec_size = 4 + d.id.len() + 4 + d.text.len() + 4 + meta_len;
        if cur_docs>0 && (cur_usize as usize + rec_size) > block_size {
            headers.push((block_id, cur_usize, cur_docs, 0));
            blocks.push(std::mem::take(&mut cur));
            cur = Vec::with_capacity(block_size);
            cur_usize = 0;
            cur_docs = 0;
            block_id += 1;
        }
        // idx entry
        idx.extend_from_slice(&block_id.to_le_bytes());
        idx.extend_from_slice(&(cur_usize).to_le_bytes());
        idx.extend_from_slice(&((rec_size as u32)).to_le_bytes());
        idx.extend_from_slice(&0u32.to_le_bytes());
        // write record directly into current block buffer
        cur.extend_from_slice(&(d.id.len() as u32).to_le_bytes());
        cur.extend_from_slice(d.id.as_bytes());
        cur.extend_from_slice(&(d.text.len() as u32).to_le_bytes());
        cur.extend_from_slice(d.text.as_bytes());
        // Reserve space for meta length, then serialize JSON directly and back-patch length
        let len_pos = cur.len();
        cur.extend_from_slice(&0u32.to_le_bytes());
        let start = cur.len();
        serde_json::to_writer(&mut cur, &meta)?;
        let written = cur.len() - start;
        debug_assert_eq!(written, meta_len, "meta length changed between count and write");
        let meta_len_le = (written as u32).to_le_bytes();
        cur[len_pos..len_pos+4].copy_from_slice(&meta_len_le);

        cur_usize += rec_size as u32; cur_docs += 1;
    }
    if cur_docs>0 { headers.push((block_id, cur_usize, cur_docs, 0)); blocks.push(cur); }
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
                    let compressed = zstd::bulk::compress(b, zstd_level).unwrap_or_else(|_| b.clone());
                    (compressed, decomp_len, 1u32)
                })
                .collect()
        } else {
            blocks.iter().map(|b| (b.clone(), b.len() as u32, 0u32)).collect()
        };

        let mut f = File::create(out.join("meta.blocks"))?; f.write_all(&((comp.len() as u32)).to_le_bytes())?;
    // Write headers: (comp_size, decomp_size, doc_count, codec)
        for (i, (bytes, decomp_len, cod)) in comp.iter().enumerate() {
            let comp_size = bytes.len() as u32;
            let dcount = headers.get(i).map(|h| h.2).unwrap_or(0);
            let codec = if *cod == 1 { 1u32 } else { 0u32 };
            // If compressed size overflows block_size, fallback: write uncompressed later and mark codec=0
            let final_comp_size = if comp_size as usize > block_size { *decomp_len } else { comp_size };
            let final_codec = if comp_size as usize > block_size { 0u32 } else { codec };
            f.write_all(&final_comp_size.to_le_bytes())?;
            f.write_all(&decomp_len.to_le_bytes())?;
            f.write_all(&dcount.to_le_bytes())?;
            f.write_all(&final_codec.to_le_bytes())?;
        }
        // Write block payloads padded to block_size
        // Reusable padding buffer
        let pad = vec![0u8; block_size];
        for (i, (bytes, _decomp_len, cod)) in comp.into_iter().enumerate() {
            let use_comp = if bytes.len() > block_size { false } else { cod == 1 };
            if use_comp {
                f.write_all(&bytes)?;
                if bytes.len() < block_size { let need = block_size - bytes.len(); f.write_all(&pad[..need])?; }
            } else {
                // write original uncompressed block
                let b = &blocks[i];
                f.write_all(b)?;
                if b.len() < block_size { let need = block_size - b.len(); f.write_all(&pad[..need])?; }
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

fn write_manifest(out: &Path, n: usize, dim: usize, block_size: usize, avgdl: f64, model: &str, dtype: &str, compress: &str) -> Result<()> {
    use nvs_core::manifest as m;
    let files = m::ManifestFiles {
        vectors: m::ManifestFilesEntry { path: format!("vectors.{}", dtype), dtype: Some(dtype.to_string()), rows: Some(n as u64), cols: Some(dim as u64), schema: None },
        doclen: m::ManifestFilesEntry { path: "doclen.u32".into(), dtype: Some("u32".into()), rows: Some(n as u64), cols: None, schema: None },
        lexicon: m::ManifestFilesEntry { path: "lexicon.bin".into(), dtype: None, rows: None, cols: None, schema: None },
        postings: m::ManifestFilesEntry { path: "postings.bin".into(), dtype: None, rows: None, cols: None, schema: None },
        terms: m::ManifestFilesEntry { path: "terms.dict".into(), dtype: None, rows: None, cols: None, schema: None },
        meta_idx: m::ManifestFilesEntry { path: "meta.idx".into(), dtype: None, rows: None, cols: None, schema: Some("u32 block_id, u32 offset, u32 doc_size".into()) },
        meta: m::ManifestFilesMeta { path: "meta.blocks".into(), block_size: Some(block_size as u32), doc_aligned: Some(true), compression: if compress == "zstd" { Some("zstd".into()) } else { None } },
    };
    let manifest = m::Manifest {
        format: "nvs.v1".into(),
        num_docs: n as u64,
        dim: dim as u64,
        embedding: m::ManifestEmbedding { model: model.into(), dtype: dtype.into() },
        bm25: m::ManifestBm25 { avgdl, k1: 1.2, b: 0.75 },
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    fs::create_dir_all(&cli.out).context("create output dir")?;
    let pb = indicatif::ProgressBar::new_spinner();
    let style = indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap();
    pb.set_style(style);
    let t0 = std::time::Instant::now();
    let docs = read_docs(&cli.input)?;
    anyhow::ensure!(!docs.is_empty(), "no documents found in input");
    let dim = docs[0].embedding.len();
    anyhow::ensure!(dim>0, "embedding dimension must be > 0");
    // Filter out mismatched dimensions to be resilient
    let docs: Vec<Doc> = docs.into_iter().filter(|d| d.embedding.len() == dim).collect();
    anyhow::ensure!(!docs.is_empty(), "no valid documents with consistent dimension");
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
    let (avgdl, terms, postings_entries, total_tokens) = write_bm25_and_terms(&docs, &cli.out)?;
    let t_bm25 = t2.elapsed();
    let unique_terms = terms.len();
    pb.set_message("Writing metadata blocks...");
    let t3 = std::time::Instant::now();
    let block_count = write_meta_and_index(&docs, cli.block_size, &cli.out, &cli.compress, lvl)?;
    let t_meta = t3.elapsed();
    pb.set_message("Writing manifest...");
    let t4 = std::time::Instant::now();
    write_manifest(&cli.out, docs_len, dim, cli.block_size, avgdl, &cli.model, dtype, &cli.compress)?;
    let t_manifest = t4.elapsed();
    pb.set_message("Computing checksums...");
    let t5 = std::time::Instant::now();
    write_checksums(&cli.out)?;
    let t_checksums = t5.elapsed();
    pb.finish_and_clear();

    let row_bytes = dim * if dtype=="f16" { 2 } else { 4 };
    let aligned = ((row_bytes + 63)/64)*64;
    let bundle_size: u64 = [
        "manifest.json","vectors.f32","vectors.f16","doclen.u32","lexicon.bin","postings.bin","terms.dict","meta.idx","meta.blocks"
    ].iter().filter_map(|name| { let p = cli.out.join(name); fs::metadata(&p).ok().map(|m| m.len()) }).sum();
    // Allocated (physical) size on disk
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    #[cfg(unix)]
    let allocated_size: u64 = [
        "manifest.json","vectors.f32","vectors.f16","doclen.u32","lexicon.bin","postings.bin","terms.dict","meta.idx","meta.blocks"
    ].iter().filter_map(|name| { let p = cli.out.join(name); fs::metadata(&p).ok().map(|m| m.blocks() * 512) }).sum();
    #[cfg(not(unix))]
    let allocated_size: u64 = 0;
    println!("{} {}", console::style("✔").green(), console::style("Bundle created").bold());
    println!("  Output: {}", console::style(cli.out.display()).bold());
    println!("  Docs: {}  Tokens: {}", docs_len, total_tokens);
    println!("  Dim: {}  DType: {}", dim, dtype);
    println!("  AvgDL: {:.2} tokens  Unique terms: {}  Postings: {}", avgdl, unique_terms, postings_entries);
    println!("  Blocks: {}  Block size: {} bytes", block_count, cli.block_size);
    println!("  Vectors: rows={} stride={}B", docs_len, aligned);
    println!("  Bundle size: {:.2} MB", (bundle_size as f64) / (1024.0*1024.0));
    #[cfg(unix)]
    println!("  Allocated size: {:.2} MB", (allocated_size as f64) / (1024.0*1024.0));
    println!("  Time: read {:?}  vectors {:?}  bm25 {:?}  meta {:?}  manifest {:?}  checksums {:?}", t_read, t_vec, t_bm25, t_meta, t_manifest, t_checksums);
    Ok(())
}
