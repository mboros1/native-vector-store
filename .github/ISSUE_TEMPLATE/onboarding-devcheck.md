---
name: Onboarding – Env & Hardware Check
about: Run devcheck and verify build/tests
title: "Onboarding: env & hardware check"
labels: ["automation"]
assignees: []
---

Welcome! Please run the dev checker and then build/tests. This helps us catch Rosetta/WSL/VM perf quirks and toolchain mismatches early.

Step 1 — devcheck (human output)

```bash
cargo run -p devcheck --release
```

Optional JSON (paste after the human section)

```bash
cargo run -p devcheck --release -- --json --len 8000000
```

Step 2 — build & tests (workspace)

```bash
cargo build --workspace
cargo test --workspace
```

Checklist

- [ ] OS/Arch printed (e.g., linux/x86_64 or macos/aarch64)
- [ ] Features include expected SIMD (AVX2 on x86_64, NEON on ARM)
- [ ] SIMD sum test shows speedup ≥ 1.5× (if not, tell us — might be Rosetta/VM/WSL)
- [ ] `cargo build --workspace` succeeded
- [ ] `cargo test --workspace` succeeded

Paste outputs here

```
<paste devcheck human output>
```

Optional JSON

```
<paste devcheck JSON output>
```

Notes (optional)

- Anything unusual about your setup (VM, WSL, Rosetta, etc.)
- Output from `rustc --version` if JSON shows unknown
