#include <gtest/gtest.h>
#include <fast_pdf_parser/fast_pdf_parser.h>
#include <filesystem>
#include <fstream>

namespace fs = std::filesystem;

class FastPdfParserTest : public ::testing::Test {
protected:
    void SetUp() override {
        // Create a simple test PDF if needed
        test_pdf_path_ = "test_data/test.pdf";
    }
    
    std::string test_pdf_path_;
};

TEST_F(FastPdfParserTest, DefaultConstruction) {
    EXPECT_NO_THROW(fast_pdf_parser::FastPdfParser parser);
}

TEST_F(FastPdfParserTest, CustomOptions) {
    fast_pdf_parser::ParseOptions options;
    options.thread_count = 4;
    options.max_memory_per_page = 100 * 1024 * 1024;
    options.extract_positions = false;
    
    EXPECT_NO_THROW(fast_pdf_parser::FastPdfParser parser(options));
}

TEST_F(FastPdfParserTest, ParseNonExistentFile) {
    fast_pdf_parser::FastPdfParser parser;
    EXPECT_THROW(parser.parse("non_existent.pdf"), std::runtime_error);
}

TEST_F(FastPdfParserTest, ParseValidPdf) {
    if (!fs::exists(test_pdf_path_)) {
        GTEST_SKIP() << "Test PDF not found";
    }
    
    fast_pdf_parser::FastPdfParser parser;
    nlohmann::json result;
    ASSERT_NO_THROW(result = parser.parse(test_pdf_path_));
    
    // Check basic structure
    EXPECT_TRUE(result.contains("content"));
    EXPECT_TRUE(result.contains("meta"));
    EXPECT_TRUE(result["meta"].contains("origin"));
    EXPECT_TRUE(result["meta"]["origin"].contains("filename"));
}

TEST_F(FastPdfParserTest, StreamingParse) {
    if (!fs::exists(test_pdf_path_)) {
        GTEST_SKIP() << "Test PDF not found";
    }
    
    fast_pdf_parser::FastPdfParser parser;
    std::vector<fast_pdf_parser::PageResult> results;
    
    parser.parse_streaming(test_pdf_path_, [&results](fast_pdf_parser::PageResult result) {
        results.push_back(result);
    });
    
    EXPECT_FALSE(results.empty());
    for (const auto& result : results) {
        if (result.success) {
            EXPECT_GE(result.page_number, 0);
            EXPECT_FALSE(result.content.empty());
        }
    }
}

TEST_F(FastPdfParserTest, BatchProcessing) {
    std::vector<std::string> test_files;
    
    // Add test files if they exist
    if (fs::exists(test_pdf_path_)) {
        test_files.push_back(test_pdf_path_);
        test_files.push_back(test_pdf_path_); // Add twice to test batch
    }
    
    if (test_files.empty()) {
        GTEST_SKIP() << "No test PDFs found";
    }
    
    fast_pdf_parser::FastPdfParser parser;
    size_t progress_calls = 0;
    
    auto results = parser.parse_batch(test_files, 
        [&progress_calls](size_t current, size_t total) {
            progress_calls++;
            EXPECT_LE(current, total);
        });
    
    EXPECT_EQ(results.size(), test_files.size());
    EXPECT_GT(progress_calls, 0);
}

TEST_F(FastPdfParserTest, Statistics) {
    if (!fs::exists(test_pdf_path_)) {
        GTEST_SKIP() << "Test PDF not found";
    }
    
    fast_pdf_parser::FastPdfParser parser;
    
    // Parse a file
    parser.parse(test_pdf_path_);
    
    auto stats = parser.get_stats();
    EXPECT_GT(stats["documents_processed"], 0);
    EXPECT_GT(stats["pages_processed"], 0);
    EXPECT_GT(stats["total_processing_time_ms"], 0);
    EXPECT_GT(stats["pages_per_second"], 0);
}

TEST_F(FastPdfParserTest, ThreadPoolScaling) {
    if (!fs::exists(test_pdf_path_)) {
        GTEST_SKIP() << "Test PDF not found";
    }
    
    // Test with different thread counts
    std::vector<size_t> thread_counts = {1, 2, 4, 8};
    
    for (size_t threads : thread_counts) {
        fast_pdf_parser::ParseOptions options;
        options.thread_count = threads;
        
        fast_pdf_parser::FastPdfParser parser(options);
        ASSERT_NO_THROW(parser.parse(test_pdf_path_));
        
        auto stats = parser.get_stats();
        EXPECT_GT(stats["pages_per_second"], 0);
    }
}