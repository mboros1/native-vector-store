use std::fs;

#[test]
fn chunk_html_cli_processes_file() {
    let exe = env!("CARGO_BIN_EXE_chunk-html-cli");
    let dir = tempfile::tempdir().unwrap();
    let html_path = dir.path().join("sample.html");
    let out_path = dir.path().join("out.json");
    fs::write(&html_path, "<html><body><h1>T</h1><p>Hello</p></body></html>").unwrap();
    let status = std::process::Command::new(exe)
        .args(["-i", html_path.to_str().unwrap(), "-o", out_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());
    let data = fs::read_to_string(&out_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&data).unwrap();
    assert!(v.is_array());
}

#[test]
fn chunk_html_cli_help() {
    let exe = env!("CARGO_BIN_EXE_chunk-html-cli");
    let output = std::process::Command::new(exe).arg("--help").output().unwrap();
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("chunk-html-cli"));
}

