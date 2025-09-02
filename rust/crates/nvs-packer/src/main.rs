use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    let tok = nvs_core::tokenizer::SimpleTokenizer::new();
    let mut doc_tokens: Vec<Vec<String>> = Vec::with_capacity(docs.len());
    let mut df_map: HashMap<String, usize> = HashMap::new();
    let mut postings_map: HashMap<String, Vec<(usize, u32)>> = HashMap::new();
    let mut total_tokens = 0usize;
    for (i, d) in docs.iter().enumerate() {
        let tokens = tok.split(&d.text);
        total_tokens += tokens.len();
        let mut tf: HashMap<&str, u32> = HashMap::new();
        for t in &tokens { *tf.entry(t.as_str()).or_insert(0) += 1; }
        for (term, &count) in tf.iter() { postings_map.entry((*term).to_string()).or_default().push((i, count)); }
        for term in tf.keys() { *df_map.entry((*term).to_string()).or_insert(0) += 1; }
        doc_tokens.push(tokens);
    }
    // doclen
    {
        let mut f = File::create(out.join("doclen.u32"))?; for tokens in &doc_tokens { let len=tokens.len() as u32; f.write_all(&len.to_le_bytes())?; }
    }
    // terms sorted
    let mut terms: Vec<String> = postings_map.keys().cloned().collect(); terms.sort();
    {
        let mut f = File::create(out.join("terms.dict"))?; for t in &terms { let len=t.len() as u32; f.write_all(&len.to_le_bytes())?; f.write_all(t.as_bytes())?; }
    }
    // postings + lexicon
    {
        let mut postings = Vec::<u8>::new(); let mut lexicon = Vec::<u8>::new(); let mut offset: u64 = 0;
        for t in &terms {
            let mut list = postings_map.get(t).cloned().unwrap_or_default(); list.sort_by_key(|&(doc, _)| doc);
            let mut prev = 0usize; let mut length = 0u32;
            for (doc, tf) in list.into_iter() { let delta = (doc - prev) as u32; prev = doc; length += 1; postings.extend_from_slice(&delta.to_le_bytes()); postings.extend_from_slice(&tf.to_le_bytes()); }
            let df = *df_map.get(t).unwrap_or(&0) as u32; lexicon.extend_from_slice(&offset.to_le_bytes()); lexicon.extend_from_slice(&length.to_le_bytes()); lexicon.extend_from_slice(&df.to_le_bytes()); offset += (length as u64) * 8;
        }
        let mut pf = File::create(out.join("postings.bin"))?; pf.write_all(&postings)?; let mut lf = File::create(out.join("lexicon.bin"))?; lf.write_all(&lexicon)?;
    }
    let avgdl = if docs.is_empty() { 0.0 } else { (total_tokens as f64) / (docs.len() as f64) };
    let postings_entries: usize = postings_map.values().map(|v| v.len()).sum();
    Ok((avgdl, terms, postings_entries, total_tokens))
}

fn write_meta_and_index(docs: &[Doc], block_size: usize, out: &Path) -> Result<usize> {
    let mut blocks: Vec<Vec<u8>> = Vec::new(); let mut headers: Vec<(u32,u32,u32,u32)> = Vec::new(); let mut idx: Vec<u8> = Vec::new();
    let mut cur = Vec::<u8>::with_capacity(block_size); let mut cur_usize=0u32; let mut cur_docs=0u32; let mut block_id=0u32;
    for d in docs {
        let meta_json = format!("{{\"embedding\":[{}]}}", d.embedding.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(","));
        let rec_size = 4 + d.id.len() + 4 + d.text.len() + 4 + meta_json.len();
        if cur_docs>0 && cur_usize as usize + rec_size > block_size { headers.push((block_id, cur_usize, cur_docs, 0)); blocks.push(std::mem::take(&mut cur)); cur = Vec::with_capacity(block_size); cur_usize=0; cur_docs=0; block_id+=1; }
        // idx entry
        idx.extend_from_slice(&block_id.to_le_bytes()); idx.extend_from_slice(&(cur_usize).to_le_bytes()); idx.extend_from_slice(&((rec_size as u32)).to_le_bytes()); idx.extend_from_slice(&0u32.to_le_bytes());
        // write record
        cur.extend_from_slice(&(d.id.len() as u32).to_le_bytes()); cur.extend_from_slice(d.id.as_bytes());
        cur.extend_from_slice(&(d.text.len() as u32).to_le_bytes()); cur.extend_from_slice(d.text.as_bytes());
        cur.extend_from_slice(&(meta_json.len() as u32).to_le_bytes()); cur.extend_from_slice(meta_json.as_bytes());
        cur_usize += rec_size as u32; cur_docs += 1;
    }
    if cur_docs>0 { headers.push((block_id, cur_usize, cur_docs, 0)); blocks.push(cur); }
    // meta.blocks
    {
        let mut f = File::create(out.join("meta.blocks"))?; f.write_all(&(headers.len() as u32).to_le_bytes())?;
        for (id, usizeb, dcount, pad) in &headers { f.write_all(&id.to_le_bytes())?; f.write_all(&usizeb.to_le_bytes())?; f.write_all(&dcount.to_le_bytes())?; f.write_all(&pad.to_le_bytes())?; }
        for b in &blocks { f.write_all(&b)?; if b.len()<block_size { f.write_all(&vec![0u8; block_size - b.len()])?; } }
    }
    // meta.idx
    { let mut f = File::create(out.join("meta.idx"))?; f.write_all(&idx)?; }
    Ok(headers.len())
}

fn write_manifest(out: &Path, n: usize, dim: usize, block_size: usize, avgdl: f64, model: &str, dtype: &str) -> Result<()> {
    let manifest = format!(
        r#"{{
  "format": "nvs.v1",
  "num_docs": {},
  "dim": {},
  "embedding": {{"model": "{}", "dtype": "{}"}},
  "bm25": {{"avgdl": {}, "k1": 1.2, "b": 0.75}},
  "files": {{
    "vectors": {{"path": "vectors.{}", "dtype": "{}", "rows": {}, "cols": {}}},
    "doclen": {{"path": "doclen.u32", "dtype": "u32", "rows": {}}},
    "lexicon": {{"path": "lexicon.bin"}},
    "postings": {{"path": "postings.bin"}},
    "terms": {{"path": "terms.dict"}},
    "meta_idx": {{"path": "meta.idx", "schema": "u32 block_id, u32 offset, u32 doc_size"}},
    "meta": {{"path": "meta.blocks", "block_size": {}, "doc_aligned": true}}
  }}
}}"#,
        n, dim, model, dtype, avgdl, dtype, dtype, n, dim, n, block_size
    );
    let mut f = File::create(out.join("manifest.json"))?; f.write_all(manifest.as_bytes())?; Ok(())
}

fn write_checksums(out: &Path) -> Result<()> {
    let files = [
        "manifest.json","vectors.f32","doclen.u32","lexicon.bin","postings.bin","terms.dict","meta.idx","meta.blocks"
    ];
    let mut s = String::new();
    for name in files {
        let path = out.join(name); let mut buf=Vec::new(); File::open(&path)?.read_to_end(&mut buf)?; let h = xxh64(&buf, 0);
        s.push_str(&format!("{h:016x}  {name}\n"));
    }
    let mut f = File::create(out.join("checksums.xxhash64"))?; f.write_all(s.as_bytes())?; Ok(())
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
    let mut docs: Vec<Doc> = docs.into_iter().filter(|d| d.embedding.len() == dim).collect();
    anyhow::ensure!(!docs.is_empty(), "no valid documents with consistent dimension");
    let dtype = if cli.quantize == "f16" { "f16" } else { "f32" };
    let t_read = t0.elapsed();
    pb.set_message(format!("Writing vectors ({})...", dtype));
    let t1 = std::time::Instant::now();
    write_vectors(&docs, dim, &cli.out, dtype)?;
    let t_vec = t1.elapsed();
    pb.set_message("Building BM25 index...");
    let t2 = std::time::Instant::now();
    let (avgdl, terms, postings_entries, total_tokens) = write_bm25_and_terms(&docs, &cli.out)?;
    let t_bm25 = t2.elapsed();
    pb.set_message("Writing metadata blocks...");
    let t3 = std::time::Instant::now();
    let block_count = write_meta_and_index(&docs, cli.block_size, &cli.out)?;
    let t_meta = t3.elapsed();
    pb.set_message("Writing manifest...");
    let t4 = std::time::Instant::now();
    write_manifest(&cli.out, docs.len(), dim, cli.block_size, avgdl, &cli.model, dtype)?;
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
    println!("{} {}", console::style("✔").green(), console::style("Bundle created").bold());
    println!("  Output: {}", console::style(cli.out.display()).bold());
    println!("  Docs: {}  Tokens: {}", docs.len(), total_tokens);
    println!("  Dim: {}  DType: {}", dim, dtype);
    println!("  AvgDL: {:.2} tokens  Unique terms: {}  Postings: {}", avgdl, terms.len(), postings_entries);
    println!("  Blocks: {}  Block size: {} bytes", block_count, cli.block_size);
    println!("  Vectors: rows={} stride={}B", docs.len(), aligned);
    println!("  Bundle size: {:.2} MB", (bundle_size as f64) / (1024.0*1024.0));
    println!("  Time: read {:?}  vectors {:?}  bm25 {:?}  meta {:?}  manifest {:?}  checksums {:?}", t_read, t_vec, t_bm25, t_meta, t_manifest, t_checksums);
    Ok(())
}
