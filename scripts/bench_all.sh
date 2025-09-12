#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."/rust

crates=(
  nvs-core
  nvs-html-core
  nvs-html
  nvs-pdf-core
  nvs-pdf
  nvs-packer
  tokenmonster
)

for c in "${crates[@]}"; do
  echo "==> cargo bench -p $c"
  cargo bench -p "$c" || { echo "bench failed for $c"; exit 1; }
done

