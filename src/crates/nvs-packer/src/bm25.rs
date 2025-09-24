use anyhow::Result;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::loader::Doc;

pub struct Bm25Stats {
    pub tf: std::time::Duration,
    pub local: std::time::Duration,
    pub merge: std::time::Duration,
    pub write: std::time::Duration,
}

pub fn compute_doc_tfs(
    docs: &[Doc],
) -> (
    Vec<FxHashMap<String, u32>>,
    Vec<AtomicUsize>,
    std::time::Duration,
) {
    let doc_lens: Vec<AtomicUsize> = (0..docs.len()).map(|_| AtomicUsize::new(0)).collect();
    let mut doc_tfs: Vec<FxHashMap<String, u32>> =
        (0..docs.len()).map(|_| FxHashMap::default()).collect();
    let t_tf_start = std::time::Instant::now();
    doc_tfs.par_iter_mut().enumerate().for_each(|(i, tfmap)| {
        let d = &docs[i];
        let clean = nvs_core::tokenizer::preprocess_bm25(&d.text);
        let kept = tokenize_bm25_into(&clean, tfmap);
        doc_lens[i].store(kept, Ordering::Relaxed);
    });
    let t_tf = t_tf_start.elapsed();
    (doc_tfs, doc_lens, t_tf)
}

pub fn write_doc_lengths(doc_lens: &[AtomicUsize], out: &Path) -> Result<()> {
    let mut f = std::fs::File::create(out.join("doclen.u32"))?;
    for len in doc_lens {
        let v = len.load(Ordering::Relaxed) as u32;
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

pub fn build_local_maps(
    doc_tfs: &[FxHashMap<String, u32>],
) -> (
    Vec<FxHashMap<String, Vec<(usize, u32)>>>,
    std::time::Duration,
    usize,
) {
    let t_local_start = std::time::Instant::now();
    let n = doc_tfs.len();
    let threads = std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(4);
    let chunks = std::cmp::max(threads, 1);
    let chunk_size = (n + chunks - 1) / chunks;
    let mut local_maps: Vec<FxHashMap<String, Vec<(usize, u32)>>> = Vec::new();
    local_maps.resize_with(chunks, FxHashMap::default);
    local_maps
        .par_iter_mut()
        .enumerate()
        .for_each(|(ci, local)| {
            let start = ci * chunk_size;
            if start >= n {
                return;
            }
            let end = std::cmp::min(n, start + chunk_size);
            for i in start..end {
                for (term, count) in doc_tfs[i].iter() {
                    local.entry(term.clone()).or_default().push((i, *count));
                }
            }
        });
    let t_local = t_local_start.elapsed();
    (local_maps, t_local, threads)
}

pub struct BucketOut {
    pub terms: Vec<String>,
    pub postings: Vec<u8>,
    pub lex: Vec<(u64, u32, u32)>,
}

pub fn bucket_kway_merge(
    local_maps: &[FxHashMap<String, Vec<(usize, u32)>>],
    buckets: usize,
) -> (Vec<BucketOut>, std::time::Duration) {
    let bucket_out: Vec<std::sync::Mutex<Option<BucketOut>>> =
        (0..buckets).map(|_| std::sync::Mutex::new(None)).collect();
    let t_merge_start = std::time::Instant::now();
    (0..buckets).into_par_iter().for_each(|b| {
        let mask = buckets.next_power_of_two() - 1;
        let use_mask = (mask + 1) == buckets;
        let mut uniq: FxHashMap<String, ()> = FxHashMap::default();
        for loc in local_maps {
            for k in loc.keys() {
                let h = fxhash::hash64(k);
                let bi = if use_mask {
                    (h as usize) & mask
                } else {
                    (h as usize) % buckets
                };
                if bi == b {
                    uniq.entry(k.clone()).or_insert(());
                }
            }
        }
        let mut terms_b: Vec<String> = uniq.into_keys().collect();
        terms_b.sort();
        let mut postings_b: Vec<u8> = Vec::new();
        let mut lex_b: Vec<(u64, u32, u32)> = Vec::with_capacity(terms_b.len());
        for term in &terms_b {
            let mut slices: Vec<&[(usize, u32)]> = Vec::new();
            let mut pos: Vec<usize> = Vec::new();
            for loc in local_maps {
                if let Some(vec) = loc.get(term) {
                    slices.push(vec);
                    pos.push(0);
                }
            }
            let mut prev = 0usize;
            let mut len: u32 = 0;
            let start = postings_b.len() as u64;
            loop {
                let mut best = usize::MAX;
                let mut which = usize::MAX;
                for i in 0..slices.len() {
                    if pos[i] < slices[i].len() {
                        let d = slices[i][pos[i]].0;
                        if d < best {
                            best = d;
                            which = i;
                        }
                    }
                }
                if which == usize::MAX {
                    break;
                }
                let (doc, tf) = slices[which][pos[which]];
                pos[which] += 1;
                let delta = (doc - prev) as u32;
                prev = doc;
                postings_b.extend_from_slice(&delta.to_le_bytes());
                postings_b.extend_from_slice(&tf.to_le_bytes());
                len += 1;
            }
            let df = len;
            lex_b.push((start, len, df));
        }
        let mut g = bucket_out[b].lock().unwrap();
        *g = Some(BucketOut {
            terms: terms_b,
            postings: postings_b,
            lex: lex_b,
        });
    });
    let t_merge = t_merge_start.elapsed();
    let mut out = Vec::with_capacity(buckets);
    for b in 0..buckets {
        if let Some(v) = bucket_out[b].lock().unwrap().take() {
            out.push(v);
        } else {
            out.push(BucketOut {
                terms: Vec::new(),
                postings: Vec::new(),
                lex: Vec::new(),
            });
        }
    }
    (out, t_merge)
}

pub fn assemble_and_write(
    buckets_vec: Vec<BucketOut>,
    out: &Path,
) -> Result<(Vec<String>, usize, std::time::Duration, std::time::Duration)> {
    let mut total_terms = 0usize;
    let mut total_post_bytes = 0usize;
    for b in &buckets_vec {
        total_terms += b.terms.len();
        if let Some((off, len, _)) = b.lex.last().copied() {
            total_post_bytes += (off as usize) + (len as usize) * 8;
        }
    }
    let buckets = buckets_vec.len();
    let t_assemble_start = std::time::Instant::now();
    let mut heads = vec![0usize; buckets];
    let mut postings = Vec::<u8>::with_capacity(total_post_bytes);
    let mut lexicon = Vec::<u8>::with_capacity(total_terms * 16);
    let mut terms_writer = std::io::BufWriter::new(std::fs::File::create(out.join("terms.dict"))?);
    let mut global_off: u64 = 0;
    let mut run_bucket: Option<usize> = None;
    let mut run_start = 0usize;
    let mut run_bytes = 0usize;
    let mut run_expected_next_off = 0usize;
    loop {
        let mut best_b = usize::MAX;
        let mut best_term: Option<&str> = None;
        for b in 0..buckets {
            let h = heads[b];
            let outb = &buckets_vec[b];
            if h < outb.terms.len() {
                let t = &outb.terms[h];
                if best_term.map_or(true, |cur| t.as_str() < cur) {
                    best_term = Some(t.as_str());
                    best_b = b;
                }
            }
        }
        if best_b == usize::MAX {
            break;
        }
        let outb = &buckets_vec[best_b];
        let idx = heads[best_b];
        let (off_rel, len, df) = outb.lex[idx];
        let start = off_rel as usize;
        let bytes = (len as usize) * 8;
        let term = outb.terms[idx].as_str();
        let l = term.len() as u32;
        terms_writer.write_all(&l.to_le_bytes())?;
        terms_writer.write_all(term.as_bytes())?;
        lexicon.extend_from_slice(&global_off.to_le_bytes());
        lexicon.extend_from_slice(&len.to_le_bytes());
        lexicon.extend_from_slice(&df.to_le_bytes());
        if run_bucket == Some(best_b) && start == run_expected_next_off {
            run_bytes += bytes;
            run_expected_next_off += bytes;
        } else {
            if let Some(rb) = run_bucket {
                let src = &buckets_vec[rb].postings[run_start..run_start + run_bytes];
                postings.extend_from_slice(src);
            }
            run_bucket = Some(best_b);
            run_start = start;
            run_bytes = bytes;
            run_expected_next_off = start + bytes;
        }
        global_off += bytes as u64;
        heads[best_b] += 1;
    }
    if let Some(rb) = run_bucket {
        let src = &buckets_vec[rb].postings[run_start..run_start + run_bytes];
        postings.extend_from_slice(src);
    }
    terms_writer.flush()?;
    let t_assemble = t_assemble_start.elapsed();
    let t_io_start = std::time::Instant::now();
    {
        let mut pf = std::fs::File::create(out.join("postings.bin"))?;
        pf.write_all(&postings)?;
        let mut lf = std::fs::File::create(out.join("lexicon.bin"))?;
        lf.write_all(&lexicon)?;
    }
    let t_io = t_io_start.elapsed();
    let postings_entries_count: usize = postings.len() / 8;
    let terms: Vec<String> = buckets_vec.into_iter().flat_map(|b| b.terms).collect();
    Ok((terms, postings_entries_count, t_assemble, t_io))
}

pub fn tokenize_bm25_into(text: &str, tf: &mut FxHashMap<String, u32>) -> usize {
    use nvs_core::tokenizer::bm25_normalize_token;
    let mut buf = String::with_capacity(32);
    let mut kept = 0usize;
    let mut flush = |buf: &mut String| {
        if buf.is_empty() {
            return;
        }
        for b in unsafe { buf.as_bytes_mut() } {
            if (b'A'..=b'Z').contains(b) {
                *b = *b + 32;
            }
        }
        if let Some(norm) = bm25_normalize_token(&buf) {
            if !nvs_core::tokenizer::is_stopword(&norm) {
                *tf.entry(norm).or_insert(0) += 1;
                kept += 1;
            }
        }
        buf.clear();
    };
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' | '\t' | '\n' | '\x0C' => {
                flush(&mut buf);
            }
            '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => { /* skip */ }
            '-' => {
                let mut it = chars.clone();
                let mut consumed = 0;
                let mut is_break = false;
                while let Some(nc) = it.next() {
                    if nc == '\n' {
                        is_break = true;
                        consumed += 1;
                        break;
                    } else if nc == '\r' || nc == '\t' || nc == ' ' {
                        consumed += 1;
                        continue;
                    } else {
                        break;
                    }
                }
                if is_break {
                    for _ in 0..consumed {
                        let _ = chars.next();
                    }
                    flush(&mut buf);
                } else {
                    buf.push('-');
                }
            }
            c if c.is_alphanumeric() || c == '_' || c >= '\u{80}' => {
                buf.push(c);
            }
            '\'' | '/' | '&' | '.' => {
                buf.push(ch);
            }
            _ => {
                flush(&mut buf);
            }
        }
    }
    flush(&mut buf);
    kept
}

pub fn write_bm25_and_terms(
    docs: &[Doc],
    out: &Path,
    bm25_buckets: usize,
) -> Result<(f64, Vec<String>, usize, usize, Bm25Stats)> {
    let (doc_tfs, doc_lens, t_tf) = compute_doc_tfs(docs);
    write_doc_lengths(&doc_lens, out)?;
    let total_tokens: usize = doc_lens.iter().map(|x| x.load(Ordering::Relaxed)).sum();
    let (local_maps, t_local, threads) = build_local_maps(&doc_tfs);
    let buckets = if bm25_buckets > 0 {
        bm25_buckets
    } else {
        std::cmp::max(1, std::cmp::min(32, threads * 2))
    };
    let (buckets_vec, t_merge) = bucket_kway_merge(&local_maps, buckets);
    let (terms, postings_entries_count, t_assemble, t_io) = assemble_and_write(buckets_vec, out)?;
    let avgdl = if docs.is_empty() {
        0.0
    } else {
        (total_tokens as f64) / (docs.len() as f64)
    };
    Ok((
        avgdl,
        terms,
        postings_entries_count,
        total_tokens,
        Bm25Stats {
            tf: t_tf,
            local: t_local,
            merge: t_merge,
            write: t_assemble + t_io,
        },
    ))
}

use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
    fn tokenize_and_tf_counts() {
        let mut tf = FxHashMap::default();
        let text = "Hello, world! Hello-world\nFoo's bar/baz & qux.";
        // The public path uses preprocess_bm25, but here we test tokenization directly too
        let kept = tokenize_bm25_into(&nvs_core::tokenizer::preprocess_bm25(text), &mut tf);
        assert!(kept >= 5);
        // check some expected tokens present (lowercased)
        assert!(tf.get("hello").is_some());
        assert!(tf.get("world").is_some());
    }

    #[test]
    fn write_bm25_outputs() {
        let dir = make_tmp("bm25");
        let docs = vec![
            Doc {
                id: "d1".into(),
                text: "alpha beta beta".into(),
                embedding: vec![0.0],
                meta: None,
            },
            Doc {
                id: "d2".into(),
                text: "beta gamma".into(),
                embedding: vec![0.0],
                meta: None,
            },
        ];
        let (avgdl, terms, postings, total_tokens, _stats) =
            write_bm25_and_terms(&docs, &dir, 4).unwrap();
        assert!(avgdl > 0.0);
        assert!(terms.len() > 0);
        assert!(postings > 0);
        assert!(total_tokens >= 3);
        // Files should exist
        assert!(dir.join("doclen.u32").exists());
        assert!(dir.join("postings.bin").exists());
        assert!(dir.join("lexicon.bin").exists());
        assert!(dir.join("terms.dict").exists());
        // doclen size equals number of docs * 4
        let len = fs::metadata(dir.join("doclen.u32")).unwrap().len() as usize;
        assert_eq!(len, docs.len() * 4);
        let _ = fs::remove_dir_all(&dir);
    }
}
