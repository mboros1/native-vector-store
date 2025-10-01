//! Native Vector Store — Rust core.
//!
//! - Open read-only bundles (manifest + vectors + metadata + BM25 index)
//! - Vector, BM25, and hybrid search
//! - Fetch documents with structured JSON metadata
//!
//! Quick start:
//!
//! ```no_run
//! use nvs_core::VectorStore;
//!
//! let store = VectorStore::open("/path/to/bundle")?;
//! let hits = store.search_hybrid(&vec![0.0, 1.0, 0.0], "keywords", 5, 0.6);
//! let ids: Vec<u32> = hits.iter().map(|(id, _)| *id).collect();
//! let docs = store.get_documents(&ids);
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod bm25;
pub mod bundle;
pub mod errors;
pub mod hybrid;
pub mod manifest;
pub mod search;
pub mod simd;
pub use crate::bm25::tokenizer;
pub mod chunker;
pub mod vector_store;

pub use bundle::Bundle;
pub use vector_store::{Document, VectorStore};
pub use crate::simd::{dot, dot_f32_f16};
