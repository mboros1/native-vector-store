use std::fs::{self, File};
use std::io::Write;

#[test]
fn pack_then_open_bundle() {
    // Prepare temp dirs
    let tmp = std::env::temp_dir();
    let input = tmp.join(format!("nvs_packer_in_{}", uuid()));
    let output = tmp.join(format!("nvs_packer_out_{}", uuid()));
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();

    // Write docs.json
    let docs = r#"[
      {"id":"d0","text":"apple banana","metadata":{"embedding":[1,0,0,0]}},
      {"id":"d1","text":"banana cherry","metadata":{"embedding":[1,0,0,0]}},
      {"id":"d2","text":"cherry apple","metadata":{"embedding":[1,0,0,0]}}
    ]"#;
    {
        let mut f = File::create(input.join("docs.json")).unwrap();
        f.write_all(docs.as_bytes()).unwrap();
    }

    // Run CLI
    let exe = env!("CARGO_BIN_EXE_nvs-packer");
    let status = std::process::Command::new(exe)
        .arg(&input)
        .arg("--out").arg(&output)
        .arg("--model").arg("test")
        .status().unwrap();
    assert!(status.success());

    // Open with nvs-core reader
    let store = nvs_core::VectorStore::from_bundle(nvs_core::Bundle::open(&output).expect("open bundle"));
    assert_eq!(store.size(), 3);
    assert_eq!(store.dimensions(), 4);
    // Read a document
    let d0 = store.get_document(0).expect("doc0");
    assert_eq!(d0.0, "d0");
    assert!(d0.1.contains("apple banana"));
    assert!(d0.2.contains("\"embedding\""));

    // Basic vector search
    let q = [1f32,0f32,0f32,0f32];
    let res = store.search_vector(&q, 2);
    assert!(!res.is_empty());

    // BM25 search
    let bm = store.search_bm25("apple", 2);
    assert!(!bm.is_empty());

    // Cleanup
    let _ = fs::remove_dir_all(&input);
    let _ = fs::remove_dir_all(&output);
}

#[test]
fn pack_then_open_bundle_f16() {
    // Prepare temp dirs
    let tmp = std::env::temp_dir();
    let input = tmp.join(format!("nvs_packer_in_f16_{}", uuid()));
    let output = tmp.join(format!("nvs_packer_out_f16_{}", uuid()));
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();

    // Write docs.json
    let docs = r#"[
      {"id":"d0","text":"alpha beta","metadata":{"embedding":[1,0,0,0]}},
      {"id":"d1","text":"beta gamma","metadata":{"embedding":[0,1,0,0]}},
      {"id":"d2","text":"gamma alpha","metadata":{"embedding":[0,0,1,0]}}
    ]"#;
    {
        let mut f = File::create(input.join("docs.json")).unwrap();
        f.write_all(docs.as_bytes()).unwrap();
    }

    // Run CLI with f16 quantization
    let exe = env!("CARGO_BIN_EXE_nvs-packer");
    let status = std::process::Command::new(exe)
        .arg(&input)
        .arg("--out").arg(&output)
        .arg("--model").arg("test")
        .arg("--quantize").arg("f16")
        .status().unwrap();
    assert!(status.success());

    // Open with nvs-core reader
    let store = nvs_core::VectorStore::from_bundle(nvs_core::Bundle::open(&output).expect("open bundle"));
    assert_eq!(store.size(), 3);
    assert_eq!(store.dimensions(), 4);
    // Basic vector search (query matches first vector best)
    let q = [1f32,0f32,0f32,0f32];
    let res = store.search_vector(&q, 3);
    assert!(!res.is_empty());
    assert_eq!(res[0].0, 0);

    // Cleanup
    let _ = fs::remove_dir_all(&input);
    let _ = fs::remove_dir_all(&output);
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{}", t)
}
