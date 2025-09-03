#include <iostream>
#include <chrono>
#include <vector>
#include <string>
#include "../include/fast_pdf_parser/tiktoken_tokenizer.h"

using namespace fast_pdf_parser;
using namespace std::chrono;

// Generate test data
std::vector<std::pair<std::string, int>> generate_test_pages(int num_pages) {
    std::vector<std::pair<std::string, int>> pages;
    
    for (int i = 1; i <= num_pages; ++i) {
        std::string content = "# Chapter " + std::to_string(i) + "\n\n";
        content += "This is the introduction to chapter " + std::to_string(i) + ".\n\n";
        
        // Add sections
        for (int j = 1; j <= 3; ++j) {
            content += "## Section " + std::to_string(i) + "." + std::to_string(j) + "\n\n";
            
            // Add paragraphs
            for (int k = 1; k <= 5; ++k) {
                content += "This is paragraph " + std::to_string(k) + " of section " + std::to_string(j) + ". ";
                content += "It contains some sample text to demonstrate the chunking algorithm. ";
                content += "The text should be long enough to have meaningful token counts. ";
                content += "We want to ensure that the tokenizer properly counts tokens across various text structures.\n\n";
            }
        }
        
        pages.push_back({content, i});
    }
    
    return pages;
}

int main() {
    std::cout << "=== Chunking Performance Benchmark ===\n\n";
    
    TiktokenTokenizer tokenizer;
    
    // Test different page counts
    std::vector<int> page_counts = {10, 50, 100, 500, 1000};
    
    for (int num_pages : page_counts) {
        auto pages = generate_test_pages(num_pages);
        
        // Calculate total text size
        size_t total_chars = 0;
        for (const auto& [text, _] : pages) {
            total_chars += text.length();
        }
        
        std::cout << "Testing with " << num_pages << " pages (" << total_chars / 1024 << " KB):\n";
        
        // Time the tokenization only
        auto start = high_resolution_clock::now();
        
        size_t total_tokens = 0;
        for (const auto& [text, _] : pages) {
            total_tokens += tokenizer.count_tokens(text);
        }
        
        auto end = high_resolution_clock::now();
        auto duration = duration_cast<microseconds>(end - start);
        
        double tokens_per_second = (total_tokens * 1000000.0) / duration.count();
        double mb_per_second = (total_chars * 1000000.0) / (duration.count() * 1024 * 1024);
        
        std::cout << "  Total tokens: " << total_tokens << "\n";
        std::cout << "  Time: " << duration.count() / 1000.0 << " ms\n";
        std::cout << "  Performance: " << static_cast<int>(tokens_per_second) << " tokens/second\n";
        std::cout << "  Throughput: " << mb_per_second << " MB/second\n\n";
    }
    
    return 0;
}