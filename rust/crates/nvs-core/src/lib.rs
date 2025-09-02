//! nvs-core: Rust reader for Native Vector Store bundles
//!
//! Parity focus:
//! - Validates manifest and block-aligned metadata per MANIFEST_SPEC.md
//! - Validates meta.idx entry count matches num_docs
//! - Future: mmap-backed reader and search APIs

pub mod errors;
pub mod manifest;
pub mod bundle;

pub use bundle::Bundle;
