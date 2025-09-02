use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::errors::*;
use crate::manifest::Manifest;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct MetaIdxEntry {
    block_id: u32,
    offset_in_block: u32,
    doc_size: u32,
    padding: u32,
}

const META_IDX_ENTRY_SIZE: usize = std::mem::size_of::<MetaIdxEntry>();

#[derive(Debug)]
pub struct Bundle {
    root: PathBuf,
    pub manifest: Manifest,
    pub meta_block_size: u32,
    pub meta_block_count: u32,
}

impl Bundle {
    pub fn open<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        // load manifest
        let manifest_path = root.join("manifest.json");
        let mut s = String::new();
        File::open(&manifest_path)?.read_to_string(&mut s)?;
        let manifest: Manifest = serde_json::from_str(&s)?;

        if manifest.format != "nvs.v1" {
            return Err(NvsError::InvalidManifest("unsupported format"));
        }
        if manifest.num_docs == 0 {
            return Err(NvsError::InvalidManifest("num_docs must be > 0"));
        }
        if manifest.dim == 0 {
            return Err(NvsError::InvalidManifest("dim must be > 0"));
        }

        // Validate meta.idx count == num_docs
        let meta_idx_path = root.join(&manifest.files.meta_idx.path);
        let meta_idx_md = fs::metadata(&meta_idx_path)?;
        let sz = meta_idx_md.len() as usize;
        if sz % META_IDX_ENTRY_SIZE != 0 {
            return Err(NvsError::InvalidBundle("meta.idx not aligned to entry size"));
        }
        let count = sz / META_IDX_ENTRY_SIZE;
        if count as u64 != manifest.num_docs {
            return Err(NvsError::InvalidBundle("meta.idx entry count mismatch"));
        }

        // Validate meta.blocks header and derive block_size
        let meta_blocks_path = root.join(&manifest.files.meta.path);
        let mut f = File::open(&meta_blocks_path)?;
        let mut u32buf = [0u8; 4];
        // read block_count
        f.read_exact(&mut u32buf)?;
        let block_count = u32::from_le_bytes(u32buf);
        if block_count == 0 {
            return Err(NvsError::InvalidBundle("block_count must be > 0"));
        }
        // header size = 4 + block_count * 16
        let header_size = 4u64 + (block_count as u64) * 16u64;
        let total_size = fs::metadata(&meta_blocks_path)?.len();
        if total_size <= header_size {
            return Err(NvsError::InvalidBundle("meta.blocks too small for headers"));
        }
        let remaining = total_size - header_size;
        if remaining % (block_count as u64) != 0 {
            return Err(NvsError::InvalidBundle("meta.blocks data not divisible by block_count"));
        }
        let derived_block = (remaining / (block_count as u64)) as u32;

        if let Some(bsz) = manifest.files.meta.block_size {
            if bsz != derived_block {
                return Err(NvsError::InvalidBundle("manifest block_size mismatch"));
            }
        }

        Ok(Self {
            root,
            manifest,
            meta_block_size: derived_block,
            meta_block_count: block_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let p = std::env::temp_dir().join(format!("{}_{}", prefix, ts));
        let _ = fs::create_dir_all(&p);
        p
    }

    fn write_manifest(root: &Path, num_docs: u64, dim: u64, block_size: u32) {
        let manifest = format!(
            r#"{{
  "format": "nvs.v1",
  "num_docs": {},
  "dim": {},
  "embedding": {{"model": "test", "dtype": "f32"}},
  "bm25": {{"avgdl": 1.0, "k1": 1.2, "b": 0.75}},
  "files": {{
    "vectors": {{"path": "vectors.f32", "dtype": "f32", "rows": {}, "cols": {}}},
    "doclen": {{"path": "doclen.u32", "dtype": "u32", "rows": {}}},
    "lexicon": {{"path": "lexicon.bin"}},
    "postings": {{"path": "postings.bin"}},
    "terms": {{"path": "terms.dict"}},
    "meta_idx": {{"path": "meta.idx", "schema": "u32 block_id, u32 offset, u32 doc_size"}},
    "meta": {{"path": "meta.blocks", "block_size": {}, "doc_aligned": true}}
  }}
}}"#,
            num_docs, dim, num_docs, dim, num_docs, block_size
        );
        let mut f = File::create(root.join("manifest.json")).unwrap();
        f.write_all(manifest.as_bytes()).unwrap();
    }

    fn write_meta_blocks(root: &Path, block_count: u32, block_size: u32) {
        let mut f = File::create(root.join("meta.blocks")).unwrap();
        f.write_all(&block_count.to_le_bytes()).unwrap();
        let hdr = [0u8; 16];
        for _ in 0..block_count {
            f.write_all(&hdr).unwrap();
        }
        let block = vec![0u8; block_size as usize];
        for _ in 0..block_count {
            f.write_all(&block).unwrap();
        }
    }

    fn write_meta_idx(root: &Path, entries: usize) {
        let mut f = File::create(root.join("meta.idx")).unwrap();
        for _ in 0..entries {
            let entry = MetaIdxEntry { block_id: 0, offset_in_block: 0, doc_size: 0, padding: 0 };
            let bytes: [u8; META_IDX_ENTRY_SIZE] = unsafe { std::mem::transmute(entry) };
            f.write_all(&bytes).unwrap();
        }
    }

    fn touch(root: &Path, name: &str) {
        let _ = File::create(root.join(name)).unwrap();
    }

    #[test]
    fn open_ok_with_valid_headers() {
        let dir = temp_dir("nvs_rust_ok");
        write_manifest(&dir, 3, 4, 256);
        touch(&dir, "vectors.f32");
        touch(&dir, "doclen.u32");
        touch(&dir, "lexicon.bin");
        touch(&dir, "postings.bin");
        touch(&dir, "terms.dict");
        write_meta_idx(&dir, 3);
        write_meta_blocks(&dir, 2, 256);

        let b = Bundle::open(&dir).expect("bundle open");
        assert_eq!(b.meta_block_size, 256);
        assert_eq!(b.meta_block_count, 2);
    }

    #[test]
    fn open_fails_on_meta_idx_count_mismatch() {
        let dir = temp_dir("nvs_rust_bad_idx");
        write_manifest(&dir, 2, 1, 128);
        touch(&dir, "vectors.f32");
        touch(&dir, "doclen.u32");
        touch(&dir, "lexicon.bin");
        touch(&dir, "postings.bin");
        touch(&dir, "terms.dict");
        write_meta_idx(&dir, 1); // should be 2
        write_meta_blocks(&dir, 1, 128);

        let err = Bundle::open(&dir).unwrap_err();
        match err { NvsError::InvalidBundle(_) => {}, _ => panic!("unexpected err") }
    }

    #[test]
    fn open_fails_on_manifest_block_size_mismatch() {
        let dir = temp_dir("nvs_rust_bad_bsz");
        write_manifest(&dir, 1, 1, 128);
        touch(&dir, "vectors.f32");
        touch(&dir, "doclen.u32");
        touch(&dir, "lexicon.bin");
        touch(&dir, "postings.bin");
        touch(&dir, "terms.dict");
        write_meta_idx(&dir, 1);
        // write meta.blocks with derived block size 256 (header: 1 block, then 256 bytes)
        write_meta_blocks(&dir, 1, 256);

        let err = Bundle::open(&dir).unwrap_err();
        match err { NvsError::InvalidBundle(_) => {}, _ => panic!("unexpected err") }
    }
}
