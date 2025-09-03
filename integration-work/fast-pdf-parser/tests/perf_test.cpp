#include <fast_pdf_parser/fast_pdf_parser.h>
#include <iostream>
#include <chrono>
#include <atomic>

int main(int argc, char* argv[]) {
    if (argc != 2) {
        std::cerr << "Usage: " << argv[0] << " <input.pdf>\n";
        return 1;
    }

    try {
        // Test different thread counts
        size_t max_threads = std::thread::hardware_concurrency();
        std::vector<size_t> thread_counts = {1, 2, 4};
        if (max_threads >= 8) thread_counts.push_back(8);
        if (max_threads >= 16) thread_counts.push_back(16);
        thread_counts.push_back(max_threads - 1);
        
        for (size_t threads : thread_counts) {
            fast_pdf_parser::ParseOptions options;
            options.thread_count = threads;
            options.batch_size = 10;
            options.extract_positions = false;  // Faster without positions
            options.extract_fonts = false;      // Faster without fonts
            
            fast_pdf_parser::FastPdfParser parser(options);
            
            std::cout << "\n=== Testing with " << threads << " threads ===" << std::endl;
            auto start = std::chrono::high_resolution_clock::now();
            
            std::atomic<size_t> page_count{0};
            std::atomic<size_t> error_count{0};
            
            parser.parse_streaming(argv[1], [&page_count, &error_count](fast_pdf_parser::PageResult result) -> bool {
                if (result.success) {
                    page_count++;
                    if (page_count % 100 == 0) {
                        std::cout << "Processed " << page_count << " pages..." << std::endl;
                    }
                } else {
                    error_count++;
                }
                return true; // Continue processing all pages
            });
            
            auto end = std::chrono::high_resolution_clock::now();
            auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(end - start);
            
            std::cout << "Processed " << page_count << " pages in " << duration.count() << "ms" << std::endl;
            std::cout << "Errors: " << error_count << std::endl;
            
            double pages_per_second = (page_count * 1000.0) / duration.count();
            std::cout << "Performance: " << pages_per_second << " pages/second" << std::endl;
            
            auto stats = parser.get_stats();
            std::cout << "Parser stats - pages/sec: " << stats["pages_per_second"] << std::endl;
        }
        
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
        return 1;
    }
    
    return 0;
}