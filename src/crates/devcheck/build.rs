fn main() {
    if let Ok(ver) = std::process::Command::new("rustc").arg("--version").output() {
        println!(
            "cargo:rustc-env=RUSTC_VERSION={}",
            String::from_utf8_lossy(&ver.stdout).trim()
        );
    }
}

