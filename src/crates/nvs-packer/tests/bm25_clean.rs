use std::fs::{self, File};
use std::io::{Read, Write};

fn parse_terms_dict(path: &std::path::Path) -> Vec<String> {
    let mut buf = Vec::new();
    File::open(path).unwrap().read_to_end(&mut buf).unwrap();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= buf.len() {
        let len = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) as usize;
        i += 4;
        if i + len > buf.len() {
            break;
        }
        let s = String::from_utf8_lossy(&buf[i..i + len]).to_string();
        out.push(s);
        i += len;
    }
    out
}

#[test]
fn bm25_tokenizer_cleans_artifacts() {
    // Prepare temp dirs
    let tmp = std::env::temp_dir();
    let input = tmp.join(format!("nvs_bm25_in_{}", uuid()));
    let output = tmp.join(format!("nvs_bm25_out_{}", uuid()));
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();

    // Write docs.json with artifacts
    // JSON-escaped text (\n and \u000C as control)
    let text_json = "High-\\nquality &&Chibnall manufacturer’s 0.01 ---ABC utm_campaign\\u000C";
    let docs = format!(
        "[{{\"id\":\"d0\",\"text\":\"{}\",\"metadata\":{{\"embedding\":[1,0,0,0]}}}}]",
        text_json
    );
    {
        let mut f = File::create(input.join("docs.json")).unwrap();
        f.write_all(docs.as_bytes()).unwrap();
    }

    // Run CLI
    let exe = env!("CARGO_BIN_EXE_nvs-packer");
    let status = std::process::Command::new(exe)
        .arg(&input)
        .arg("--out")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    // Inspect terms.dict
    let terms = parse_terms_dict(&output.join("terms.dict"));
    let has = |w: &str| terms.iter().any(|t| t == w);
    let not = |w: &str| !has(w);
    assert!(has("high"));
    assert!(has("quality"));
    assert!(has("chibnall"));
    assert!(has("manufacturer")); // possessive stripped
    assert!(not("High-"));
    assert!(not("0.01"));
    assert!(not("utm_campaign"));
    assert!(not("---ABC"));

    let _ = fs::remove_dir_all(&input);
    let _ = fs::remove_dir_all(&output);
}

fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}", t)
}
