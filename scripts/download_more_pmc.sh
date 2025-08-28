#!/bin/bash

# Download more PMC PDFs from different directories to reach 1-2GB

set -e

SAMPLES_DIR="samples/pdf"
TARGET_SIZE_MB=1500  # 1.5GB target

echo "📥 Downloading more PubMed Central PDFs to reach ~${TARGET_SIZE_MB}MB..."

# Function to get directory size in MB
get_dir_size_mb() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        du -sm "$1" 2>/dev/null | cut -f1
    else
        du -sm "$1" 2>/dev/null | cut -f1
    fi
}

CURRENT_SIZE=$(get_dir_size_mb "${SAMPLES_DIR}")
echo "Current size: ${CURRENT_SIZE}MB"

# Download from multiple subdirectories to get variety
# PMC organizes by first 2 chars of PMC ID
DIRS=(
    "00/01" "00/02" "00/03" "00/04" "00/05"
    "00/0a" "00/0b" "00/0c" "00/0d" "00/0e"
    "01/00" "01/01" "01/02" "01/03" "01/04"
    "02/00" "02/01" "02/02" "02/03" "02/04"
    "03/00" "03/01" "03/02" "03/03" "03/04"
    "04/00" "04/01" "04/02" "04/03" "04/04"
    "05/00" "05/01" "05/02" "05/03" "05/04"
)

for DIR in "${DIRS[@]}"; do
    CURRENT_SIZE=$(get_dir_size_mb "${SAMPLES_DIR}")
    
    if [ "$CURRENT_SIZE" -ge "$TARGET_SIZE_MB" ]; then
        echo "✅ Reached target size (${CURRENT_SIZE}MB >= ${TARGET_SIZE_MB}MB)"
        break
    fi
    
    echo "Downloading from oa_pdf/${DIR}/..."
    
    # Use timeout to limit each directory download
    timeout 60 rsync -av --progress \
        --max-size=50M \
        --include '*/' \
        --include '*.pdf' \
        --exclude '*' \
        --timeout=30 \
        "rsync://ftp.ncbi.nlm.nih.gov/pub/pmc/oa_pdf/${DIR}/" \
        "${SAMPLES_DIR}/" 2>/dev/null || true
    
    NEW_SIZE=$(get_dir_size_mb "${SAMPLES_DIR}")
    echo "Size after ${DIR}: ${NEW_SIZE}MB"
    
    # Stop if we're getting close
    if [ "$NEW_SIZE" -ge "$TARGET_SIZE_MB" ]; then
        break
    fi
done

# Final stats
FINAL_SIZE=$(get_dir_size_mb "${SAMPLES_DIR}")
PDF_COUNT=$(find "${SAMPLES_DIR}" -name "*.pdf" -type f | wc -l | tr -d ' ')

echo ""
echo "✅ Download complete!"
echo "   Total size: ${FINAL_SIZE}MB"
echo "   PDF files: ${PDF_COUNT}"
echo "   Location: ${SAMPLES_DIR}/"

# Show distribution
echo ""
echo "File size distribution:"
find "${SAMPLES_DIR}" -name "*.pdf" -type f -exec du -h {} \; | \
    awk '{print $1}' | sort -h | uniq -c | tail -10