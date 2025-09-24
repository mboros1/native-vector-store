use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFilesMeta {
    pub path: String,
    #[serde(default)]
    pub block_size: Option<u32>,
    #[serde(default)]
    pub doc_aligned: Option<bool>,
    #[serde(default)]
    pub compression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFilesEntry {
    pub path: String,
    #[serde(default)]
    pub dtype: Option<String>,
    #[serde(default)]
    pub rows: Option<u64>,
    #[serde(default)]
    pub cols: Option<u64>,
    #[serde(default)]
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFiles {
    pub vectors: ManifestFilesEntry,
    pub doclen: ManifestFilesEntry,
    pub lexicon: ManifestFilesEntry,
    pub postings: ManifestFilesEntry,
    pub terms: ManifestFilesEntry,
    #[serde(rename = "meta_idx")]
    pub meta_idx: ManifestFilesEntry,
    #[serde(rename = "meta")]
    pub meta: ManifestFilesMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEmbedding {
    pub model: String,
    pub dtype: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestBm25 {
    pub avgdl: f64,
    pub k1: f64,
    pub b: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub num_docs: u64,
    pub dim: u64,
    pub embedding: ManifestEmbedding,
    pub bm25: ManifestBm25,
    pub files: ManifestFiles,
}
