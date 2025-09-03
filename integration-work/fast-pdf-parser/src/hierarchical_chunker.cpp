#include <fast_pdf_parser/hierarchical_chunker.h>
#include <fast_pdf_parser/fast_pdf_parser.h>
#include <fast_pdf_parser/tiktoken_tokenizer.h>
#include <iostream>
#include <fstream>
#include <chrono>
#include <filesystem>
#include <nlohmann/json.hpp>
#include <regex>
#include <algorithm>
#include <numeric>
#include <iomanip>
#include <set>

namespace fs = std::filesystem;
using namespace fast_pdf_parser;

// Configuration constants
const int DEFAULT_MAX_TOKENS = 512;
const int DEFAULT_OVERLAP_TOKENS = 50;
const int DEFAULT_MIN_TOKENS = 150;  // Aggressive minimum to avoid small chunks

// Annotation types
enum class LineType {
    NORMAL,
    MAJOR_HEADING,  // #, ## level headings
    MINOR_HEADING,  // ###+ level headings
    LIST_ITEM,
    BLANK,
    CODE_BLOCK
};

// Annotated line structure
struct AnnotatedLine {
    std::string text;
    LineType type;
    int tokens;
    int page;
    int heading_level = 0;  // 0 = not a heading, 1 = #, 2 = ##, etc.
};

// Semantic unit - a group of related lines
struct SemanticUnit {
    std::vector<AnnotatedLine> lines;
    int total_tokens = 0;
    std::set<int> pages;
    bool has_major_heading = false;
    int max_heading_level = 999;  // Lower is more important
    
    void add_line(const AnnotatedLine& line) {
        lines.push_back(line);
        total_tokens += line.tokens;
        pages.insert(line.page);
        if (line.type == LineType::MAJOR_HEADING) {
            has_major_heading = true;
            max_heading_level = std::min(max_heading_level, line.heading_level);
        }
    }
    
    std::string get_text() const {
        std::string result;
        for (const auto& line : lines) {
            result += line.text + "\n";
        }
        return result;
    }
};

// Chunk structure
struct Chunk {
    std::string text;
    int tokens = 0;
    int start_page = -1;
    int end_page = -1;
    std::string overlap_text;
    int overlap_tokens = 0;
    bool has_major_heading = false;
    int min_heading_level = 999;  // Track most important heading
};

// Helper function to detect line type and heading level
std::pair<LineType, int> detect_line_type(const std::string& line) {
    // Check for blank line
    if (line.empty() || std::all_of(line.begin(), line.end(), ::isspace)) {
        return {LineType::BLANK, 0};
    }
    
    // Check for markdown headings
    std::regex heading_regex("^(#+)\\s+(.+)$");
    std::smatch match;
    if (std::regex_match(line, match, heading_regex)) {
        int level = match[1].str().length();
        if (level <= 2) {
            return {LineType::MAJOR_HEADING, level};
        } else {
            return {LineType::MINOR_HEADING, level};
        }
    }
    
    // Check for list items
    std::regex list_regex("^\\s*[-*+•]\\s+(.+)$|^\\s*\\d+\\.\\s+(.+)$");
    if (std::regex_match(line, list_regex)) {
        return {LineType::LIST_ITEM, 0};
    }
    
    // Check for code blocks (simple heuristic)
    if (line.find("```") != std::string::npos || 
        (line.length() > 0 && line[0] == ' ' && line.find("  ") == 0)) {
        return {LineType::CODE_BLOCK, 0};
    }
    
    return {LineType::NORMAL, 0};
}

// Pass 1: Annotate lines with type and token count
std::vector<AnnotatedLine> annotate_lines(const std::vector<std::pair<std::string, int>>& pages,
                                          const TiktokenTokenizer& tokenizer) {
    std::vector<AnnotatedLine> annotated;
    
    for (const auto& [page_text, page_num] : pages) {
        std::istringstream stream(page_text);
        std::string line;
        
        while (std::getline(stream, line)) {
            auto [type, level] = detect_line_type(line);
            int tokens = tokenizer.count_tokens(line);
            
            annotated.push_back({
                line,
                type,
                tokens,
                page_num,
                level
            });
        }
    }
    
    return annotated;
}

// Pass 2: Group lines into semantic units
std::vector<SemanticUnit> create_semantic_units(const std::vector<AnnotatedLine>& lines) {
    std::vector<SemanticUnit> units;
    SemanticUnit current_unit;
    
    for (size_t i = 0; i < lines.size(); ++i) {
        const auto& line = lines[i];
        
        // Start new unit on major headings or after blanks before headings
        bool should_break = false;
        
        if (line.type == LineType::MAJOR_HEADING || line.type == LineType::MINOR_HEADING) {
            should_break = true;
        } else if (line.type == LineType::BLANK && i + 1 < lines.size()) {
            // Look ahead - if next line is a heading, break here
            if (lines[i + 1].type == LineType::MAJOR_HEADING || 
                lines[i + 1].type == LineType::MINOR_HEADING) {
                should_break = true;
            }
        }
        
        if (should_break && !current_unit.lines.empty()) {
            units.push_back(current_unit);
            current_unit = SemanticUnit();
        }
        
        // Skip blank lines at unit boundaries
        if (!(line.type == LineType::BLANK && current_unit.lines.empty())) {
            current_unit.add_line(line);
        }
    }
    
    // Don't forget the last unit
    if (!current_unit.lines.empty()) {
        units.push_back(current_unit);
    }
    
    return units;
}

// Pass 3: Create initial chunks from semantic units
std::vector<Chunk> create_initial_chunks(const std::vector<SemanticUnit>& units,
                                         int max_tokens) {
    std::vector<Chunk> chunks;
    Chunk current_chunk;
    
    for (const auto& unit : units) {
        // If adding this unit would exceed max_tokens, start a new chunk
        // Exception: if current chunk is empty, add it anyway (unit > max_tokens)
        if (!current_chunk.text.empty() && 
            current_chunk.tokens + unit.total_tokens > max_tokens) {
            chunks.push_back(current_chunk);
            current_chunk = Chunk();
        }
        
        // Add unit to current chunk
        current_chunk.text += unit.get_text();
        current_chunk.tokens += unit.total_tokens;
        
        // Update page range
        if (!unit.pages.empty()) {
            if (current_chunk.start_page == -1) {
                current_chunk.start_page = *unit.pages.begin();
            }
            current_chunk.end_page = *unit.pages.rbegin();
        }
        
        // Track heading information
        if (unit.has_major_heading) {
            current_chunk.has_major_heading = true;
            current_chunk.min_heading_level = std::min(current_chunk.min_heading_level, 
                                                       unit.max_heading_level);
        }
    }
    
    // Don't forget the last chunk
    if (!current_chunk.text.empty()) {
        chunks.push_back(current_chunk);
    }
    
    return chunks;
}

// Pass 4: Add overlap text
std::vector<Chunk> add_overlap(std::vector<Chunk>& chunks, 
                               int overlap_tokens,
                               const TiktokenTokenizer& tokenizer) {
    for (size_t i = 1; i < chunks.size(); ++i) {
        // Extract last overlap_tokens worth of text from previous chunk
        const std::string& prev_text = chunks[i-1].text;
        
        // Simple approach: take last N characters and count tokens
        size_t chars_to_take = std::min(prev_text.length(), size_t(overlap_tokens * 5));
        std::string overlap_text = prev_text.substr(prev_text.length() - chars_to_take);
        
        // Trim to actual token count
        while (tokenizer.count_tokens(overlap_text) > overlap_tokens && overlap_text.length() > 10) {
            overlap_text = overlap_text.substr(10);
        }
        
        chunks[i].overlap_text = overlap_text;
        chunks[i].overlap_tokens = tokenizer.count_tokens(overlap_text);
    }
    
    return chunks;
}

// Pass 5: Enhanced merging with heuristics
std::vector<Chunk> merge_small_chunks_hierarchically(const std::vector<Chunk>& chunks,
                                                     int min_tokens,
                                                     int max_tokens,
                                                     const TiktokenTokenizer& tokenizer) {
    if (chunks.empty()) return chunks;
    
    std::vector<Chunk> merged;
    size_t i = 0;
    
    while (i < chunks.size()) {
        Chunk current = chunks[i];
        
        // Try to merge with following chunks if current is small
        while (current.tokens < min_tokens && i + 1 < chunks.size()) {
            const Chunk& next = chunks[i + 1];
            
            // Calculate combined size
            int combined_tokens = current.tokens + next.tokens;
            
            // Merge decision based on multiple factors
            bool should_merge = false;
            
            // 1. Always merge if combined size is reasonable
            if (combined_tokens <= max_tokens) {
                should_merge = true;
            }
            // 2. Allow slightly over if it prevents tiny chunks
            else if (combined_tokens <= max_tokens * 1.1 && next.tokens < min_tokens / 2) {
                should_merge = true;
            }
            
            // Veto merging if next has major heading and current is already reasonable size
            if (next.has_major_heading && next.min_heading_level <= 2 && current.tokens >= min_tokens / 2) {
                should_merge = false;
            }
            
            if (!should_merge) break;
            
            // Perform merge
            current.text += next.text;
            current.tokens = combined_tokens;
            current.end_page = next.end_page;
            if (next.has_major_heading) {
                current.has_major_heading = true;
                current.min_heading_level = std::min(current.min_heading_level, next.min_heading_level);
            }
            
            i++;  // Skip the merged chunk
        }
        
        merged.push_back(current);
        i++;
    }
    
    return merged;
}

// Pass 6: Split oversized chunks at semantic boundaries
std::vector<Chunk> split_oversized_chunks(const std::vector<Chunk>& chunks,
                                          int max_tokens,
                                          const TiktokenTokenizer& tokenizer) {
    std::vector<Chunk> result;
    
    for (const auto& chunk : chunks) {
        if (chunk.tokens <= max_tokens) {
            result.push_back(chunk);
            continue;
        }
        
        // Need to split this chunk
        std::istringstream stream(chunk.text);
        std::string line;
        Chunk current_split;
        current_split.start_page = chunk.start_page;
        
        while (std::getline(stream, line)) {
            int line_tokens = tokenizer.count_tokens(line);
            
            // Check if adding this line would exceed limit
            if (!current_split.text.empty() && 
                current_split.tokens + line_tokens > max_tokens) {
                
                // Look for semantic boundary (prefer line breaks, sentences)
                if (current_split.tokens >= max_tokens * 0.8) {
                    // Close enough to target, split here
                    current_split.end_page = chunk.end_page; // Approximate
                    result.push_back(current_split);
                    
                    current_split = Chunk();
                    current_split.start_page = chunk.start_page; // Approximate
                }
            }
            
            current_split.text += line + "\n";
            current_split.tokens += line_tokens;
        }
        
        // Add final split
        if (!current_split.text.empty()) {
            current_split.end_page = chunk.end_page;
            result.push_back(current_split);
        }
    }
    
    return result;
}

// Pass 7: Final merge pass to eliminate small chunks created by splitting
std::vector<Chunk> final_merge_pass(const std::vector<Chunk>& chunks,
                                    int min_tokens,
                                    int max_tokens,
                                    const TiktokenTokenizer& tokenizer) {
    if (chunks.empty()) return chunks;
    
    std::vector<Chunk> final_chunks;
    size_t i = 0;
    
    while (i < chunks.size()) {
        Chunk current = chunks[i];
        
        // Merge small chunks while respecting max_tokens limit strictly
        while (current.tokens < min_tokens && i + 1 < chunks.size()) {
            const Chunk& next = chunks[i + 1];
            int combined_tokens = current.tokens + next.tokens;
            
            // STRICT limit - no oversizing allowed in final pass
            if (combined_tokens <= max_tokens) {
                current.text += next.text;
                current.tokens = combined_tokens;
                current.end_page = next.end_page;
                if (next.has_major_heading) {
                    current.has_major_heading = true;
                    current.min_heading_level = std::min(current.min_heading_level, next.min_heading_level);
                }
                i++;
            } else {
                // Can't merge forward without exceeding limit
                break;
            }
        }
        
        // Try to merge with previous chunk if current is still small
        if (current.tokens < min_tokens && !final_chunks.empty()) {
            Chunk& prev = final_chunks.back();
            int combined_tokens = prev.tokens + current.tokens;
            
            // STRICT limit - no oversizing allowed in final pass
            if (combined_tokens <= max_tokens) {
                prev.text += current.text;
                prev.tokens = combined_tokens;
                prev.end_page = current.end_page;
                if (current.has_major_heading) {
                    prev.has_major_heading = true;
                    prev.min_heading_level = std::min(prev.min_heading_level, current.min_heading_level);
                }
                i++;
                continue;  // Skip adding current
            }
        }
        
        final_chunks.push_back(current);
        i++;
    }
    
    return final_chunks;
}

// Internal chunking function
static std::vector<Chunk> create_hierarchical_chunks_internal(const std::vector<std::pair<std::string, int>>& pages,
                                                              const TiktokenTokenizer& tokenizer,
                                                              int max_tokens = DEFAULT_MAX_TOKENS,
                                                              int overlap_tokens = DEFAULT_OVERLAP_TOKENS,
                                                              int min_tokens = DEFAULT_MIN_TOKENS) {
    
    // Filter out empty pages
    std::vector<std::pair<std::string, int>> non_empty_pages;
    for (const auto& [text, page_num] : pages) {
        if (!text.empty()) {
            non_empty_pages.push_back({text, page_num});
        }
    }
    
    if (non_empty_pages.empty()) {
        return {};
    }
    
    // Pass 1: Annotate lines
    auto annotated_lines = annotate_lines(non_empty_pages, tokenizer);
    
    // Pass 2: Create semantic units
    auto semantic_units = create_semantic_units(annotated_lines);
    
    // Pass 3: Create initial chunks
    auto chunks = create_initial_chunks(semantic_units, max_tokens);
    
    // Pass 4: Add overlap
    chunks = add_overlap(chunks, overlap_tokens, tokenizer);
    
    // Pass 5: Merge small chunks
    chunks = merge_small_chunks_hierarchically(chunks, min_tokens, max_tokens, tokenizer);
    
    // Pass 6: Split oversized chunks
    chunks = split_oversized_chunks(chunks, max_tokens, tokenizer);
    
    // Pass 7: Final merge pass to eliminate any small chunks created by splitting
    chunks = final_merge_pass(chunks, min_tokens, max_tokens, tokenizer);
    
    // Final recalculation of tokens to ensure accuracy
    for (auto& chunk : chunks) {
        chunk.tokens = tokenizer.count_tokens(chunk.text);
    }
    
    return chunks;
}

static void analyze_chunk_distribution(const std::vector<Chunk>& chunks) {
    if (chunks.empty()) {
        std::cout << "No chunks to analyze\n";
        return;
    }
    
    // Collect token counts
    std::vector<int> token_counts;
    for (const auto& chunk : chunks) {
        token_counts.push_back(chunk.tokens);
    }
    
    // Sort for quintile analysis
    std::sort(token_counts.begin(), token_counts.end());
    
    // Calculate quintiles
    auto get_quintile = [&](double percentile) -> int {
        size_t index = static_cast<size_t>(percentile * (token_counts.size() - 1));
        return token_counts[index];
    };
    
    std::cout << "\n=== Final Chunk Distribution Analysis ===\n";
    std::cout << "Total chunks: " << chunks.size() << "\n";
    std::cout << "Min tokens: " << token_counts.front() << "\n";
    std::cout << "Max tokens: " << token_counts.back() << "\n";
    std::cout << "Average tokens: " << std::accumulate(token_counts.begin(), token_counts.end(), 0) / chunks.size() << "\n";
    
    std::cout << "\nQuintiles:\n";
    std::cout << "  20th percentile: " << get_quintile(0.2) << " tokens\n";
    std::cout << "  40th percentile: " << get_quintile(0.4) << " tokens\n";
    std::cout << "  60th percentile: " << get_quintile(0.6) << " tokens\n";
    std::cout << "  80th percentile: " << get_quintile(0.8) << " tokens\n";
    
    // Token range distribution
    std::map<std::string, int> distribution;
    for (int tokens : token_counts) {
        if (tokens <= 50) distribution["1-50"]++;
        else if (tokens <= 100) distribution["51-100"]++;
        else if (tokens <= 150) distribution["101-150"]++;
        else if (tokens <= 200) distribution["151-200"]++;
        else if (tokens <= 300) distribution["201-300"]++;
        else if (tokens <= 400) distribution["301-400"]++;
        else if (tokens <= 500) distribution["401-500"]++;
        else if (tokens <= 512) distribution["501-512"]++;
        else distribution["513+"]++;
    }
    
    std::cout << "\nToken Range Distribution:\n";
    for (const auto& [range, count] : distribution) {
        double percentage = (count * 100.0) / chunks.size();
        std::cout << "  " << range << " tokens: " << count << " chunks (" 
                  << std::fixed << std::setprecision(1) << percentage << "%)\n";
    }
    
    // Check for problematic small chunks
    int small_chunks = 0;
    for (int tokens : token_counts) {
        if (tokens < DEFAULT_MIN_TOKENS) {
            small_chunks++;
        }
    }
    
    if (small_chunks > 0) {
        std::cout << "\nWARNING: " << small_chunks << " chunks are below the minimum threshold of " 
                  << DEFAULT_MIN_TOKENS << " tokens\n";
    } else {
        std::cout << "\nSUCCESS: All chunks meet the minimum threshold of " 
                  << DEFAULT_MIN_TOKENS << " tokens\n";
    }
}

// Implementation of HierarchicalChunker class
namespace fast_pdf_parser {

class HierarchicalChunker::Impl {
public:
    ChunkOptions options;
    TiktokenTokenizer tokenizer;
    
    Impl(const ChunkOptions& opts) : options(opts) {}
};

HierarchicalChunker::HierarchicalChunker(const ChunkOptions& options) 
    : pImpl(std::make_unique<Impl>(options)) {
}

HierarchicalChunker::~HierarchicalChunker() = default;

ChunkingResult HierarchicalChunker::chunk_file(const std::string& pdf_path, int page_limit) {
    ChunkingResult result;
    auto start_time = std::chrono::high_resolution_clock::now();
    
    try {
        // Set up parser options
        ParseOptions parse_opts;
        parse_opts.thread_count = pImpl->options.thread_count > 0 ? 
            pImpl->options.thread_count : std::thread::hardware_concurrency();
        parse_opts.batch_size = 10;
        parse_opts.extract_positions = false;
        parse_opts.extract_fonts = false;
        
        FastPdfParser parser(parse_opts);
        
        // Extract pages
        std::vector<std::pair<std::string, int>> pages;
        int page_count = 0;
        
        parser.parse_streaming(pdf_path, [&](PageResult page_result) -> bool {
            if (!page_result.success) {
                return true; // Continue despite individual page errors
            }
            
            page_count++;
            
            // Extract text from the page result - properly formatted
            std::string page_text;
            if (page_result.content.contains("blocks")) {
                for (const auto& block : page_result.content["blocks"]) {
                    for (const auto& line : block["lines"]) {
                        if (!page_text.empty()) page_text += "\n";
                        page_text += line["text"].get<std::string>();
                    }
                }
            }
            
            pages.push_back({page_text, page_result.page_number});
            
            // Stop if we've hit the page limit
            if (page_limit > 0 && page_count >= page_limit) {
                return false;
            }
            
            return true;
        });
        
        result.total_pages = page_count;
        
        // Create chunks
        auto chunks = create_hierarchical_chunks_internal(
            pages,
            pImpl->tokenizer,
            pImpl->options.max_tokens,
            pImpl->options.overlap_tokens,
            pImpl->options.min_tokens
        );
        
        // Convert to ChunkResult
        for (const auto& chunk : chunks) {
            ChunkResult chunk_result;
            chunk_result.text = chunk.text;
            chunk_result.token_count = chunk.tokens;
            chunk_result.start_page = chunk.start_page;
            chunk_result.end_page = chunk.end_page;
            chunk_result.has_major_heading = chunk.has_major_heading;
            chunk_result.min_heading_level = chunk.min_heading_level;
            
            result.chunks.push_back(chunk_result);
        }
        
        result.total_chunks = result.chunks.size();
        
    } catch (const std::exception& e) {
        result.error = std::string("Error chunking PDF: ") + e.what();
    }
    
    auto end_time = std::chrono::high_resolution_clock::now();
    auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(end_time - start_time);
    result.processing_time_ms = duration.count();
    
    return result;
}

bool HierarchicalChunker::process_pdf_to_json(const std::string& pdf_path, const std::string& output_path, int page_limit) {
    try {
        auto result = chunk_file(pdf_path, page_limit);
        
        if (!result.error.empty()) {
            return false;
        }
        
        // Generate JSON output
        nlohmann::json output = nlohmann::json::array();
        std::hash<std::string> hasher;
        int64_t file_hash = static_cast<int64_t>(hasher(pdf_path));
        
        for (size_t i = 0; i < result.chunks.size(); ++i) {
            const auto& chunk = result.chunks[i];
            
            nlohmann::json chunk_json;
            chunk_json["text"] = chunk.text;
            
            nlohmann::json meta;
            meta["schema_name"] = "docling_core.transforms.chunker.DocMeta";
            meta["version"] = "1.0.0";
            meta["start_page"] = chunk.start_page;
            meta["end_page"] = chunk.end_page;
            meta["page_count"] = chunk.end_page - chunk.start_page + 1;
            meta["chunk_index"] = static_cast<int>(i);
            meta["total_chunks"] = result.total_chunks;
            meta["token_count"] = chunk.token_count;
            meta["has_major_heading"] = chunk.has_major_heading;
            meta["min_heading_level"] = chunk.min_heading_level;
            
            nlohmann::json origin;
            origin["mimetype"] = "application/pdf";
            origin["binary_hash"] = file_hash;
            origin["filename"] = fs::path(pdf_path).filename().string();
            origin["uri"] = nullptr;
            
            meta["origin"] = origin;
            meta["doc_items"] = nlohmann::json::array();
            meta["headings"] = nlohmann::json::array();
            meta["captions"] = nullptr;
            
            chunk_json["meta"] = meta;
            output.push_back(chunk_json);
        }
        
        std::ofstream outfile(output_path);
        outfile << output.dump(2);
        outfile.close();
        
        return true;
        
    } catch (const std::exception& e) {
        return false;
    }
}

ChunkOptions HierarchicalChunker::get_options() const {
    return pImpl->options;
}

void HierarchicalChunker::set_options(const ChunkOptions& options) {
    pImpl->options = options;
}

} // namespace fast_pdf_parser

// Unit tests  
#ifdef ENABLE_TESTS
#include "../deps/doctest.h"

TEST_CASE("HierarchicalChunker text chunking") {
    using namespace fast_pdf_parser;
    
    SUBCASE("Basic chunking functionality") {
        ChunkOptions opts;
        opts.max_tokens = 100;
        opts.min_tokens = 50;
        opts.overlap_tokens = 10;
        
        HierarchicalChunker chunker(opts);
        CHECK(chunker.get_options().max_tokens == 100);
        CHECK(chunker.get_options().min_tokens == 50);
        CHECK(chunker.get_options().overlap_tokens == 10);
    }
    
    SUBCASE("Options modification") {
        ChunkOptions initial_opts;
        initial_opts.max_tokens = 512;
        
        HierarchicalChunker chunker(initial_opts);
        
        ChunkOptions new_opts;
        new_opts.max_tokens = 1024;
        new_opts.min_tokens = 200;
        
        chunker.set_options(new_opts);
        auto current = chunker.get_options();
        
        CHECK(current.max_tokens == 1024);
        CHECK(current.min_tokens == 200);
    }
}

TEST_CASE("ChunkResult structure") {
    using namespace fast_pdf_parser;
    
    SUBCASE("ChunkResult fields") {
        ChunkResult chunk;
        chunk.text = "Test text";
        chunk.token_count = 10;
        chunk.start_page = 0;
        chunk.end_page = 1;
        chunk.has_major_heading = true;
        chunk.min_heading_level = 2;
        
        CHECK(chunk.text == "Test text");
        CHECK(chunk.token_count == 10);
        CHECK(chunk.start_page == 0);
        CHECK(chunk.end_page == 1);
        CHECK(chunk.has_major_heading == true);
        CHECK(chunk.min_heading_level == 2);
    }
}

TEST_CASE("ChunkingResult structure") {
    using namespace fast_pdf_parser;
    
    SUBCASE("ChunkingResult fields") {
        ChunkingResult result;
        result.total_pages = 10;
        result.total_chunks = 5;
        result.processing_time_ms = 1000;
        result.error = "";
        
        ChunkResult chunk;
        chunk.text = "Sample chunk";
        chunk.token_count = 50;
        result.chunks.push_back(chunk);
        
        CHECK(result.total_pages == 10);
        CHECK(result.total_chunks == 5);
        CHECK(result.processing_time_ms == 1000);
        CHECK(result.error.empty());
        CHECK(result.chunks.size() == 1);
        CHECK(result.chunks[0].text == "Sample chunk");
    }
}
#endif // ENABLE_TESTS

