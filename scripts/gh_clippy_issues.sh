#!/usr/bin/env bash
set -euo pipefail

# Create GitHub issues for crates with clippy findings based on src/clippy.log
# Dry-run by default: prints the issues it would create.
# Pass --create to actually create issues via gh.

LABEL="automation"
DRY_RUN=1
if [ "${1:-}" = "--create" ]; then
  DRY_RUN=0
  shift || true
fi

if [ ! -f src/clippy.log ]; then
  echo "src/clippy.log not found. Run: (cd src && cargo clippy --workspace --all-targets --all-features -q 2>clippy.log)" >&2
  exit 1
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "[dry-run] Would ensure label '$LABEL' exists"
else
  if ! command -v gh >/dev/null 2>&1; then
    echo "gh CLI not found. Install https://cli.github.com/ and authenticate (gh auth login)." >&2
    exit 1
  fi
  echo "Ensuring label '$LABEL' exists..."
  gh label create "$LABEL" --color BFD4F2 --description "Created via automation" 2>/dev/null || true
fi

echo "Parsing clippy.log..."
FILES=$(awk '/^[[:space:]]*-->/ { split($2, a, ":"); print a[1] }' src/clippy.log | sort -u | grep -v '^/Users/')
if [ -z "$FILES" ]; then
  echo "[dry-run] No clippy findings parsed from src/clippy.log (files list empty)." >&2
  exit 0
fi

# Build a tab-separated list of (file, message with location) from clippy.log
TMP=$(mktemp)
awk '
  BEGIN { msg="" }
  /^(warning|error): / { msg=$0; next }
  /^[[:space:]]*-->/ {
    loc=$2; file=loc; sub(/:[0-9]+:[0-9]+$/, "", file);
    if (msg != "") {
      print file "\t" msg " @ " loc;
      msg="";
    }
  }
' src/clippy.log > "$TMP"

# Group findings by crate (path prefix crates/<crate>/...)
TMPDIR=$(mktemp -d)
for f in $FILES; do
  crate="workspace"
  case "$f" in
    crates/*)
      rest=${f#crates/}
      crate=${rest%%/*}
      ;;
  esac
  grep -F "${f}	" "$TMP" >> "$TMPDIR/${crate}.txt" || true
done

COUNT_CRATES=$(ls "$TMPDIR"/*.txt 2>/dev/null | wc -l | tr -d ' ' || echo 0)
echo "Found $COUNT_CRATES crate(s) with clippy findings."

shopt -s nullglob
for p in "$TMPDIR"/*.txt; do
  [ -f "$p" ] || continue
  crate=$(basename "$p" .txt)
  COUNT=$(wc -l < "$p" | tr -d ' ')
  TITLE="Clippy findings for crate ${crate} (${COUNT})"
  LIST=$(awk -F"\t" '{ print "- " $1 ": " $2 }' "$p" | sed -n '1,20p')
  MORE=$(( COUNT - 20 ))
  if [ $MORE -gt 0 ]; then
    LIST=$(printf "%s\n\n(+%d more)" "$LIST" "$MORE")
  fi
  BODY=$(cat << EOF
Automated report: Clippy found ~${COUNT} finding(s) in crate: ${crate}.

Reproduce locally:

    cargo clippy --all-targets --all-features -p ${crate}

Note: clippy often provides inline suggestions (help:) for quick fixes.

Findings:
${LIST}

Label: ${LABEL}
EOF
)
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "\n[dry-run] Would create issue: $TITLE"
    echo "-----"
    echo "$BODY"
    echo "-----"
  else
    echo "Creating issue: $TITLE"
    gh issue create -t "$TITLE" -b "$BODY" -l "$LABEL" || {
      echo "Failed to create issue for crate $crate; continuing..." >&2
    }
  fi
done
shopt -u nullglob

rm -f "$TMP"
echo "Done."
