#[test]
fn chunk_pdf_cli_help() {
    let exe = env!("CARGO_BIN_EXE_chunk-pdf-cli");
    let output = std::process::Command::new(exe).arg("--help").output().unwrap();
    assert!(output.status.success());
    let s = String::from_utf8_lossy(&output.stdout);
    assert!(s.contains("chunk-pdf-cli"));
}

