# devcheck

Tiny onboarding tool to print a human summary and a JSON blob with OS/arch/cores/CPU features and a quick SIMD sanity test (scalar vs SIMD sum on a large f32 slice).

Usage
- Human + JSON: `cargo run -p devcheck --release`
- JSON only: `cargo run -p devcheck --release -- --json`
- Adjust length (number of f32 values): `cargo run -p devcheck --release -- --len 8000000`
- Disable SIMD path: `cargo run -p devcheck --release -- --simd off`

Notes
- On x86/x86_64, features are gathered via CPUID (raw-cpuid). SIMD path uses SSE or AVX2 when available.
- On aarch64, features are detected at runtime; SIMD path uses NEON when available.
- Output JSON includes rustc version if available via env `RUSTC_VERSION`.
