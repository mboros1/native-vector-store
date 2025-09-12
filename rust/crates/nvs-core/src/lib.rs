//! nvs-core: Rust reader for Native Vector Store bundles
//!
//! Parity focus:
//! - Validates manifest and block-aligned metadata per MANIFEST_SPEC.md
//! - Validates meta.idx entry count matches num_docs
//! - Future: mmap-backed reader and search APIs

pub mod bm25;
pub mod bundle;
pub mod english_abbreviations;
pub mod english_punctuations;
pub mod english_stop_words;
pub mod errors;
pub mod hybrid;
pub mod manifest;
pub mod search;
pub mod simd;
pub mod tokenizer;
pub mod vector_store;
pub mod chunker;

pub use bundle::Bundle;
pub use vector_store::VectorStore;
