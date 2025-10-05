#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
MANIFEST="$ROOT_DIR/src/Cargo.toml"

echo "== devcheck (human) =="
cargo run --manifest-path "$MANIFEST" -p devcheck --release | tee "$ROOT_DIR/devcheck.human.out" || true

echo
echo "== devcheck (json) =="
cargo run --manifest-path "$MANIFEST" -p devcheck --release -- --json --len 8000000 | tee "$ROOT_DIR/devcheck.json.out" || true

echo
echo "== cargo build --workspace =="
cargo build --manifest-path "$MANIFEST" --workspace

echo
echo "== cargo test --workspace =="
cargo test --manifest-path "$MANIFEST" --workspace

echo
echo "All done. Artifacts: devcheck.human.out, devcheck.json.out"

