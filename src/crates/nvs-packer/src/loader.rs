use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use xxhash_rust::xxh64::xxh64;

#[derive(Deserialize)]
pub struct InputDocRaw {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub metadata: Option<JsonValue>,
}

#[derive(Clone)]
pub struct Doc {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub meta: Option<JsonMap<String, JsonValue>>,
}

pub fn extract_embedding_and_meta(
    meta: JsonValue,
) -> Option<(Vec<f32>, Option<JsonMap<String, JsonValue>>)> {
    // metadata may be object with embedding field; remove it and return remaining
    if let Some(mut m) = meta.as_object().cloned() {
        if let Some(emb) = m.remove("embedding") {
            if let Some(arr) = emb.as_array() {
                let mut v = Vec::with_capacity(arr.len());
                for x in arr {
                    if let Some(f) = x.as_f64() {
                        v.push(f as f32);
                    } else if let Some(i) = x.as_i64() {
                        v.push(i as f32);
                    } else {
                        return None;
                    }
                }
                return Some((v, Some(m)));
            }
        }
    }
    None
}

pub fn build_doc_from_raw(r: InputDocRaw, idx_salt: Option<usize>) -> Option<Doc> {
    let text = r.text.or(r.content).unwrap_or_default();
    let mv = r.metadata?;
    let (embedding, meta_other) = extract_embedding_and_meta(mv)?;
    if embedding.is_empty() {
        return None;
    }
    let id = r.id.unwrap_or_else(|| {
        let mut h = xxh64(text.as_bytes(), 0);
        if let Some(s) = idx_salt {
            h ^= s as u64;
        }
        format!("doc-{h:016x}")
    });
    Some(Doc {
        id,
        text,
        embedding,
        meta: meta_other,
    })
}

pub fn read_docs(input_dir: &Path) -> Result<(Vec<Doc>, Vec<(String, usize)>)> {
    use walkdir::WalkDir;
    let mut docs = Vec::new();
    let mut receipts: Vec<(String, usize)> = Vec::new();
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(indicatif::ProgressStyle::with_template(
        "{spinner:.green} {msg}",
    )?);
    pb.set_message("Scanning JSON files...");
    let mut _total = 0usize;
    let mut skipped = 0usize;
    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file()
            && entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
        {
            let path = entry.path();
            pb.set_message(format!("Reading {}", path.display()));
            let mut s = String::new();
            File::open(path)
                .with_context(|| format!("open {}", path.display()))?
                .read_to_string(&mut s)?;
            if s.trim_start().starts_with('[') {
                let before = docs.len();
                let arr: Vec<serde_json::Value> = serde_json::from_str(&s)
                    .with_context(|| format!("parse array in {}", path.display()))?;
                for (i, v) in arr.into_iter().enumerate() {
                    _total += 1;
                    match serde_json::from_value::<InputDocRaw>(v) {
                        Ok(r) => {
                            if let Some(doc) = build_doc_from_raw(r, Some(i)) {
                                docs.push(doc);
                            } else {
                                skipped += 1;
                                eprintln!(
                                    "{} skipping invalid/empty doc ({}:#{})",
                                    console::style("! ").yellow(),
                                    path.display(),
                                    i
                                );
                            }
                        }
                        Err(e) => {
                            skipped += 1;
                            eprintln!(
                                "{} skipping invalid doc ({}:#{}) — {}",
                                console::style("! ").yellow(),
                                path.display(),
                                i,
                                e
                            );
                        }
                    }
                }
                let produced = docs.len() - before;
                receipts.push((path.display().to_string(), produced));
            } else {
                _total += 1;
                match serde_json::from_str::<InputDocRaw>(&s) {
                    Ok(r) => {
                        let fname = path.display().to_string();
                        if let Some(doc) = build_doc_from_raw(r, None) {
                            docs.push(doc);
                            receipts.push((fname, 1));
                        } else {
                            skipped += 1;
                            eprintln!(
                                "{} skipping doc without embedding/metadata ({})",
                                console::style("! ").yellow(),
                                path.display()
                            );
                            receipts.push((fname, 0));
                        }
                    }
                    Err(e) => {
                        skipped += 1;
                        eprintln!(
                            "{} skipping invalid doc ({}) — {}",
                            console::style("! ").yellow(),
                            path.display(),
                            e
                        );
                        receipts.push((path.display().to_string(), 0));
                    }
                }
            }
        }
    }
    pb.finish_with_message(format!("Loaded {} docs (skipped {})", docs.len(), skipped));
    receipts.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((docs, receipts))
}

// Fast adaptive loader (moved largely as-is for behavior parity)
pub fn read_docs_fast(
    input_dir: &Path,
    mmap_threshold: usize,
) -> Result<(Vec<Doc>, Vec<(String, usize)>)> {
    use crossbeam_channel as chan;
    use memmap2::Mmap;
    use serde::de::{self, Deserializer as _, SeqAccess, Visitor};
    use std::thread;
    use walkdir::WalkDir;

    #[derive(Debug)]
    enum Buf {
        Mmap(Mmap),
        Vec(Vec<u8>),
    }
    impl Buf {
        fn as_slice(&self) -> &[u8] {
            match self {
                Buf::Mmap(m) => &m,
                Buf::Vec(v) => v,
            }
        }
    }
    #[derive(Debug)]
    struct Job {
        _path: PathBuf,
        buf: Buf,
    }

    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(indicatif::ProgressStyle::with_template(
        "{spinner:.green} {msg}",
    )?);
    pb.set_message("Scanning JSON files (fast)...");

    let (tx, rx) = chan::bounded::<Job>(64);
    {
        let tx = tx.clone();
        let input_dir = input_dir.to_path_buf();
        thread::spawn(move || {
            for entry in WalkDir::new(&input_dir).into_iter().filter_map(|e| e.ok()) {
                if !(entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .map(|e| e == "json")
                        .unwrap_or(false))
                {
                    continue;
                }
                let path = entry.path().to_path_buf();
                let md = match std::fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let job = if md.len() as usize <= mmap_threshold {
                    match File::open(&path).and_then(|f| {
                        unsafe { Mmap::map(&f) }
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                    }) {
                        Ok(m) => Job {
                            _path: path,
                            buf: Buf::Mmap(m),
                        },
                        Err(_) => match std::fs::read(&path) {
                            Ok(v) => Job {
                                _path: path,
                                buf: Buf::Vec(v),
                            },
                            Err(_) => continue,
                        },
                    }
                } else {
                    match std::fs::read(&path) {
                        Ok(v) => Job {
                            _path: path,
                            buf: Buf::Vec(v),
                        },
                        Err(_) => continue,
                    }
                };
                if tx.send(job).is_err() {
                    break;
                }
            }
        });
    }

    let nthreads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let mut handles = Vec::new();
    for _ in 0..nthreads {
        let rx = rx.clone();
        handles.push(thread::spawn(
            move || -> (Vec<Doc>, usize, Vec<(String, usize)>) {
                let mut out: Vec<Doc> = Vec::with_capacity(1024);
                let mut skipped: usize = 0;
                let mut receipts: Vec<(String, usize)> = Vec::with_capacity(128);

                while let Ok(job) = rx.recv() {
                    let file_name = job._path.display().to_string();
                    let bytes = job.buf.as_slice();
                    let mut de = serde_json::Deserializer::from_slice(bytes);
                    struct StreamVisitor<'a> {
                        out: &'a mut Vec<Doc>,
                        skipped: &'a mut usize,
                    }
                    impl<'de, 'a> Visitor<'de> for StreamVisitor<'a> {
                        type Value = ();
                        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                            write!(f, "array or object of docs")
                        }
                        fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
                        where
                            A: SeqAccess<'de>,
                        {
                            while let Some(raw) = seq.next_element::<InputDocRaw>()? {
                                if let Some((embedding, meta_other)) =
                                    raw.metadata.and_then(extract_embedding_and_meta)
                                {
                                    let text = raw.text.or(raw.content).unwrap_or_default();
                                    if !embedding.is_empty() {
                                        let id = raw.id.unwrap_or_else(|| {
                                            let h = xxh64(text.as_bytes(), 0);
                                            format!("doc-{h:016x}")
                                        });
                                        self.out.push(Doc {
                                            id,
                                            text,
                                            embedding,
                                            meta: meta_other,
                                        });
                                    } else {
                                        *self.skipped += 1;
                                    }
                                } else {
                                    *self.skipped += 1;
                                }
                            }
                            Ok(())
                        }
                        fn visit_map<M>(self, mut map: M) -> Result<(), M::Error>
                        where
                            M: de::MapAccess<'de>,
                        {
                            let mut id: Option<String> = None;
                            let mut text: Option<String> = None;
                            let mut content: Option<String> = None;
                            let mut metadata: Option<serde_json::Value> = None;
                            while let Some(k) = map.next_key::<String>()? {
                                match k.as_str() {
                                    "id" => {
                                        id = map.next_value()?;
                                    }
                                    "text" => {
                                        text = map.next_value()?;
                                    }
                                    "content" => {
                                        content = map.next_value()?;
                                    }
                                    "metadata" => {
                                        metadata = map.next_value()?;
                                    }
                                    _ => {
                                        let _: serde_json::Value = map.next_value()?;
                                    }
                                }
                            }
                            if let Some((embedding, meta_other)) =
                                metadata.and_then(extract_embedding_and_meta)
                            {
                                let the_text = text.or(content).unwrap_or_default();
                                if !embedding.is_empty() {
                                    let the_id = id.unwrap_or_else(|| {
                                        let h = xxh64(the_text.as_bytes(), 0);
                                        format!("doc-{h:016x}")
                                    });
                                    self.out.push(Doc {
                                        id: the_id,
                                        text: the_text,
                                        embedding,
                                        meta: meta_other,
                                    });
                                } else {
                                    *self.skipped += 1;
                                }
                            } else {
                                *self.skipped += 1;
                            }
                            Ok(())
                        }
                    }

                    let res = de.deserialize_any(StreamVisitor {
                        out: &mut out,
                        skipped: &mut skipped,
                    });
                    if res.is_err() {
                        // fallback: try single object
                        match serde_json::from_slice::<InputDocRaw>(bytes) {
                            Ok(r) => {
                                if let Some(doc) = build_doc_from_raw(r, None) {
                                    out.push(doc);
                                    receipts.push((file_name, 1));
                                } else {
                                    receipts.push((file_name, 0));
                                }
                            }
                            Err(_) => {
                                receipts.push((file_name, 0));
                            }
                        }
                    } else {
                        let produced = out.len();
                        receipts.push((file_name, produced));
                    }
                }
                (out, skipped, receipts)
            },
        ));
    }

    drop(tx);
    let mut docs = Vec::new();
    let mut skipped_total = 0usize;
    let mut receipts_all: Vec<(String, usize)> = Vec::new();
    for h in handles {
        let (mut d, s, mut r) = h.join().unwrap();
        docs.append(&mut d);
        skipped_total += s;
        receipts_all.append(&mut r);
    }
    pb.finish_with_message(format!(
        "Loaded {} docs (skipped {})",
        docs.len(),
        skipped_total
    ));
    receipts_all.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((docs, receipts_all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn make_tmp(prefix: &str) -> PathBuf {
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
    fn extract_embedding_and_meta_ok() {
        let meta = serde_json::json!({
            "embedding": [1.0, 2.5, 3.0],
            "author": "alice",
            "tags": ["x"]
        });
        let (emb, rest) = extract_embedding_and_meta(meta).expect("should parse");
        assert_eq!(emb, vec![1.0, 2.5, 3.0]);
        let rest = rest.unwrap();
        assert_eq!(rest.get("author").unwrap(), &serde_json::json!("alice"));
        assert!(rest.get("embedding").is_none());
    }

    #[test]
    fn build_doc_from_raw_id_and_meta() {
        let raw = InputDocRaw {
            id: None,
            text: Some("Hello world".into()),
            content: None,
            metadata: Some(serde_json::json!({ "embedding": [0.1, 0.2] , "x": 1 })),
        };
        let doc = build_doc_from_raw(raw, None).expect("doc");
        // id is doc-<xxh64(text)>
        let h = xxh64("Hello world".as_bytes(), 0);
        assert_eq!(doc.id, format!("doc-{h:016x}"));
        assert_eq!(doc.embedding, vec![0.1, 0.2]);
        assert_eq!(doc.meta.unwrap().get("x").unwrap(), &serde_json::json!(1));
    }

    #[test]
    fn read_docs_single_and_array() {
        let dir = make_tmp("loader");
        // single object
        let mut f1 = File::create(dir.join("one.json")).unwrap();
        write!(
            f1,
            "{}",
            serde_json::json!({
                "text": "A", "metadata": {"embedding": [1,2,3], "lang": "en"}
            })
        )
        .unwrap();

        // array with one valid and one invalid
        let mut f2 = File::create(dir.join("arr.json")).unwrap();
        write!(
            f2,
            "{}",
            serde_json::json!([
                {"text": "B", "metadata": {"embedding": [0.0] }},
                {"text": "C", "metadata": {"no_embedding": true }}
            ])
        )
        .unwrap();

        let (docs, receipts) = read_docs(&dir).expect("read");
        // Should load 2 docs (A, B)
        assert_eq!(docs.len(), 2);
        // Receipts contain entries for both files
        let names: Vec<_> = receipts.iter().map(|(n, _)| n.clone()).collect();
        assert!(names.iter().any(|n| n.ends_with("one.json")));
        assert!(names.iter().any(|n| n.ends_with("arr.json")));

        // cleanup
        let _ = fs::remove_dir_all(&dir);
    }
}
