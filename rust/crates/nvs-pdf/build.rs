use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    // Only run bundling when the pdfium-bundled feature is enabled.
    let bundled = env::var("CARGO_FEATURE_PDFIUM_BUNDLED").is_ok();
    if !bundled { return; }

    println!("cargo:rerun-if-changed=build.rs");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    let (asset, libname): (&str, &str) = match (target_os.as_str(), target_arch.as_str()) {
        ("macos", "aarch64") => ("pdfium-mac-arm64.tgz", "libpdfium.dylib"),
        ("macos", "x86_64") => ("pdfium-mac-x64.tgz", "libpdfium.dylib"),
        ("linux", "x86_64") => ("pdfium-linux-x64.tgz", "libpdfium.so"),
        ("linux", "aarch64") => ("pdfium-linux-arm64.tgz", "libpdfium.so"),
        _ => {
            // Unsupported target for auto-bundle: just exit quietly
            eprintln!("nvs-pdf build.rs: auto-bundle not configured for target {}-{}", target_os, target_arch);
            return;
        }
    };

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let bundle_root = out.join("pdfium_bundle");
    let _ = fs::create_dir_all(&bundle_root);

    let url = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/{}",
        asset
    );
    let tgz = bundle_root.join(asset);

    // Download using curl
    let status = Command::new("curl")
        .args(["-L", "-o", tgz.to_str().unwrap(), &url])
        .status()
        .expect("failed to spawn curl");
    if !status.success() {
        eprintln!("nvs-pdf build.rs: curl failed to fetch {}", url);
        return;
    }

    // Extract using tar
    let status = Command::new("tar")
        .args(["-xzf", tgz.to_str().unwrap(), "-C", bundle_root.to_str().unwrap()])
        .status()
        .expect("failed to spawn tar");
    if !status.success() {
        eprintln!("nvs-pdf build.rs: tar failed to extract {}", tgz.display());
        return;
    }

    // Determine directory containing the library
    // Common layouts: <bundle_root>/lib/libpdfium.* or directly under <bundle_root>
    let lib_in_lib = bundle_root.join("lib").join(libname);
    let lib_direct = bundle_root.join(libname);
    let (lib_dir, lib_path) = if lib_in_lib.exists() {
        (lib_in_lib.parent().unwrap().to_path_buf(), lib_in_lib)
    } else if lib_direct.exists() {
        (bundle_root.clone(), lib_direct)
    } else {
        // Some archives may unpack into a subfolder; try to scan for the file
        let mut found: Option<PathBuf> = None;
        if let Ok(mut entries) = fs::read_dir(&bundle_root) {
            while let Some(Ok(e)) = entries.next() {
                let p = e.path().join("lib").join(libname);
                if p.exists() { found = Some(p); break; }
            }
        }
        if let Some(p) = found { (p.parent().unwrap().to_path_buf(), p) } else { (bundle_root.clone(), bundle_root.join(libname)) }
    };

    // Export env vars for runtime binding
    println!("cargo:rustc-env=PDFIUM_BUNDLE_DIR={}", lib_dir.display());
    println!("cargo:rustc-env=PDFIUM_LIBRARY_PATH={}", lib_path.display());
    println!("cargo:rustc-env=PDFIUM_LIB_DIR={}", lib_dir.display());
}

