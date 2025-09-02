use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::errors::*;
use crate::manifest::Manifest;
use std::collections::HashMap;
use memmap2::Mmap;

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
    // Vectors
    vectors: Mmap,
    // Metadata
    meta_blocks: Mmap,
    meta_idx: Vec<MetaIdxEntry>,
    // BM25 (internal)
    doclen: Vec<u32>,
    terms: HashMap<String, usize>,
    lexicon: Vec<LexiconEntry>,
    postings: Vec<u8>,
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

        // Validate meta.idx count == num_docs and load entries
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
        let mut meta_idx_entries = Vec::with_capacity(count);
        {
            let mut f = File::open(&meta_idx_path)?;
            let mut buf = Vec::with_capacity(sz);
            f.read_to_end(&mut buf)?;
            let mut i = 0usize;
            while i + 16 <= buf.len() {
                let block_id = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()); i+=4;
                let offset_in_block = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()); i+=4;
                let doc_size = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()); i+=4;
                let padding = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()); i+=4;
                meta_idx_entries.push(MetaIdxEntry{ block_id, offset_in_block, doc_size, padding });
            }
        }

        // Validate meta.blocks header and derive block_size
        let meta_blocks_path = root.join(&manifest.files.meta.path);
        let meta_blocks_file = File::open(&meta_blocks_path)?;
        let mut f = meta_blocks_file.try_clone()?;
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
        let meta_blocks = unsafe { Mmap::map(&meta_blocks_file)? };

        // Map vectors
        let vectors_path = root.join(&manifest.files.vectors.path);
        // Only f32 supported for now
        if manifest.embedding.dtype.to_lowercase() != "f32" {
            return Err(NvsError::InvalidBundle("only f32 vectors supported"));
        }
        let vec_file = File::open(&vectors_path)?;
        let vectors = unsafe { Mmap::map(&vec_file)? };
        let row_bytes = (manifest.dim as usize) * 4;
        let aligned_row_bytes = ((row_bytes + 63) / 64) * 64;
        let expected = (manifest.num_docs as usize) * aligned_row_bytes;
        if vectors.len() != expected {
            return Err(NvsError::InvalidBundle("vectors size mismatch"));
        }

        // Load BM25-related files
        let doclen_path = root.join(&manifest.files.doclen.path);
        let mut doclen = Vec::<u32>::new();
        {
            let mut f = File::open(&doclen_path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            if buf.len() % 4 != 0 { return Err(NvsError::InvalidBundle("doclen size not multiple of 4")); }
            let n = buf.len() / 4;
            doclen.resize(n, 0);
            for i in 0..n {
                let b = [buf[4*i], buf[4*i+1], buf[4*i+2], buf[4*i+3]];
                doclen[i] = u32::from_le_bytes(b);
            }
            if n as u64 != manifest.num_docs { return Err(NvsError::InvalidBundle("doclen rows mismatch")); }
        }
        // terms.dict
        let terms_path = root.join(&manifest.files.terms.path);
        let terms = load_terms(&terms_path)?;
        // lexicon.bin
        let lexicon_path = root.join(&manifest.files.lexicon.path);
        let lexicon = load_lexicon(&lexicon_path)?;
        // postings.bin
        let postings_path = root.join(&manifest.files.postings.path);
        let postings = {
            let mut f = File::open(&postings_path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            buf
        };

        Ok(Self {
            root,
            manifest,
            meta_block_size: derived_block,
            meta_block_count: block_count,
            vectors,
            meta_blocks,
            meta_idx: meta_idx_entries,
            doclen,
            terms,
            lexicon,
            postings,
        })
    }

    pub fn get_document(&self, doc_id: u32) -> Option<(String, String, String)> {
        let idx = *self.meta_idx.get(doc_id as usize)?;
        let header_size = 4usize + (self.meta_block_count as usize) * 16usize;
        let block_size = self.meta_block_size as usize;
        let base = &self.meta_blocks;
        let blocks_start = header_size;
        let block0 = blocks_start;
        let block_begin = block0 + (idx.block_id as usize) * block_size;
        // Bounds checks
        if (idx.offset_in_block as usize) > block_size { return None; }
        if (idx.offset_in_block as usize) + (idx.doc_size as usize) > block_size { return None; }
        let mut p = block_begin + idx.offset_in_block as usize;
        let end = block_begin + block_size;
        if p + 4 > end { return None; }
        let id_len = u32::from_le_bytes(base[p..p+4].try_into().ok()?) as usize; p += 4;
        if p + id_len > end { return None; }
        let id = String::from_utf8(base[p..p+id_len].to_vec()).ok()?; p += id_len;
        if p + 4 > end { return None; }
        let text_len = u32::from_le_bytes(base[p..p+4].try_into().ok()?) as usize; p += 4;
        if p + text_len > end { return None; }
        let text = String::from_utf8(base[p..p+text_len].to_vec()).ok()?; p += text_len;
        if p + 4 > end { return None; }
        let meta_len = u32::from_le_bytes(base[p..p+4].try_into().ok()?) as usize; p += 4;
        if p + meta_len > end { return None; }
        let meta = String::from_utf8(base[p..p+meta_len].to_vec()).ok()?;
        Some((id, text, meta))
    }

    pub fn search_hybrid(&self, query_vec: &[f32], query_text: &str, k: usize, mut vector_weight: f32) -> Vec<(u32, f32)> {
        if k == 0 { return Vec::new(); }
        if query_vec.len() != self.manifest.dim as usize { return Vec::new(); }
        if vector_weight.is_nan() { vector_weight = 0.5; }
        if vector_weight < 0.0 { vector_weight = 0.0; }
        if vector_weight > 1.0 { vector_weight = 1.0; }

        let kk = std::cmp::min(k * 2, self.manifest.num_docs as usize);
        let vres = self.search_vector(query_vec, kk);
        let bres = self.search_bm25(query_text, kk);

        let mut combined: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
        // RRF with constant 60 like C++
        let c = 60.0f32;
        for (i, (doc, _s)) in vres.iter().enumerate() {
            let rrf = 1.0f32 / (c + (i as f32) + 1.0);
            *combined.entry(*doc).or_insert(0.0) += vector_weight * rrf;
        }
        let one_minus = 1.0f32 - vector_weight;
        for (i, (doc, _s)) in bres.iter().enumerate() {
            let rrf = 1.0f32 / (c + (i as f32) + 1.0);
            *combined.entry(*doc).or_insert(0.0) += one_minus * rrf;
        }

        let mut items: Vec<(u32, f32)> = combined.into_iter().collect();
        items.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0)));
        if items.len() > k { items.truncate(k); }
        items
    }

    #[inline]
    fn row_stride_f32(&self) -> usize {
        let row_bytes = (self.manifest.dim as usize) * 4;
        let aligned_row_bytes = ((row_bytes + 63) / 64) * 64;
        aligned_row_bytes / 4
    }

    pub fn search_vector(&self, query: &[f32], k: usize) -> Vec<(u32, f32)> {
        if k == 0 { return Vec::new(); }
        if query.len() != self.manifest.dim as usize { return Vec::new(); }
        // Cast mapped bytes into f32 slice
        let store_f32: &[f32] = bytemuck::cast_slice(&self.vectors);
        let mut res = crate::search::search_parallel(
            query,
            self.manifest.num_docs as usize,
            self.manifest.dim as usize,
            self.row_stride_f32(),
            store_f32,
            k,
        );
        // Stable tie-breaking by doc id ascending
        res.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0)));
        res
    }

    // Public API: accept a raw query string; tokenize and dispatch
    pub fn search_bm25(&self, query: &str, k: usize) -> Vec<(u32, f32)> {
        let tok = crate::tokenizer::SimpleTokenizer::new();
        let terms = tok.split(query);
        let view: Vec<&str> = terms.iter().map(|s| s.as_str()).collect();
        self.search_bm25_terms(&view, k)
    }

    // Internal: accept pre-tokenized terms
    fn search_bm25_terms(&self, query_terms: &[&str], k: usize) -> Vec<(u32, f32)> {
        let n = self.manifest.num_docs as usize;
        if n == 0 || k == 0 || query_terms.is_empty() { return Vec::new(); }
        let avgdl = self.manifest.bm25.avgdl as f32;
        let k1 = self.manifest.bm25.k1 as f32;
        let b = self.manifest.bm25.b as f32;

        let mut acc: HashMap<u32, f32> = HashMap::with_capacity(1024);
        for &qt in query_terms {
            if let Some(&tid) = self.terms.get(qt) {
                if tid >= self.lexicon.len() { continue; }
                let lex = &self.lexicon[tid];
                let idf = ((self.manifest.num_docs as f32 - lex.df as f32 + 0.5) / (lex.df as f32 + 0.5)).ln();
                let mut prev = 0u32;
                let mut off = lex.offset as usize;
                for _ in 0..lex.length {
                    if off + 8 > self.postings.len() { break; }
                    let delta = u32::from_le_bytes(self.postings[off..off+4].try_into().unwrap());
                    let tf = u32::from_le_bytes(self.postings[off+4..off+8].try_into().unwrap());
                    off += 8;
                    let doc = prev.wrapping_add(delta);
                    prev = doc;
                    let dl = self.doclen.get(doc as usize).copied().unwrap_or(0) as f32;
                    let tfc = (tf as f32 * (k1 + 1.0)) / (tf as f32 + k1 * (1.0 - b + b * dl / avgdl));
                    *acc.entry(doc).or_insert(0.0) += idf * tfc;
                }
            }
        }
        // Top-k extraction with a min-heap
        use ordered_float::OrderedFloat;
        type HeapItem = std::cmp::Reverse<(OrderedFloat<f32>, u32)>;
        let mut heap: std::collections::BinaryHeap<HeapItem> = std::collections::BinaryHeap::new();
        for (&doc, &score) in acc.iter() {
            let item = std::cmp::Reverse((OrderedFloat(score), doc));
            if heap.len() < k { heap.push(item); }
            else if let Some(mut top) = heap.peek_mut() { if item.0 .0 > top.0 .0 { *top = item; } }
        }
        let mut v: Vec<(OrderedFloat<f32>,u32)> = heap.into_sorted_vec().into_iter().map(|r| r.0).collect();
        v.reverse();
        v.into_iter().map(|(s,d)|(d,s.0)).collect()
    }
}

#[derive(Debug, Clone, Copy)]
struct LexiconEntry { offset: u64, length: u32, df: u32 }

fn load_lexicon(path: &Path) -> Result<Vec<LexiconEntry>> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new(); f.read_to_end(&mut buf)?;
    if buf.len() % 16 != 0 { return Err(NvsError::InvalidBundle("lexicon size not multiple of 16")); }
    let mut v = Vec::with_capacity(buf.len()/16);
    let mut i=0usize;
    while i+16 <= buf.len() {
        let off = u64::from_le_bytes(buf[i..i+8].try_into().unwrap()); i+=8;
        let length = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()); i+=4;
        let df = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()); i+=4;
        v.push(LexiconEntry{offset: off, length, df});
    }
    Ok(v)
}

fn load_terms(path: &Path) -> Result<HashMap<String, usize>> {
    let mut f = File::open(path)?; let mut buf = Vec::new(); f.read_to_end(&mut buf)?;
    let mut m = HashMap::new(); let mut i=0usize; let mut id=0usize;
    while i + 4 <= buf.len() {
        let len = u32::from_le_bytes(buf[i..i+4].try_into().unwrap()) as usize; i+=4;
        if i + len > buf.len() { break; }
        let s = String::from_utf8_lossy(&buf[i..i+len]).to_string(); i+=len;
        m.insert(s, id); id+=1;
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use crate::tokenizer::SimpleTokenizer;

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
        // write vectors file with correct padded size (zeros)
        {
            let row_bytes = (4usize) * 4;
            let aligned_row_bytes = ((row_bytes + 63) / 64) * 64;
            let data = vec![0u8; (3usize) * aligned_row_bytes];
            let mut f = File::create(dir.join("vectors.f32")).unwrap();
            f.write_all(&data).unwrap();
        }
        // write doclen for 3 docs
        {
            let mut f = File::create(dir.join("doclen.u32")).unwrap();
            for v in [0u32,0u32,0u32] { f.write_all(&v.to_le_bytes()).unwrap(); }
        }
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
        // vectors file with correct padded size (zeros)
        {
            let row_bytes = (1usize) * 4;
            let aligned_row_bytes = ((row_bytes + 63) / 64) * 64;
            let data = vec![0u8; (1usize) * aligned_row_bytes];
            let mut f = File::create(dir.join("vectors.f32")).unwrap();
            f.write_all(&data).unwrap();
        }
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

    #[test]
    fn bm25_small_corpus_ordering() {
        let dir = temp_dir("nvs_rust_bm25");
        // 3 docs, dim 1, block_size 128
        write_manifest(&dir, 3, 1, 128);
        // vectors file with correct padded size (zeros)
        {
            let dim = 1usize; let num_docs = 3usize;
            let row_bytes = dim * 4; let aligned_row_bytes = ((row_bytes + 63) / 64) * 64;
            let data = vec![0u8; num_docs * aligned_row_bytes];
            let mut f = File::create(dir.join("vectors.f32")).unwrap(); f.write_all(&data).unwrap();
        }
        // doclen: token counts per doc
        {
            let mut f = File::create(dir.join("doclen.u32")).unwrap();
            // len(a)=3, len(b)=1, len(c)=3
            for v in [3u32,1u32,3u32] { f.write_all(&v.to_le_bytes()).unwrap(); }
        }
        // terms: apple, banana, cherry
        {
            let mut f = File::create(dir.join("terms.dict")).unwrap();
            for s in ["apple","banana","cherry"] {
                let len = s.len() as u32; f.write_all(&len.to_le_bytes()).unwrap(); f.write_all(s.as_bytes()).unwrap();
            }
        }
        // postings: each entry [delta,u32][tf,u32]
        // apple in doc0(tf=3) and doc2(tf=1)
        // banana in doc1(tf=3)
        // cherry in doc2(tf=2)
        let mut postings = Vec::<u8>::new();
        let mut lex = Vec::<u8>::new();
        let mut offset: u64 = 0;
        let add_entry = |delta: u32, tf: u32, buf: &mut Vec<u8>| { buf.extend_from_slice(&delta.to_le_bytes()); buf.extend_from_slice(&tf.to_le_bytes()); };
        // apple: 2 entries
        add_entry(0, 3, &mut postings); // doc0
        add_entry(2, 1, &mut postings); // doc2 (prev=0 -> +2)
        lex.extend_from_slice(&offset.to_le_bytes()); lex.extend_from_slice(&(2u32).to_le_bytes()); lex.extend_from_slice(&(2u32).to_le_bytes());
        offset += 2*8;
        // banana: 1 entry (doc1)
        add_entry(1, 3, &mut postings);
        lex.extend_from_slice(&offset.to_le_bytes()); lex.extend_from_slice(&(1u32).to_le_bytes()); lex.extend_from_slice(&(1u32).to_le_bytes());
        offset += 1*8;
        // cherry: 1 entry (doc2)
        add_entry(1, 2, &mut postings); // from prev doc1 -> doc2 delta=1
        lex.extend_from_slice(&offset.to_le_bytes()); lex.extend_from_slice(&(1u32).to_le_bytes()); lex.extend_from_slice(&(1u32).to_le_bytes());

        {
            let mut f = File::create(dir.join("postings.bin")).unwrap(); f.write_all(&postings).unwrap();
            let mut lf = File::create(dir.join("lexicon.bin")).unwrap(); lf.write_all(&lex).unwrap();
        }
        // minimal meta files
        write_meta_idx(&dir, 3);
        write_meta_blocks(&dir, 1, 128);

        let b = Bundle::open(&dir).unwrap();
        // Query apple should bring doc0 before doc2
        let res = b.search_bm25("apple", 3);
        assert!(!res.is_empty());
        assert_eq!(res[0].0, 0);
        // Multi-term apple+banana likely keeps doc1 and doc0 in top 2
        let res2 = b.search_bm25("apple banana", 3);
        assert!(res2.iter().any(|&(id,_)| id==0));
        assert!(res2.iter().any(|&(id,_)| id==1));
    }

    #[test]
    fn vector_search_small() {
        use std::io::Write;
        let dir = temp_dir("nvs_rust_vec");
        let num_docs = 4u64; let dim = 4u64; let block = 128u32;
        write_manifest(&dir, num_docs, dim, block);
        // Write vectors: identity rows padded to 64B
        {
            let row_bytes = (dim as usize) * 4;
            let aligned_row_bytes = ((row_bytes + 63) / 64) * 64;
            let mut data = vec![0u8; (num_docs as usize) * aligned_row_bytes];
            for i in 0..(num_docs as usize) {
                for j in 0..(dim as usize) {
                    let v = if i == j { 1f32 } else { 0f32 };
                    let off = i*aligned_row_bytes + j*4;
                    data[off..off+4].copy_from_slice(&v.to_le_bytes());
                }
            }
            let mut f = File::create(dir.join("vectors.f32")).unwrap();
            f.write_all(&data).unwrap();
        }
        // minimal bm25 files
        {
            let mut f = File::create(dir.join("doclen.u32")).unwrap(); for _ in 0..num_docs { f.write_all(&0u32.to_le_bytes()).unwrap(); }
        }
        File::create(dir.join("lexicon.bin")).unwrap();
        File::create(dir.join("postings.bin")).unwrap();
        File::create(dir.join("terms.dict")).unwrap();
        write_meta_idx(&dir, num_docs as usize);
        write_meta_blocks(&dir, 1, 128);

        let b = Bundle::open(&dir).unwrap();
        let q = [1f32, 0f32, 0f32, 0f32];
        let res = b.search_vector(&q, 3);
        assert!(!res.is_empty());
        // Top-1 should be doc 0
        assert_eq!(res[0].0, 0);
        // Scores should be non-increasing
        for i in 1..res.len() { assert!(res[i-1].1 >= res[i].1); }
        // Determinism
        let res2 = b.search_vector(&q, 3);
        assert_eq!(res, res2);
    }

    #[test]
    fn get_document_basic() {
        use std::io::Write;
        let dir = temp_dir("nvs_rust_getdoc");
        write_manifest(&dir, 2, 1, 128);
        // vectors
        {
            let row_bytes = 4usize; let aligned = ((row_bytes + 63) / 64) * 64; let data = vec![0u8; 2*aligned];
            let mut f = File::create(dir.join("vectors.f32")).unwrap(); f.write_all(&data).unwrap();
        }
        // doclen
        { let mut f = File::create(dir.join("doclen.u32")).unwrap(); for _ in 0..2 { f.write_all(&0u32.to_le_bytes()).unwrap(); } }
        // empty bm25 index files
        File::create(dir.join("lexicon.bin")).unwrap();
        File::create(dir.join("postings.bin")).unwrap();
        File::create(dir.join("terms.dict")).unwrap();
        // meta.blocks with 1 block and 2 docs
        let (id0, text0, meta0) = ("a", "text a", "{\"k\":1}");
        let (id1, text1, meta1) = ("b", "text b", "{\"k\":2}");
        let rec_size = |id:&str, tx:&str, mj:&str| 4 + id.len() + 4 + tx.len() + 4 + mj.len();
        let s0 = rec_size(id0, text0, meta0);
        let s1 = rec_size(id1, text1, meta1);
        let mut mb = Vec::<u8>::new();
        // block_count = 1
        mb.extend_from_slice(&1u32.to_le_bytes());
        // header for block 0: [id, usize, dcount, pad]
        mb.extend_from_slice(&0u32.to_le_bytes());
        mb.extend_from_slice(&(s0 as u32 + s1 as u32).to_le_bytes());
        mb.extend_from_slice(&2u32.to_le_bytes());
        mb.extend_from_slice(&0u32.to_le_bytes());
        // block data
        let write_rec = |id:&str, tx:&str, mj:&str, buf:&mut Vec<u8>| {
            buf.extend_from_slice(&(id.len() as u32).to_le_bytes()); buf.extend_from_slice(id.as_bytes());
            buf.extend_from_slice(&(tx.len() as u32).to_le_bytes()); buf.extend_from_slice(tx.as_bytes());
            buf.extend_from_slice(&(mj.len() as u32).to_le_bytes()); buf.extend_from_slice(mj.as_bytes());
        };
        write_rec(id0, text0, meta0, &mut mb);
        write_rec(id1, text1, meta1, &mut mb);
        // pad to block_size 128
        let block_size = 128usize;
        let _header_size = 4 + 1*16;
        let data_len = s0 + s1;
        let pad_len = block_size - data_len;
        mb.extend(std::iter::repeat(0u8).take(pad_len));
        // write file
        let mut fmb = File::create(dir.join("meta.blocks")).unwrap(); fmb.write_all(&mb).unwrap();
        // meta.idx entries
        {
            let mut idx = Vec::<u8>::new();
            idx.extend_from_slice(&0u32.to_le_bytes()); idx.extend_from_slice(&0u32.to_le_bytes()); idx.extend_from_slice(&(s0 as u32).to_le_bytes()); idx.extend_from_slice(&0u32.to_le_bytes());
            idx.extend_from_slice(&0u32.to_le_bytes()); idx.extend_from_slice(&(s0 as u32).to_le_bytes()); idx.extend_from_slice(&(s1 as u32).to_le_bytes()); idx.extend_from_slice(&0u32.to_le_bytes());
            let mut fi = File::create(dir.join("meta.idx")).unwrap(); fi.write_all(&idx).unwrap();
        }

        let b = Bundle::open(&dir).unwrap();
        let d0 = b.get_document(0).unwrap();
        assert_eq!(d0.0, "a"); assert!(d0.1.contains("text a")); assert!(d0.2.contains("\"k\":1"));
        let d1 = b.get_document(1).unwrap();
        assert_eq!(d1.0, "b"); assert!(d1.1.contains("text b")); assert!(d1.2.contains("\"k\":2"));
    }

    #[test]
    fn hybrid_extremes_vector_and_bm25() {
        use std::io::Write;
        // Build a bundle where BM25 has signal and vectors are zero; weight 0.0 follows BM25
        let dir = temp_dir("nvs_rust_hybrid_bm25");
        write_manifest(&dir, 3, 1, 128);
        // vectors: zeros
        {
            let row_bytes = 4usize; let aligned = ((row_bytes + 63) / 64) * 64; let data = vec![0u8; 3*aligned];
            let mut f = File::create(dir.join("vectors.f32")).unwrap(); f.write_all(&data).unwrap();
        }
        // doclen
        { let mut f = File::create(dir.join("doclen.u32")).unwrap(); for _ in 0..3 { f.write_all(&1u32.to_le_bytes()).unwrap(); } }
        // terms: one term 'apple'
        {
            let mut f = File::create(dir.join("terms.dict")).unwrap(); let s = "apple"; f.write_all(&(s.len() as u32).to_le_bytes()).unwrap(); f.write_all(s.as_bytes()).unwrap();
        }
        // postings: apple in doc1 only
        {
            let mut lf = File::create(dir.join("lexicon.bin")).unwrap();
            let mut pf = File::create(dir.join("postings.bin")).unwrap();
            // offset=0, length=1, df=1
            lf.write_all(&0u64.to_le_bytes()).unwrap(); lf.write_all(&1u32.to_le_bytes()).unwrap(); lf.write_all(&1u32.to_le_bytes()).unwrap();
            // posting [delta=1, tf=1]
            pf.write_all(&1u32.to_le_bytes()).unwrap(); pf.write_all(&1u32.to_le_bytes()).unwrap();
        }
        write_meta_idx(&dir, 3); write_meta_blocks(&dir, 1, 128);
        let b = Bundle::open(&dir).unwrap();
        let v = [1f32];
        let hv = b.search_hybrid(&v, "apple", 2, 0.0);
        assert_eq!(hv[0].0, 1, "bm25 extreme should rank doc1 first");

        // Build a bundle where BM25 is empty and vectors are identity; weight 1.0 follows vectors
        let dir2 = temp_dir("nvs_rust_hybrid_vec");
        write_manifest(&dir2, 3, 3, 128);
        {
            let dim=3usize; let n=3usize; let row_bytes = dim*4; let aligned=((row_bytes+63)/64)*64; let mut data=vec![0u8; n*aligned];
            for i in 0..n { for j in 0..dim { let v = if i==j {1f32} else {0f32}; let off=i*aligned + j*4; data[off..off+4].copy_from_slice(&v.to_le_bytes()); } }
            let mut f = File::create(dir2.join("vectors.f32")).unwrap(); f.write_all(&data).unwrap();
        }
        { let mut f = File::create(dir2.join("doclen.u32")).unwrap(); for _ in 0..3 { f.write_all(&0u32.to_le_bytes()).unwrap(); } }
        File::create(dir2.join("lexicon.bin")).unwrap(); File::create(dir2.join("postings.bin")).unwrap(); File::create(dir2.join("terms.dict")).unwrap();
        write_meta_idx(&dir2, 3); write_meta_blocks(&dir2, 1, 128);
        let b2 = Bundle::open(&dir2).unwrap();
        let q = [1f32,0f32,0f32];
        let hv2 = b2.search_hybrid(&q, "unused", 2, 1.0);
        assert_eq!(hv2[0].0, 0, "vector extreme should rank doc0 first");
    }

    // --- E2E-like pack-then-open helpers and tests ---
    #[derive(Clone)]
    struct TDoc { id: String, text: String, embedding: Vec<f32> }

    fn pack_bundle(dir: &Path, docs: &[TDoc], dim: usize, block_size: usize) {
        // Vectors (64B aligned rows)
        {
            let row_bytes = dim * 4; let aligned=((row_bytes+63)/64)*64; let mut data=vec![0u8; docs.len()*aligned];
            for (i, d) in docs.iter().enumerate() {
                assert_eq!(d.embedding.len(), dim);
                for j in 0..dim { let off=i*aligned + j*4; data[off..off+4].copy_from_slice(&d.embedding[j].to_le_bytes()); }
            }
            let mut f = File::create(dir.join("vectors.f32")).unwrap(); f.write_all(&data).unwrap();
        }
        // Tokenize and collect BM25 stats
        let tok = SimpleTokenizer::new();
        let mut doc_tokens: Vec<Vec<String>> = Vec::with_capacity(docs.len());
        let mut df_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut postings_map: std::collections::HashMap<String, Vec<(usize, u32)>> = std::collections::HashMap::new();
        for (i, d) in docs.iter().enumerate() {
            let tokens = tok.split(&d.text);
            let mut tf: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
            for t in &tokens { *tf.entry(t.as_str()).or_insert(0) += 1; }
            for (term, &count) in tf.iter() {
                postings_map.entry(term.to_string()).or_default().push((i, count));
            }
            for term in tf.keys() { *df_map.entry((*term).to_string()).or_insert(0) += 1; }
            doc_tokens.push(tokens);
        }
        // doclen
        {
            let mut f = File::create(dir.join("doclen.u32")).unwrap();
            for tokens in &doc_tokens { let len = tokens.len() as u32; f.write_all(&len.to_le_bytes()).unwrap(); }
        }
        // Terms sorted for consistent IDs
        let mut terms: Vec<String> = postings_map.keys().cloned().collect(); terms.sort();
        {
            let mut f = File::create(dir.join("terms.dict")).unwrap();
            for t in &terms { let len=t.len() as u32; f.write_all(&len.to_le_bytes()).unwrap(); f.write_all(t.as_bytes()).unwrap(); }
        }
        // Build postings.bin and lexicon.bin
        {
            let mut postings = Vec::<u8>::new(); let mut lexicon = Vec::<u8>::new(); let mut offset: u64 = 0;
            for t in &terms {
                let mut list = postings_map.get(t).cloned().unwrap_or_default();
                list.sort_by_key(|&(doc, _)| doc);
                let mut prev = 0usize; let mut length = 0u32;
                for (doc, tf) in list.into_iter() {
                    let delta = (doc - prev) as u32; prev = doc; length += 1;
                    postings.extend_from_slice(&delta.to_le_bytes()); postings.extend_from_slice(&tf.to_le_bytes());
                }
                let df = *df_map.get(t).unwrap_or(&0) as u32;
                lexicon.extend_from_slice(&offset.to_le_bytes()); lexicon.extend_from_slice(&length.to_le_bytes()); lexicon.extend_from_slice(&df.to_le_bytes());
                offset += (length as u64) * 8;
            }
            let mut pf = File::create(dir.join("postings.bin")).unwrap(); pf.write_all(&postings).unwrap();
            let mut lf = File::create(dir.join("lexicon.bin")).unwrap(); lf.write_all(&lexicon).unwrap();
        }
        // Build meta.blocks and meta.idx
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
        // meta.blocks: write header and padded blocks
        {
            let mut f = File::create(dir.join("meta.blocks")).unwrap();
            f.write_all(&(headers.len() as u32).to_le_bytes()).unwrap();
            for (id, usizeb, dcount, pad) in &headers { f.write_all(&id.to_le_bytes()).unwrap(); f.write_all(&usizeb.to_le_bytes()).unwrap(); f.write_all(&dcount.to_le_bytes()).unwrap(); f.write_all(&pad.to_le_bytes()).unwrap(); }
            for b in &blocks { f.write_all(&b).unwrap(); if b.len()<block_size { f.write_all(&vec![0u8; block_size - b.len()]).unwrap(); } }
        }
        // meta.idx
        { let mut f = File::create(dir.join("meta.idx")).unwrap(); f.write_all(&idx).unwrap(); }
        // manifest
        {
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
                docs.len(), dim, docs.len(), dim, docs.len(), block_size
            );
            let mut f = File::create(dir.join("manifest.json")).unwrap(); f.write_all(manifest.as_bytes()).unwrap();
        }
        // checksums.xxhash64
        {
            use xxhash_rust::xxh64::xxh64;
            let files = [
                "manifest.json","vectors.f32","doclen.u32","lexicon.bin","postings.bin","terms.dict","meta.idx","meta.blocks"
            ];
            let mut out = String::new();
            for name in files { let path = dir.join(name); let mut buf=Vec::new(); File::open(&path).unwrap().read_to_end(&mut buf).unwrap(); let h = xxh64(&buf, 0); out.push_str(&format!("{h:016x}  {name}\n")); }
            let mut f = File::create(dir.join("checksums.xxhash64")).unwrap(); f.write_all(out.as_bytes()).unwrap();
        }
    }

    #[test]
    fn e2e_pack_then_open_single_block() {
        let dir_in = temp_dir("nvs_rust_e2e_in_single");
        let docs = vec![
            TDoc{ id: "doc0".into(), text: "doc text number 0".into(), embedding: vec![1.0,0.0,0.0,0.0] },
            TDoc{ id: "doc1".into(), text: "doc text number 1".into(), embedding: vec![1.0,0.0,0.0,0.0] },
            TDoc{ id: "doc2".into(), text: "doc text number 2".into(), embedding: vec![1.0,0.0,0.0,0.0] },
        ];
        pack_bundle(&dir_in, &docs, 4, 131072);
        let b = Bundle::open(&dir_in).unwrap();
        assert_eq!(b.manifest.num_docs, 3);
        assert_eq!(b.manifest.dim, 4);
        let d0 = b.get_document(0).unwrap(); assert_eq!(d0.0, "doc0"); assert!(d0.1.contains("doc text number 0")); assert!(d0.2.contains("\"embedding\""));
        let d2 = b.get_document(2).unwrap(); assert_eq!(d2.0, "doc2"); assert!(d2.1.contains("doc text number 2"));
        let q = [1f32,0f32,0f32,0f32]; let res = b.search_vector(&q, 2); assert!(!res.is_empty());
    }

    #[test]
    fn e2e_pack_then_open_multiple_blocks() {
        let dir_in = temp_dir("nvs_rust_e2e_in_multi");
        let mut docs = Vec::new(); for i in 0..10 { docs.push(TDoc{ id: format!("m{i}"), text: format!("m text number {i}"), embedding: vec![1.0,0.0,0.0,0.0] }); }
        pack_bundle(&dir_in, &docs, 4, 256);
        let b = Bundle::open(&dir_in).unwrap(); assert_eq!(b.manifest.num_docs, 10);
        let d0 = b.get_document(0).unwrap(); assert_eq!(d0.0, "m0"); let d9 = b.get_document(9).unwrap(); assert_eq!(d9.0, "m9");
        for i in 0..10 { let d = b.get_document(i).unwrap(); assert_eq!(d.0, format!("m{i}")); }
    }

    #[test]
    fn e2e_block_headers_and_checksums() {
        let dir_in = temp_dir("nvs_rust_e2e_hdr");
        let mut docs = Vec::new(); for i in 0..10 { docs.push(TDoc{ id: format!("h{i}"), text: format!("h text {i}"), embedding: vec![1.0,0.0,0.0,0.0] }); }
        pack_bundle(&dir_in, &docs, 4, 256);
        // Parse meta.blocks
        {
            let mut f = File::open(dir_in.join("meta.blocks")).unwrap();
            let mut buf = Vec::new(); f.read_to_end(&mut buf).unwrap(); let mut p=0usize;
            let block_count = u32::from_le_bytes(buf[p..p+4].try_into().unwrap()) as usize; p+=4;
            let mut hdrs = Vec::new();
            for _ in 0..block_count { let id=u32::from_le_bytes(buf[p..p+4].try_into().unwrap()); let usizeb=u32::from_le_bytes(buf[p+4..p+8].try_into().unwrap()); let dcount=u32::from_le_bytes(buf[p+8..p+12].try_into().unwrap()); let pad=u32::from_le_bytes(buf[p+12..p+16].try_into().unwrap()); p+=16; hdrs.push((id,usizeb,dcount,pad)); }
            let total_size = buf.len();
            let header_size = 4 + block_count*16; let block_size = (total_size - header_size)/block_count; assert!(block_size>0);
            let mut total_docs=0usize;
            for i in 0..block_count { let (id, usizeb, dcount, _)=hdrs[i]; let start = header_size + i*block_size; let mut consumed=0usize; let mut pos=start;
                while consumed < usizeb as usize { let idl=u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap()) as usize; pos+=4; consumed+=4; pos+=idl; consumed+=idl; let tl=u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap()) as usize; pos+=4; consumed+=4; pos+=tl; consumed+=tl; let ml=u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap()) as usize; pos+=4; consumed+=4; pos+=ml; consumed+=ml; total_docs+=1; }
                assert_eq!(consumed, usizeb as usize); assert_eq!(total_docs as u32, hdrs.iter().map(|h| h.2).take(i+1).sum::<u32>());
            }
            assert_eq!(total_docs, 10);
        }
        // Checksums format sanity
        {
            let mut s = String::new(); File::open(dir_in.join("checksums.xxhash64")).unwrap().read_to_string(&mut s).unwrap();
            let mut seen=0; for line in s.lines() { if line.is_empty() { continue; } let mut parts = line.split("  "); let hex = parts.next().unwrap(); let fname = parts.next().unwrap_or(""); assert_eq!(hex.len(), 16); assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && c.is_lowercase() || c.is_ascii_digit())); assert!(Path::new(&dir_in).join(fname).exists()); seen+=1; }
            assert!(seen>=5);
        }
    }
}
