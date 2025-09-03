#pragma once

#include <string>
#include <vector>
#include <memory>
#include <nlohmann/json.hpp>

namespace fast_pdf_parser {

// Configuration for PDF chunking
struct ChunkOptions {
    int max_tokens = 512;
    int min_tokens = 150;
    int overlap_tokens = 0;
    int thread_count = 0;  // 0 = use hardware concurrency
};

// Result for a single chunk
struct ChunkResult {
    std::string text;
    int token_count;
    int start_page;
    int end_page;
    bool has_major_heading;
    int min_heading_level;
};

// Result for the entire chunking operation
struct ChunkingResult {
    std::vector<ChunkResult> chunks;
    int total_pages;
    int total_chunks;
    double processing_time_ms;
    std::string error;  // Empty if successful
};

// Main API class for hierarchical PDF chunking
class HierarchicalChunker {
public:
    explicit HierarchicalChunker(const ChunkOptions& options = ChunkOptions{});
    ~HierarchicalChunker();
    
    // Chunk a PDF file
    ChunkingResult chunk_file(const std::string& pdf_path, int page_limit = -1);
    
    // Process a PDF file and save JSON output
    bool process_pdf_to_json(const std::string& pdf_path, const std::string& output_path, int page_limit = -1);
    
    // Get/set options
    ChunkOptions get_options() const;
    void set_options(const ChunkOptions& options);
    
private:
    class Impl;
    std::unique_ptr<Impl> pImpl;
};

} // namespace fast_pdf_parser