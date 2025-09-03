#pragma once

#include <string>
#include <memory>
#include <nlohmann/json.hpp>

namespace fast_pdf_parser {

struct ExtractOptions {
    bool extract_positions = true;
    bool extract_fonts = true;
    bool extract_colors = false;
    bool structured_output = true;
};

class TextExtractor {
public:
    TextExtractor();
    ~TextExtractor();

    nlohmann::json extract_page(const std::string& pdf_path, int page_number, 
                               const ExtractOptions& options = ExtractOptions{});
    
    nlohmann::json extract_all_pages(const std::string& pdf_path,
                                    const ExtractOptions& options = ExtractOptions{});
    
    int get_page_count(const std::string& pdf_path);

private:
    class Impl;
    std::unique_ptr<Impl> pImpl;
};

} // namespace fast_pdf_parser