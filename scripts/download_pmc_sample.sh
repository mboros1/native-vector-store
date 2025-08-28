#!/bin/bash

# Download a sample of PubMed Central Open Access PDFs
# Target: 1-2GB of PDFs for testing

set -e

SAMPLES_DIR="samples/pdf"
MAX_SIZE_MB=2048  # 2GB limit
CURRENT_SIZE=0

echo "📥 Downloading PubMed Central PDF samples (max ${MAX_SIZE_MB}MB)..."
echo "Target directory: ${SAMPLES_DIR}"

# Create directory if it doesn't exist
mkdir -p "${SAMPLES_DIR}"

# Use rsync with size limit
# The --max-size flag limits individual file sizes
# We'll monitor total size and stop when reaching our limit
echo "Starting rsync download..."

# Create a temp file to track downloads
TEMP_LOG=$(mktemp)

# Function to get directory size in MB
get_dir_size_mb() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        du -sm "$1" 2>/dev/null | cut -f1
    else
        # Linux
        du -sm "$1" 2>/dev/null | cut -f1
    fi
}

# Start rsync with progress tracking
# Download from a specific subdirectory to limit scope
# PMC organizes PDFs by journal/year, so let's grab from recent high-quality journals
rsync -av --progress \
    --max-size=50M \
    --include '*/' \
    --include '*.pdf' \
    --exclude '*' \
    --timeout=30 \
    rsync://ftp.ncbi.nlm.nih.gov/pub/pmc/oa_pdf/00/00/ \
    "${SAMPLES_DIR}/" 2>&1 | tee "$TEMP_LOG" &

RSYNC_PID=$!

# Monitor download size
echo "Monitoring download size..."
while kill -0 $RSYNC_PID 2>/dev/null; do
    CURRENT_SIZE=$(get_dir_size_mb "${SAMPLES_DIR}")
    
    if [ -n "$CURRENT_SIZE" ] && [ "$CURRENT_SIZE" -ge "$MAX_SIZE_MB" ]; then
        echo "✅ Reached size limit (${CURRENT_SIZE}MB >= ${MAX_SIZE_MB}MB)"
        kill $RSYNC_PID 2>/dev/null || true
        break
    fi
    
    echo -ne "\rCurrent size: ${CURRENT_SIZE}MB / ${MAX_SIZE_MB}MB"
    sleep 2
done

wait $RSYNC_PID 2>/dev/null || true

# Final size check
FINAL_SIZE=$(get_dir_size_mb "${SAMPLES_DIR}")
PDF_COUNT=$(find "${SAMPLES_DIR}" -name "*.pdf" -type f | wc -l | tr -d ' ')

echo ""
echo "✅ Download complete!"
echo "   Total size: ${FINAL_SIZE}MB"
echo "   PDF files: ${PDF_COUNT}"
echo "   Location: ${SAMPLES_DIR}/"

# Clean up
rm -f "$TEMP_LOG"

# List some sample files
echo ""
echo "Sample files downloaded:"
find "${SAMPLES_DIR}" -name "*.pdf" -type f | head -10

echo ""
echo "Next steps:"
echo "1. Check ~/git/fast-pdf-parser/ for the PDF parser tool"
echo "2. Run: scripts/process_pdfs.sh to convert PDFs to JSON chunks"