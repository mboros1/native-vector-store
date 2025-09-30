use anyhow::{Context, Result};
use clap::Parser;
use std::fs;

mod bm25;
mod cli;
mod loader;
mod writer;

use crate::bm25::write_bm25_and_terms;
use crate::cli::Cli;
use crate::loader::{read_docs, read_docs_fast, Doc};
use crate::writer::{
    write_checksums, write_manifest, write_meta_and_index, write_receipts, write_vectors,
};

fn main() -> Result<()> {
    let cli = Cli::parse();
    fs::create_dir_all(&cli.out).context("create output dir")?;
    let pb = indicatif::ProgressBar::new_spinner();
    let style = indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")?;
    pb.set_style(style);

    let t0 = std::time::Instant::now();
    let (docs, receipts) = if cli.fast_loader {
        read_docs_fast(&cli.input, cli.mmap_threshold)?
    } else {
        read_docs(&cli.input)?
    };
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

    // Remove stale opposite dtype vectors file to avoid double-counting size and confusion
    if dtype == "f16" {
        let _ = fs::remove_file(cli.out.join("vectors.f32"));
    } else {
        let _ = fs::remove_file(cli.out.join("vectors.f16"));
    }

    // Sequential writer (pipeline removed)
    let lvl = cli.zstd_level.clamp(1, 22);
    pb.set_message("Building BM25 index...");
    let t2 = std::time::Instant::now();
    let (avgdl, terms, postings_entries, total_tokens, bm_stats) =
        write_bm25_and_terms(&docs, &cli.out, cli.bm25_buckets)?;
    let t_bm25 = t2.elapsed();
    let unique_terms = terms.len();

    pb.set_message("Writing metadata blocks...");
    let t3 = std::time::Instant::now();
    let block_count = write_meta_and_index(
        &docs,
        cli.block_size,
        &cli.out,
        &cli.compress,
        lvl,
        cli.meta_include_embeddings,
    )?;
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
    let vec_name = if dtype == "f16" { "vectors.f16" } else { "vectors.f32" };
    let bundle_size: u64 = [
        "manifest.json",
        vec_name,
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
        vec_name,
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
