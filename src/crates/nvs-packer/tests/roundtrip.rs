use std::fs::{self, File};
use std::io::Write;

fn write_docs(path: &std::path::Path) {
    let docs = r#"[
      {"id":"d0","text":"alpha beta","metadata":{"embedding":[1,0,0,0]}},
      {"id":"d1","text":"beta gamma","metadata":{"embedding":[0,1,0,0]}},
      {"id":"d2","text":"gamma alpha","metadata":{"embedding":[0,0,1,0]}}
    ]"#;
    let mut f = File::create(path.join("docs.json")).unwrap();
    f.write_all(docs.as_bytes()).unwrap();
}

#[test]
fn roundtrip_zstd_f32() {
    let tmp = std::env::temp_dir();
    let input = tmp.join(format!("nvs_rt_in_{}", uuid()));
    let output = tmp.join(format!("nvs_rt_out_{}", uuid()));
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();
    write_docs(&input);

    let exe = env!("CARGO_BIN_EXE_nvs-packer");
    let status = std::process::Command::new(exe)
        .arg(&input)
        .arg("--out").arg(&output)
        .arg("--model").arg("test-rt")
        .arg("--compress").arg("zstd")
        .status().unwrap();
    assert!(status.success());

    let b = nvs_core::Bundle::open(&output).expect("open bundle");
    assert_eq!(b.manifest.endianness.as_deref(), Some("little"));
    assert_eq!(b.manifest.files.vectors.row_alignment, Some(64));
    // Check a document can be read and JSON parsed
    let (id, text, meta) = b.get_document_value(0).expect("doc0");
    assert_eq!(id, "d0");
    assert!(text.contains("alpha"));
    assert!(meta.is_object());

    let _ = fs::remove_dir_all(&input);
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn roundtrip_zstd_f16() {
    let tmp = std::env::temp_dir();
    let input = tmp.join(format!("nvs_rt_in_f16_{}", uuid()));
    let output = tmp.join(format!("nvs_rt_out_f16_{}", uuid()));
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();
    write_docs(&input);

    let exe = env!("CARGO_BIN_EXE_nvs-packer");
    let status = std::process::Command::new(exe)
        .arg(&input)
        .arg("--out").arg(&output)
        .arg("--model").arg("test-rt")
        .arg("--compress").arg("zstd")
        .arg("--quantize").arg("f16")
        .status().unwrap();
    assert!(status.success());

    let store = nvs_core::VectorStore::from_bundle(nvs_core::Bundle::open(&output).expect("open"));
    assert_eq!(store.dimensions(), 4);
    // Simple vector query should not panic and return results
    let q = [1f32, 0f32, 0f32, 0f32];
    let res = store.search_vector(&q, 2);
    assert!(!res.is_empty());

    let _ = fs::remove_dir_all(&input);
    let _ = fs::remove_dir_all(&output);
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{}", t)
}

