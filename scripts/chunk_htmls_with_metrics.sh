#!/usr/bin/env bash
set -euo pipefail

HTML_DIR="${1:-samples/html}"
OUT_DIR="${2:-samples/json-html}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Input HTML dir: $HTML_DIR"
echo "Output dir: $OUT_DIR"
mkdir -p "$OUT_DIR"

# Resolve binary location (prefer release, then debug). Allow override via NVS_HTML_BIN
if [[ -n "${NVS_HTML_BIN:-}" ]]; then
  BIN="$NVS_HTML_BIN"
else
  TARGET_DIR="$REPO_ROOT/rust/target"
  if [[ -x "$TARGET_DIR/release/chunk-html-cli" ]]; then
    BIN="$TARGET_DIR/release/chunk-html-cli"
  elif [[ -x "$TARGET_DIR/debug/chunk-html-cli" ]]; then
    BIN="$TARGET_DIR/debug/chunk-html-cli"
  else
    echo "chunk-html-cli not found. Build it first, e.g.:" >&2
    echo "  (cd rust && cargo build -p nvs-html --bin chunk-html-cli)" >&2
    exit 1
  fi
fi

"$BIN" \
  --input "$HTML_DIR" \
  --output "$OUT_DIR" \
  --recursive \
  --max-chunk-size 512 \
  --min-chunk-size 150 \
  --overlap 50

echo "Done. Wrote JSON chunks to: $OUT_DIR"
