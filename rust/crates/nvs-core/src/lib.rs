//! nvs-core: Rust reader for Native Vector Store bundles
//!
//! Parity focus:
//! - Validates manifest and block-aligned metadata per MANIFEST_SPEC.md
//! - Validates meta.idx entry count matches num_docs
//! - Future: mmap-backed reader and search APIs

pub mod errors;
pub mod manifest;
pub mod bundle;
pub mod simd;
pub mod search;
pub mod tokenizer;
pub mod bm25;
pub mod hybrid;
pub mod vector_store;
pub mod english_stop_words;
pub mod english_abbreviations;
pub mod english_punctuations;

pub use bundle::Bundle;
pub use vector_store::VectorStore;
