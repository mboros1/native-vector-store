#pragma once

#include <string>
#include <vector>
#include <memory>
#include <functional>
#include <thread>
#include <nlohmann/json.hpp>

namespace fast_pdf_parser {

struct ParseOptions {
    size_t thread_count = std::thread::hardware_concurrency();
    size_t max_memory_per_page = 50 * 1024 * 1024; // 50MB
    bool extract_positions = true;
    bool extract_fonts = true;
    bool extract_colors = false;
    size_t batch_size = 10; // pages per batch
};

struct PageResult {
    int page_number;
    nlohmann::json content;
    std::string error;
    bool success;
};

using ProgressCallback = std::function<void(size_t current, size_t total)>;
using PageCallback = std::function<bool(PageResult)>; // Return false to stop processing

class FastPdfParser {
public:
    explicit FastPdfParser(const ParseOptions& options = ParseOptions{});
    ~FastPdfParser();

    // Single document parsing
    nlohmann::json parse(const std::string& pdf_path);
    
    // Streaming parse with callback for each page
    void parse_streaming(const std::string& pdf_path, PageCallback callback);
    
    // Batch processing of multiple documents
    std::vector<nlohmann::json> parse_batch(const std::vector<std::string>& pdf_paths,
                                           ProgressCallback progress = nullptr);

    // Get parser statistics
    nlohmann::json get_stats() const;

private:
    class Impl;
    std::unique_ptr<Impl> pImpl;
};

} // namespace fast_pdf_parser