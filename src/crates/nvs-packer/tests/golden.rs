use std::fs::{self, File};
use std::io::{Read, Write};

#[test]
fn golden_bundle_fields_no_embeddings_in_meta() {
    // Prepare temp dirs
    let tmp = std::env::temp_dir();
    let input = tmp.join(format!("nvs_packer_golden_in_{}", uuid()));
    let output = tmp.join(format!("nvs_packer_golden_out_{}", uuid()));
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();

    // Write docs.json with extra metadata fields besides embedding
    let docs = r#"[
      {"id":"d0","text":"alpha beta","metadata":{"embedding":[1,0,0,0],"title":"Doc 0","url":"http://example/0"}},
      {"id":"d1","text":"beta gamma","metadata":{"embedding":[0,1,0,0],"title":"Doc 1","url":"http://example/1"}},
      {"id":"d2","text":"gamma alpha","metadata":{"embedding":[0,0,1,0],"title":"Doc 2","url":"http://example/2"}}
    ]"#;
    {
        let mut f = File::create(input.join("docs.json")).unwrap();
        f.write_all(docs.as_bytes()).unwrap();
    }

    // Run CLI (default: no embeddings in meta)
    let exe = env!("CARGO_BIN_EXE_nvs-packer");
    let status = std::process::Command::new(exe)
        .arg(&input)
        .arg("--out")
        .arg(&output)
        .arg("--model")
        .arg("test-golden")
        .status()
        .unwrap();
    assert!(status.success());

    // Manifest exists and deserializes
    let manifest_path = output.join("manifest.json");
    assert!(manifest_path.exists());
    let manifest: nvs_core::manifest::Manifest = {
        let mut s = String::new();
        File::open(&manifest_path)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        serde_json::from_str(&s).expect("parse manifest")
    };
    assert_eq!(manifest.format, "nvs.v1");
    assert_eq!(manifest.num_docs, 3);
    assert_eq!(manifest.dim, 4);
    assert_eq!(manifest.embedding.model, "test-golden");
    assert!(manifest.bm25.avgdl > 0.0);

    // Files exist
    let files = &manifest.files;
    assert!(output.join(&files.vectors.path).exists());
    assert!(output.join(&files.doclen.path).exists());
    assert!(output.join(&files.lexicon.path).exists());
    assert!(output.join(&files.postings.path).exists());
    assert!(output.join(&files.terms.path).exists());
    assert!(output.join(&files.meta_idx.path).exists());
    assert!(output.join(&files.meta.path).exists());

    // Vectors size equals rows * stride
    let vec_path = output.join(&files.vectors.path);
    let vec_md = fs::metadata(&vec_path).unwrap();
    let rows = files.vectors.rows.unwrap_or(0) as usize;
    let cols = files.vectors.cols.unwrap_or(0) as usize;
    let row_bytes = cols
        * if files.vectors.dtype.as_deref() == Some("f16") {
            2
        } else {
            4
        };
    let stride = row_bytes.div_ceil(64) * 64;
    assert_eq!(vec_md.len() as usize, rows * stride);

    // doclen.u32 size equals num_docs * 4
    let dl_md = fs::metadata(output.join(&files.doclen.path)).unwrap();
    assert_eq!(dl_md.len(), manifest.num_docs * 4);

    // meta.idx entries count equals num_docs (skip 8-byte magic)
    let idx_buf = fs::read(output.join(&files.meta_idx.path)).unwrap();
    assert!(idx_buf.len() >= 8);
    assert_eq!(&idx_buf[..8], b"NVSIDX\0\x01");
    let payload = &idx_buf[8..];
    assert_eq!(payload.len() % 16, 0);
    assert_eq!(payload.len() as u64 / 16, manifest.num_docs);

    // Parse meta.blocks header and validate counts and a sample document
    let mbuf = fs::read(output.join(&files.meta.path)).unwrap();
    let mut p = 0usize;
    assert_eq!(&mbuf[p..p + 8], b"NVSMETA\x01");
    p += 8;
    let block_count = u32::from_le_bytes(mbuf[p..p + 4].try_into().unwrap()) as usize;
    p += 4;
    assert!(block_count >= 1);
    let mut headers = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let comp_size = u32::from_le_bytes(mbuf[p..p + 4].try_into().unwrap());
        let decomp_size = u32::from_le_bytes(mbuf[p + 4..p + 8].try_into().unwrap());
        let doc_count = u32::from_le_bytes(mbuf[p + 8..p + 12].try_into().unwrap());
        let codec = u32::from_le_bytes(mbuf[p + 12..p + 16].try_into().unwrap());
        p += 16;
        assert!(doc_count >= 1);
        if codec == 0 {
            assert!(decomp_size as usize <= inferred_block_size(&mbuf, block_count));
        }
        headers.push((
            comp_size as usize,
            decomp_size as usize,
            doc_count as usize,
            codec,
        ));
    }
    let block_size = inferred_block_size(&mbuf, block_count);
    let total_docs: usize = headers.iter().map(|h| h.2).sum();
    assert_eq!(total_docs as u64, manifest.num_docs);

    // Decode first block and verify meta JSON lacks embedding, but keeps other fields
    let (csize, dsize, _dcount, codec) = headers[0];
    let header_bytes = 8 + 4 + block_count * 16;
    let block0 = header_bytes;
    let slice = &mbuf[block0..block0 + block_size];
    let block = if codec == 1 {
        let mut out = vec![0u8; dsize];
        zstd::bulk::decompress_to_buffer(&slice[..csize.min(block_size)], &mut out).unwrap();
        out
    } else {
        slice[..dsize.min(block_size)].to_vec()
    };
    // parse first record
    let mut q = 0usize;
    let id_len = u32::from_le_bytes(block[q..q + 4].try_into().unwrap()) as usize;
    q += 4;
    let _id = String::from_utf8(block[q..q + id_len].to_vec()).unwrap();
    q += id_len;
    let text_len = u32::from_le_bytes(block[q..q + 4].try_into().unwrap()) as usize;
    q += 4;
    let _text = String::from_utf8(block[q..q + text_len].to_vec()).unwrap();
    q += text_len;
    let meta_len = u32::from_le_bytes(block[q..q + 4].try_into().unwrap()) as usize;
    q += 4;
    let meta_json = String::from_utf8(block[q..q + meta_len].to_vec()).unwrap();
    assert!(
        !meta_json.contains("\"embedding\""),
        "embedding should not be in meta by default"
    );
    assert!(meta_json.contains("\"title\""));
    assert!(meta_json.contains("\"url\""));

    // Cleanup
    let _ = fs::remove_dir_all(&input);
    let _ = fs::remove_dir_all(&output);
}

fn inferred_block_size(buf: &[u8], block_count: usize) -> usize {
    let header_size = 8 + 4 + block_count * 16;
    (buf.len() - header_size) / block_count
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}", t)
}
