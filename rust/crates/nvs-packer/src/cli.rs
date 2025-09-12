use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "nvs-packer")]
#[command(about = "Pack JSON docs into a native-vector-store bundle", long_about = None)]
pub struct Cli {
    /// Input directory containing JSON files (expects docs.json array by default)
    pub input: PathBuf,
    /// Output directory (created if missing)
    #[arg(short = 'o', long = "out", default_value = "./nvs-bundle")]
    pub out: PathBuf,
    /// Metadata block size in bytes
    #[arg(long = "block-size", default_value_t = 131072)]
    pub block_size: usize,
    /// Embedding model name for manifest
    #[arg(long = "model", default_value = "unknown")]
    pub model: String,
    /// Output vector dtype: f32 (default) or f16
    #[arg(long = "quantize", value_parser = ["f16", "f32"], default_value = "f32")]
    pub quantize: String,
    /// Compress metadata blocks: none (default) or zstd
    #[arg(long = "compress", value_parser = ["none", "zstd"], default_value = "none")]
    pub compress: String,
    /// Zstd compression level (1-22), used when --compress=zstd
    #[arg(long = "zstd-level", default_value_t = 3)]
    pub zstd_level: i32,
    /// Include embeddings in meta.blocks JSON (defaults to false to avoid duplication)
    #[arg(long = "meta-include-embeddings", default_value_t = false)]
    pub meta_include_embeddings: bool,
    /// Use fast adaptive JSON loader (mmap small files, parallel consumers, streaming arrays)
    #[arg(long = "fast-loader", default_value_t = false)]
    pub fast_loader: bool,
    /// Threshold in bytes below which JSON files are mmapped (used with --fast-loader)
    #[arg(long = "mmap-threshold", default_value_t = 5_000_000)]
    pub mmap_threshold: usize,
    /// Parallel BM25 merge buckets (0=auto, recommend 16-32)
    #[arg(long = "bm25-buckets", default_value_t = 0)]
    pub bm25_buckets: usize,
}

