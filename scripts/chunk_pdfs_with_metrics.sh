#!/bin/bash

# Process PDFs to JSON chunks with comprehensive metrics
# Uses the fast-pdf-parser chunk-pdf-cli tool

set -e

# Configuration
PDF_DIR="${1:-samples/pdf}"
OUTPUT_DIR="${2:-samples/json}"
CHUNK_CLI="${3:-bin/chunk-pdf-cli}"
METRICS_FILE="samples/processing_metrics.json"
LOG_FILE="samples/processing.log"
FAILED_LIST="samples/failed_pdfs.txt"

# Parameters for chunking
MAX_CHUNK_SIZE=1000  # tokens
MIN_CHUNK_SIZE=100   # tokens
OVERLAP=50           # tokens
MAX_WORKERS=8        # parallel processing threads

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}📊 PDF Chunking with Metrics Tracking${NC}"
echo "========================================="
echo "PDF Directory:    $PDF_DIR"
echo "Output Directory: $OUTPUT_DIR"
echo "Chunk CLI:        $CHUNK_CLI"
echo "Max Workers:      $MAX_WORKERS"
echo ""

# Validate inputs
if [ ! -d "$PDF_DIR" ]; then
    echo -e "${RED}❌ PDF directory not found: $PDF_DIR${NC}"
    exit 1
fi

if [ ! -x "$CHUNK_CLI" ]; then
    echo -e "${RED}❌ Chunk CLI not found or not executable: $CHUNK_CLI${NC}"
    exit 1
fi

# Create output directories
mkdir -p "$OUTPUT_DIR"
mkdir -p "$(dirname "$METRICS_FILE")"

# Initialize tracking variables
TOTAL_PDFS=$(find "$PDF_DIR" -name "*.pdf" -type f | wc -l | tr -d ' ')
PROCESSED=0
FAILED=0
SKIPPED=0
START_TIME=$(date +%s)

# Initialize log files
echo "Processing started at $(date)" > "$LOG_FILE"
> "$FAILED_LIST"

# Function to get file size in bytes
get_file_size() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        stat -f%z "$1" 2>/dev/null || echo 0
    else
        stat -c%s "$1" 2>/dev/null || echo 0
    fi
}

# Function to process a single PDF
process_pdf() {
    local pdf_path="$1"
    local pdf_name=$(basename "$pdf_path")
    local output_path="$OUTPUT_DIR/${pdf_name%.pdf}_chunks.json"
    local temp_log=$(mktemp)
    
    # Skip if already processed
    if [ -f "$output_path" ] && [ -s "$output_path" ]; then
        echo "SKIPPED|$pdf_path|Already processed" >> "$LOG_FILE"
        return 2
    fi
    
    # Get file metrics
    local file_size=$(get_file_size "$pdf_path")
    local process_start=$(date +%s%N)
    
    # Run the chunk-pdf-cli tool
    if timeout 120 "$CHUNK_CLI" \
        --input "$pdf_path" \
        --output "$output_path" \
        --max-chunk-size "$MAX_CHUNK_SIZE" \
        --min-chunk-size "$MIN_CHUNK_SIZE" \
        --overlap "$OVERLAP" \
        > "$temp_log" 2>&1; then
        
        local process_end=$(date +%s%N)
        local process_time=$((($process_end - $process_start) / 1000000)) # milliseconds
        
        # Extract metrics from output if available
        local chunks_created=$(grep -c '"text"' "$output_path" 2>/dev/null || echo 0)
        local output_size=$(get_file_size "$output_path")
        
        echo "SUCCESS|$pdf_path|$file_size|$output_size|$chunks_created|$process_time" >> "$LOG_FILE"
        rm -f "$temp_log"
        return 0
    else
        local error_msg=$(tail -n 5 "$temp_log" | tr '\n' ' ')
        echo "FAILED|$pdf_path|$file_size|$error_msg" >> "$LOG_FILE"
        echo "$pdf_path" >> "$FAILED_LIST"
        rm -f "$temp_log"
        return 1
    fi
}

# Function to show progress
show_progress() {
    local current=$1
    local total=$2
    local percent=$((current * 100 / total))
    local filled=$((percent / 2))
    
    printf "\rProgress: ["
    printf "%${filled}s" | tr ' ' '='
    printf "%$((50 - filled))s" | tr ' ' ' '
    printf "] %d%% (%d/%d)" $percent $current $total
}

# Export functions for parallel processing
export -f process_pdf
export -f get_file_size
export PDF_DIR OUTPUT_DIR CHUNK_CLI MAX_CHUNK_SIZE MIN_CHUNK_SIZE OVERLAP LOG_FILE FAILED_LIST

echo -e "${YELLOW}⏳ Processing $TOTAL_PDFS PDF files...${NC}"
echo ""

# Create a temporary file for parallel job tracking
JOBS_FILE=$(mktemp)

# Process PDFs in parallel using xargs
find "$PDF_DIR" -name "*.pdf" -type f | \
    xargs -P "$MAX_WORKERS" -I {} bash -c '
        process_pdf "{}"
        echo $? >> '"$JOBS_FILE"'
    '

# Count results
while IFS= read -r exit_code; do
    case $exit_code in
        0) ((PROCESSED++)) ;;
        1) ((FAILED++)) ;;
        2) ((SKIPPED++)) ;;
    esac
    show_progress $((PROCESSED + FAILED + SKIPPED)) $TOTAL_PDFS
done < "$JOBS_FILE"

echo "" # New line after progress bar

# Calculate final metrics
END_TIME=$(date +%s)
TOTAL_TIME=$((END_TIME - START_TIME))

# Calculate sizes
TOTAL_INPUT_SIZE=$(find "$PDF_DIR" -name "*.pdf" -type f -exec du -cb {} + | tail -1 | cut -f1)
TOTAL_OUTPUT_SIZE=$(find "$OUTPUT_DIR" -name "*_chunks.json" -type f -exec du -cb {} + 2>/dev/null | tail -1 | cut -f1 || echo 0)

# Parse log for detailed metrics
TOTAL_CHUNKS=0
TOTAL_PROCESS_TIME=0
SUCCESS_COUNT=0

while IFS='|' read -r status path input_size output_size chunks time_ms rest; do
    if [ "$status" = "SUCCESS" ]; then
        TOTAL_CHUNKS=$((TOTAL_CHUNKS + chunks))
        TOTAL_PROCESS_TIME=$((TOTAL_PROCESS_TIME + time_ms))
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    fi
done < <(grep "^SUCCESS" "$LOG_FILE")

# Calculate averages
AVG_CHUNKS_PER_DOC=0
AVG_PROCESS_TIME=0
if [ $SUCCESS_COUNT -gt 0 ]; then
    AVG_CHUNKS_PER_DOC=$((TOTAL_CHUNKS / SUCCESS_COUNT))
    AVG_PROCESS_TIME=$((TOTAL_PROCESS_TIME / SUCCESS_COUNT))
fi

# Generate metrics JSON
cat > "$METRICS_FILE" << EOF
{
  "processing_summary": {
    "timestamp": "$(date -Iseconds)",
    "total_pdfs": $TOTAL_PDFS,
    "processed": $PROCESSED,
    "failed": $FAILED,
    "skipped": $SKIPPED,
    "success_rate": $(echo "scale=2; $PROCESSED * 100 / $TOTAL_PDFS" | bc)
  },
  "performance": {
    "total_time_seconds": $TOTAL_TIME,
    "avg_time_per_doc_ms": $AVG_PROCESS_TIME,
    "docs_per_second": $(echo "scale=2; $PROCESSED / $TOTAL_TIME" | bc),
    "parallel_workers": $MAX_WORKERS
  },
  "data_metrics": {
    "total_input_size_mb": $(echo "scale=2; $TOTAL_INPUT_SIZE / 1048576" | bc),
    "total_output_size_mb": $(echo "scale=2; $TOTAL_OUTPUT_SIZE / 1048576" | bc),
    "compression_ratio": $(echo "scale=2; $TOTAL_OUTPUT_SIZE / $TOTAL_INPUT_SIZE" | bc),
    "total_chunks_created": $TOTAL_CHUNKS,
    "avg_chunks_per_document": $AVG_CHUNKS_PER_DOC
  },
  "chunking_parameters": {
    "max_chunk_size": $MAX_CHUNK_SIZE,
    "min_chunk_size": $MIN_CHUNK_SIZE,
    "overlap": $OVERLAP
  }
}
EOF

# Display summary
echo ""
echo -e "${GREEN}✅ Processing Complete!${NC}"
echo "========================================="
echo -e "${BLUE}📊 Summary:${NC}"
echo "  • Total PDFs:        $TOTAL_PDFS"
echo "  • Successfully processed: ${GREEN}$PROCESSED${NC}"
echo "  • Failed:            ${RED}$FAILED${NC}"
echo "  • Skipped:           ${YELLOW}$SKIPPED${NC}"
echo ""
echo -e "${BLUE}⚡ Performance:${NC}"
echo "  • Total time:        ${TOTAL_TIME}s"
echo "  • Docs/second:       $(echo "scale=2; $PROCESSED / $TOTAL_TIME" | bc)"
echo "  • Avg time/doc:      ${AVG_PROCESS_TIME}ms"
echo ""
echo -e "${BLUE}📦 Data:${NC}"
echo "  • Input size:        $(echo "scale=2; $TOTAL_INPUT_SIZE / 1048576" | bc)MB"
echo "  • Output size:       $(echo "scale=2; $TOTAL_OUTPUT_SIZE / 1048576" | bc)MB"
echo "  • Total chunks:      $TOTAL_CHUNKS"
echo "  • Avg chunks/doc:    $AVG_CHUNKS_PER_DOC"
echo ""
echo -e "${BLUE}📁 Output:${NC}"
echo "  • JSON chunks:       $OUTPUT_DIR/"
echo "  • Metrics:           $METRICS_FILE"
echo "  • Log file:          $LOG_FILE"
if [ $FAILED -gt 0 ]; then
    echo "  • Failed PDFs:       $FAILED_LIST"
fi

# Clean up
rm -f "$JOBS_FILE"

# Create a sample inspection script
cat > "samples/inspect_chunks.py" << 'EOF'
#!/usr/bin/env python3
"""Inspect the generated chunks for quality analysis."""

import json
import glob
import sys
from pathlib import Path
from collections import Counter
import statistics

def analyze_chunks(json_dir):
    """Analyze all chunk files in the directory."""
    
    files = list(Path(json_dir).glob("*_chunks.json"))
    
    if not files:
        print(f"No chunk files found in {json_dir}")
        return
    
    total_chunks = 0
    chunk_sizes = []
    docs_with_chunks = 0
    
    for file in files:
        try:
            with open(file, 'r') as f:
                data = json.load(f)
                
            if isinstance(data, list) and data:
                docs_with_chunks += 1
                total_chunks += len(data)
                
                for chunk in data:
                    if 'text' in chunk:
                        chunk_sizes.append(len(chunk['text']))
        except Exception as e:
            print(f"Error reading {file}: {e}")
    
    if chunk_sizes:
        print("\n📊 Chunk Analysis:")
        print(f"  • Total documents with chunks: {docs_with_chunks}")
        print(f"  • Total chunks: {total_chunks}")
        print(f"  • Avg chunks per document: {total_chunks/docs_with_chunks:.1f}")
        print(f"\n📏 Chunk Size Distribution (characters):")
        print(f"  • Min: {min(chunk_sizes):,}")
        print(f"  • Max: {max(chunk_sizes):,}")
        print(f"  • Mean: {statistics.mean(chunk_sizes):,.0f}")
        print(f"  • Median: {statistics.median(chunk_sizes):,.0f}")
        print(f"  • Std Dev: {statistics.stdev(chunk_sizes):,.0f}")
        
        # Show distribution
        buckets = [0, 500, 1000, 2000, 3000, 4000, 5000, 10000, float('inf')]
        distribution = Counter()
        
        for size in chunk_sizes:
            for i in range(len(buckets)-1):
                if buckets[i] <= size < buckets[i+1]:
                    label = f"{buckets[i]}-{buckets[i+1]}" if buckets[i+1] != float('inf') else f"{buckets[i]}+"
                    distribution[label] += 1
                    break
        
        print(f"\n📊 Size Distribution:")
        for range_label, count in sorted(distribution.items()):
            pct = (count/len(chunk_sizes)) * 100
            bar = '█' * int(pct/2)
            print(f"  {range_label:>12}: {bar} {count:5d} ({pct:5.1f}%)")
        
        # Sample a chunk for inspection
        print(f"\n📝 Sample chunk from {files[0].name}:")
        with open(files[0], 'r') as f:
            sample = json.load(f)
            if sample and isinstance(sample, list):
                print(f"  First 500 chars: {sample[0].get('text', '')[:500]}...")

if __name__ == "__main__":
    json_dir = sys.argv[1] if len(sys.argv) > 1 else "samples/json"
    analyze_chunks(json_dir)
EOF

chmod +x samples/inspect_chunks.py

echo ""
echo -e "${YELLOW}💡 To inspect the chunks, run:${NC}"
echo "  python3 samples/inspect_chunks.py $OUTPUT_DIR"

exit 0