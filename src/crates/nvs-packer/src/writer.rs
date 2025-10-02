use anyhow::Result;
use serde::ser::{SerializeMap, Serializer};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use xxhash_rust::xxh64::xxh64;
use nvs_format::{META_BLOCKS_MAGIC, META_IDX_MAGIC, META_IDX_ENTRY_SIZE};

use crate::loader::Doc;

pub fn write_vectors(docs: &[Doc], dim: usize, out: &Path, dtype: &str) -> Result<()> {
    match dtype {
        "f16" => {
            use half::f16;
            let row_bytes = dim * 2;
            let aligned = ((row_bytes + 63) / 64) * 64;
            let mut data = vec![0u8; docs.len() * aligned];
            for (i, d) in docs.iter().enumerate() {
                anyhow::ensure!(
                    d.embedding.len() == dim,
                    "dimension mismatch for doc {}",
                    d.id
                );
                for j in 0..dim {
                    let off = i * aligned + j * 2;
                    let h = f16::from_f32(d.embedding[j]);
                    data[off..off + 2].copy_from_slice(&h.to_le_bytes());
                }
            }
            let mut f = File::create(out.join("vectors.f16"))?;
            f.write_all(&data)?;
            Ok(())
        }
        _ => {
            let row_bytes = dim * 4;
            let aligned = ((row_bytes + 63) / 64) * 64;
            let mut data = vec![0u8; docs.len() * aligned];
            for (i, d) in docs.iter().enumerate() {
                anyhow::ensure!(
                    d.embedding.len() == dim,
                    "dimension mismatch for doc {}",
                    d.id
                );
                for j in 0..dim {
                    let off = i * aligned + j * 4;
                    data[off..off + 4].copy_from_slice(&d.embedding[j].to_le_bytes());
                }
            }
            let mut f = File::create(out.join("vectors.f32"))?;
            f.write_all(&data)?;
            Ok(())
        }
    }
}

pub fn write_meta_and_index(
    docs: &[Doc],
    block_size: usize,
    out: &Path,
    compress: &str,
    zstd_level: i32,
    include_embeddings: bool,
) -> Result<usize> {
    // Append a single metadata record to the current block buffer and return its size in bytes
    fn append_meta_record(cur: &mut Vec<u8>, d: &Doc, include_embeddings: bool) -> Result<u32> {
        let cur_len0 = cur.len();
        // id
        cur.extend_from_slice(&(d.id.len() as u32).to_le_bytes());
        cur.extend_from_slice(d.id.as_bytes());
        // text
        cur.extend_from_slice(&(d.text.len() as u32).to_le_bytes());
        cur.extend_from_slice(d.text.as_bytes());
        // meta (length-prefixed JSON)
        let len_pos = cur.len();
        cur.extend_from_slice(&0u32.to_le_bytes());
        let meta_start = cur.len();
        if include_embeddings {
            let mut ser = serde_json::Serializer::new(&mut *cur);
            let mut map = ser.serialize_map(None)?;
            if let Some(ref m) = d.meta {
                for (k, v) in m.iter() {
                    map.serialize_entry(k, v)?;
                }
            }
            map.serialize_entry("embedding", &d.embedding)?;
            map.end()?;
        } else if let Some(ref m) = d.meta {
            let mut ser = serde_json::Serializer::new(&mut *cur);
            let mut map = ser.serialize_map(Some(m.len()))?;
            for (k, v) in m.iter() {
                map.serialize_entry(k, v)?;
            }
            map.end()?;
        } else {
            cur.extend_from_slice(b"{}");
        }
        let meta_written = (cur.len() - meta_start) as u32;
        cur[len_pos..len_pos + 4].copy_from_slice(&meta_written.to_le_bytes());
        let rec_size = (cur.len() - cur_len0) as u32;
        Ok(rec_size)
    }

    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut headers: Vec<(u32, u32, u32, u32)> = Vec::new();
    let mut idx: Vec<u8> = Vec::new();
    let mut cur = Vec::<u8>::with_capacity(block_size);
    let mut cur_usize = 0u32;
    let mut cur_docs = 0u32;
    let mut block_id = 0u32;
    for d in docs {
        let mut wrote = false;
        for attempt in 0..2 {
            let rec_offset = cur_usize;
            let cur_len0 = cur.len();
            let rec_size = append_meta_record(&mut cur, d, include_embeddings)?;

            if cur_docs > 0 && (cur_usize as usize + rec_size as usize) > block_size {
                // overflow: rollback and start a new block
                cur.truncate(cur_len0);
                if attempt == 0 {
                    headers.push((block_id, cur_usize, cur_docs, 0));
                    blocks.push(std::mem::take(&mut cur));
                    cur = Vec::with_capacity(block_size);
                    cur_usize = 0;
                    cur_docs = 0;
                    block_id += 1;
                    continue;
                } else {
                    anyhow::bail!("record larger than block size");
                }
            }

            // idx entry (after confirming fit)
            idx.extend_from_slice(&block_id.to_le_bytes());
            idx.extend_from_slice(&rec_offset.to_le_bytes());
            idx.extend_from_slice(&rec_size.to_le_bytes());
            idx.extend_from_slice(&0u32.to_le_bytes());

            cur_usize += rec_size;
            cur_docs += 1;
            wrote = true;
            break;
        }
        if !wrote {
            anyhow::bail!("failed to write record after rollover");
        }
    }
    if cur_docs > 0 {
        headers.push((block_id, cur_usize, cur_docs, 0));
        blocks.push(cur);
    }
    // meta.blocks with optional zstd compression per block (still padded to fixed block_size)
    {
        use rayon::prelude::*;
        let codec_flag = if compress == "zstd" { 1u32 } else { 0u32 };
        // Pre-compress blocks in parallel to maintain performance
        let comp: Vec<(Vec<u8>, u32, u32)> = if codec_flag == 1 {
            blocks
                .par_iter()
                .map(|b| {
                    let decomp_len = b.len() as u32;
                    let compressed =
                        zstd::bulk::compress(b, zstd_level).unwrap_or_else(|_| b.clone());
                    (compressed, decomp_len, 1u32)
                })
                .collect()
        } else {
            blocks
                .iter()
                .map(|b| (b.clone(), b.len() as u32, 0u32))
                .collect()
        };

        let mut f = File::create(out.join("meta.blocks"))?;
        // Magic + version
        f.write_all(META_BLOCKS_MAGIC)?;
        // Block count
        f.write_all(&(comp.len() as u32).to_le_bytes())?;
        // Write headers: (comp_size, decomp_size, doc_count, codec)
        for (i, (bytes, decomp_len, cod)) in comp.iter().enumerate() {
            let comp_size = bytes.len() as u32;
            let dcount = headers.get(i).map(|h| h.2).unwrap_or(0);
            let codec = if *cod == 1 { 1u32 } else { 0u32 };
            // If compressed size overflows block_size, fallback: write uncompressed later and mark codec=0
            let final_comp_size = if comp_size as usize > block_size {
                *decomp_len
            } else {
                comp_size
            };
            let final_codec = if comp_size as usize > block_size {
                0u32
            } else {
                codec
            };
            f.write_all(&final_comp_size.to_le_bytes())?;
            f.write_all(&decomp_len.to_le_bytes())?;
            f.write_all(&dcount.to_le_bytes())?;
            f.write_all(&final_codec.to_le_bytes())?;
        }
        // Write block payloads padded to block_size
        // Reusable padding buffer
        let pad = vec![0u8; block_size];
        for (i, (bytes, _decomp_len, cod)) in comp.into_iter().enumerate() {
            let use_comp = if bytes.len() > block_size {
                false
            } else {
                cod == 1
            };
            if use_comp {
                f.write_all(&bytes)?;
                if bytes.len() < block_size {
                    let need = block_size - bytes.len();
                    f.write_all(&pad[..need])?;
                }
            } else {
                // write original uncompressed block
                let b = &blocks[i];
                f.write_all(b)?;
                if b.len() < block_size {
                    let need = block_size - b.len();
                    f.write_all(&pad[..need])?;
                }
            }
        }
    }
    // meta.idx (magic + buffered entries)
    {
        let f = File::create(out.join("meta.idx"))?;
        let mut bw = BufWriter::new(f);
        bw.write_all(META_IDX_MAGIC)?;
        bw.write_all(&idx)?;
        bw.flush()?;
    }
    Ok(headers.len())
}

pub fn write_manifest(
    out: &Path,
    n: usize,
    dim: usize,
    block_size: usize,
    avgdl: f64,
    model: &str,
    dtype: &str,
    compress: &str,
) -> Result<()> {
    use nvs_core::manifest as m;
    let files = m::ManifestFiles {
        vectors: m::ManifestFilesEntry {
            path: format!("vectors.{}", dtype),
            dtype: Some(dtype.to_string()),
            rows: Some(n as u64),
            cols: Some(dim as u64),
            schema: None,
            row_alignment: Some(64),
        },
        doclen: m::ManifestFilesEntry {
            path: "doclen.u32".into(),
            dtype: Some("u32".into()),
            rows: Some(n as u64),
            cols: None,
            schema: None,
            row_alignment: None,
        },
        lexicon: m::ManifestFilesEntry {
            path: "lexicon.bin".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: None,
            row_alignment: None,
        },
        postings: m::ManifestFilesEntry {
            path: "postings.bin".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: None,
            row_alignment: None,
        },
        terms: m::ManifestFilesEntry {
            path: "terms.dict".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: None,
            row_alignment: None,
        },
        meta_idx: m::ManifestFilesEntry {
            path: "meta.idx".into(),
            dtype: None,
            rows: None,
            cols: None,
            schema: Some("u32 block_id, u32 offset, u32 doc_size, u32 reserved0".into()),
            row_alignment: None,
        },
        meta: m::ManifestFilesMeta {
            path: "meta.blocks".into(),
            block_size: Some(block_size as u32),
            doc_aligned: Some(true),
            compression: if compress == "zstd" {
                Some("zstd".into())
            } else {
                None
            },
        },
    };
    let manifest = m::Manifest {
        format: "nvs.v1".into(),
        endianness: Some("little".into()),
        num_docs: n as u64,
        dim: dim as u64,
        embedding: m::ManifestEmbedding {
            model: model.into(),
            dtype: dtype.into(),
        },
        bm25: m::ManifestBm25 {
            avgdl,
            k1: 1.2,
            b: 0.75,
        },
        files,
    };
    let f = File::create(out.join("manifest.json"))?;
    let mut bw = BufWriter::new(f);
    serde_json::to_writer_pretty(&mut bw, &manifest)?;
    bw.flush()?;
    Ok(())
}

pub fn write_checksums(out: &Path) -> Result<()> {
    let candidates = [
        "manifest.json",
        "vectors.f32",
        "vectors.f16",
        "doclen.u32",
        "lexicon.bin",
        "postings.bin",
        "terms.dict",
        "meta.idx",
        "meta.blocks",
    ];
    let mut s = String::new();
    for name in candidates {
        let path = out.join(name);
        if path.exists() {
            let mut buf = Vec::new();
            File::open(&path)?.read_to_end(&mut buf)?;
            let h = xxh64(&buf, 0);
            s.push_str(&format!("{h:016x}  {name}\n"));
        }
    }
    let mut f = File::create(out.join("checksums.xxhash64"))?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

pub fn write_receipts(out: &Path, receipts: &[(String, usize)]) -> Result<()> {
    let mut f = File::create(out.join("receipts.txt"))?;
    for (name, count) in receipts.iter() {
        writeln!(f, "{}\t{}", name, count)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Doc;
    use std::fs;
    use std::io::Read;

    fn make_tmp(prefix: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("nvs_packer_test_{}_{}", prefix, t));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn write_vectors_f32_and_f16() {
        let dir = make_tmp("writer_vecs");
        let docs = vec![
            Doc {
                id: "a".into(),
                text: "t".into(),
                embedding: vec![1.0, 2.0, 3.0],
                meta: None,
            },
            Doc {
                id: "b".into(),
                text: "t".into(),
                embedding: vec![4.0, 5.0, 6.0],
                meta: None,
            },
        ];
        let dim = 3;
        // f32
        write_vectors(&docs, dim, &dir, "f32").unwrap();
        let row_bytes = dim * 4;
        let stride = ((row_bytes + 63) / 64) * 64;
        let sz = fs::metadata(dir.join("vectors.f32")).unwrap().len() as usize;
        assert_eq!(sz, docs.len() * stride);
        // f16
        write_vectors(&docs, dim, &dir, "f16").unwrap();
        let row_bytes = dim * 2;
        let stride = ((row_bytes + 63) / 64) * 64;
        let sz = fs::metadata(dir.join("vectors.f16")).unwrap().len() as usize;
        assert_eq!(sz, docs.len() * stride);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_meta_manifest_checksums_receipts() {
        let dir = make_tmp("writer_meta");
        let docs = vec![
            Doc {
                id: "d1".into(),
                text: "hello".into(),
                embedding: vec![0.0, 1.0],
                meta: Some(serde_json::Map::new()),
            },
            Doc {
                id: "d2".into(),
                text: "world".into(),
                embedding: vec![1.0, 0.0],
                meta: None,
            },
        ];
        let blocks = write_meta_and_index(&docs, 1024, &dir, "none", 3, true).unwrap();
        assert!(blocks >= 1);
        let idx_path = dir.join("meta.idx");
        let idx_buf = fs::read(&idx_path).unwrap();
        assert!(idx_buf.len() >= 8);
        assert_eq!(&idx_buf[..8], nvs_format::META_IDX_MAGIC);
        let payload = &idx_buf[8..];
        assert_eq!(payload.len(), docs.len() * 16);
        assert!(dir.join("meta.blocks").exists());

        write_manifest(&dir, docs.len(), 2, 1024, 3.0, "model", "f32", "none").unwrap();
        let mut s = String::new();
        File::open(dir.join("manifest.json"))
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["format"], "nvs.v1");
        assert_eq!(v["embedding"]["dtype"], "f32");

        write_checksums(&dir).unwrap();
        assert!(dir.join("checksums.xxhash64").exists());

        write_receipts(&dir, &[("file1".into(), 1), ("file2".into(), 2)]).unwrap();
        let mut s2 = String::new();
        File::open(dir.join("receipts.txt"))
            .unwrap()
            .read_to_string(&mut s2)
            .unwrap();
        assert!(s2.contains("file1\t1"));
        assert!(s2.contains("file2\t2"));
        let _ = fs::remove_dir_all(&dir);
    }
}
