#include <fast_pdf_parser/hierarchical_chunker.h>
#include <iostream>
#include <filesystem>
#include <chrono>
#include <iomanip>
#include <thread>
#include <numeric>
#include <algorithm>
#include <map>
#include <string>
#include <vector>
#include <getopt.h>
#include <fstream>

namespace fs = std::filesystem;
using namespace fast_pdf_parser;

struct CLIOptions {
    std::string input_file;
    std::string output_file;
    int max_chunk_size = 512;
    int min_chunk_size = 150;
    int overlap = 0;
    int page_limit = 0;
    int thread_count = 0;  // 0 = auto
    bool verbose = false;
    bool quiet = false;
    bool analyze = true;
    bool help = false;
    bool version = false;
};

void print_usage(const char* program_name) {
    std::cout << "Usage: " << program_name << " [OPTIONS]\n";
    std::cout << "\nRequired:\n";
    std::cout << "  -i, --input FILE           Input PDF file path\n";
    std::cout << "\nOptional:\n";
    std::cout << "  -o, --output FILE          Output JSON file path (default: auto-generated)\n";
    std::cout << "  --max-chunk-size N         Maximum tokens per chunk (default: 512)\n";
    std::cout << "  --min-chunk-size N         Minimum tokens per chunk (default: 150)\n";
    std::cout << "  --overlap N                Token overlap between chunks (default: 0)\n";
    std::cout << "  --page-limit N             Process only first N pages (default: all)\n";
    std::cout << "  --threads N                Number of threads (default: auto-detect)\n";
    std::cout << "  -v, --verbose              Verbose output\n";
    std::cout << "  -q, --quiet                Quiet mode (minimal output)\n";
    std::cout << "  --no-analyze               Skip chunk distribution analysis\n";
    std::cout << "  -h, --help                 Show this help message\n";
    std::cout << "  --version                  Show version information\n";
    std::cout << "\nExamples:\n";
    std::cout << "  " << program_name << " -i document.pdf\n";
    std::cout << "  " << program_name << " -i document.pdf -o chunks.json --max-chunk-size 1000\n";
    std::cout << "  " << program_name << " --input report.pdf --page-limit 10 --verbose\n";
}

void print_version() {
    std::cout << "fast-pdf-parser chunk-pdf-cli version 2.0.0\n";
    std::cout << "Built with C++17, MuPDF, and tiktoken\n";
}

CLIOptions parse_arguments(int argc, char* argv[]) {
    CLIOptions options;
    
    const char* short_opts = "i:o:vqh";
    const struct option long_opts[] = {
        {"input", required_argument, nullptr, 'i'},
        {"output", required_argument, nullptr, 'o'},
        {"max-chunk-size", required_argument, nullptr, 1001},
        {"min-chunk-size", required_argument, nullptr, 1002},
        {"overlap", required_argument, nullptr, 1003},
        {"page-limit", required_argument, nullptr, 1004},
        {"threads", required_argument, nullptr, 1005},
        {"verbose", no_argument, nullptr, 'v'},
        {"quiet", no_argument, nullptr, 'q'},
        {"no-analyze", no_argument, nullptr, 1006},
        {"help", no_argument, nullptr, 'h'},
        {"version", no_argument, nullptr, 1007},
        {nullptr, 0, nullptr, 0}
    };
    
    int opt;
    int option_index = 0;
    
    while ((opt = getopt_long(argc, argv, short_opts, long_opts, &option_index)) != -1) {
        switch (opt) {
            case 'i':
                options.input_file = optarg;
                break;
            case 'o':
                options.output_file = optarg;
                break;
            case 1001:  // max-chunk-size
                options.max_chunk_size = std::stoi(optarg);
                if (options.max_chunk_size <= 0) {
                    throw std::invalid_argument("max-chunk-size must be positive");
                }
                break;
            case 1002:  // min-chunk-size
                options.min_chunk_size = std::stoi(optarg);
                if (options.min_chunk_size <= 0) {
                    throw std::invalid_argument("min-chunk-size must be positive");
                }
                break;
            case 1003:  // overlap
                options.overlap = std::stoi(optarg);
                if (options.overlap < 0) {
                    throw std::invalid_argument("overlap cannot be negative");
                }
                break;
            case 1004:  // page-limit
                options.page_limit = std::stoi(optarg);
                if (options.page_limit < 0) {
                    throw std::invalid_argument("page-limit cannot be negative");
                }
                break;
            case 1005:  // threads
                options.thread_count = std::stoi(optarg);
                if (options.thread_count < 0) {
                    throw std::invalid_argument("thread count cannot be negative");
                }
                break;
            case 'v':
                options.verbose = true;
                break;
            case 'q':
                options.quiet = true;
                break;
            case 1006:  // no-analyze
                options.analyze = false;
                break;
            case 'h':
                options.help = true;
                return options;
            case 1007:  // version
                options.version = true;
                return options;
            default:
                throw std::invalid_argument("Unknown option");
        }
    }
    
    // Validate options
    if (options.input_file.empty() && !options.help && !options.version) {
        throw std::invalid_argument("Input file is required");
    }
    
    if (options.min_chunk_size > options.max_chunk_size) {
        throw std::invalid_argument("min-chunk-size cannot be greater than max-chunk-size");
    }
    
    if (options.overlap >= options.max_chunk_size) {
        throw std::invalid_argument("overlap must be less than max-chunk-size");
    }
    
    if (options.verbose && options.quiet) {
        throw std::invalid_argument("Cannot use both --verbose and --quiet");
    }
    
    // Auto-generate output filename if not provided
    if (!options.input_file.empty() && options.output_file.empty()) {
        fs::path input_path(options.input_file);
        fs::path output_dir = input_path.parent_path();
        if (output_dir.empty()) {
            output_dir = ".";
        }
        std::string stem = input_path.stem().string();
        options.output_file = (output_dir / (stem + "_chunks.json")).string();
    }
    
    return options;
}

void analyze_chunk_distribution(const std::vector<ChunkResult>& chunks, bool quiet) {
    if (chunks.empty()) {
        if (!quiet) std::cout << "\nNo chunks created\n";
        return;
    }
    
    std::vector<int> token_counts;
    for (const auto& chunk : chunks) {
        token_counts.push_back(chunk.token_count);
    }
    
    std::sort(token_counts.begin(), token_counts.end());
    
    int min_tokens = token_counts.front();
    int max_tokens = token_counts.back();
    double avg_tokens = std::accumulate(token_counts.begin(), token_counts.end(), 0.0) / token_counts.size();
    
    if (!quiet) {
        std::cout << "\n=== Chunk Distribution Analysis ===\n";
        std::cout << "Total chunks: " << chunks.size() << "\n";
        std::cout << "Min tokens: " << min_tokens << "\n";
        std::cout << "Max tokens: " << max_tokens << "\n";
        std::cout << "Average tokens: " << static_cast<int>(avg_tokens) << "\n";
        
        // Calculate quintiles
        std::cout << "\nQuintiles:\n";
        for (int p = 20; p <= 80; p += 20) {
            size_t idx = (token_counts.size() - 1) * p / 100;
            std::cout << "  " << p << "th percentile: " << token_counts[idx] << " tokens\n";
        }
        
        // Token range distribution
        std::map<std::string, int> distribution;
        for (int tokens : token_counts) {
            if (tokens <= 100) distribution["1-100"]++;
            else if (tokens <= 200) distribution["101-200"]++;
            else if (tokens <= 300) distribution["201-300"]++;
            else if (tokens <= 400) distribution["301-400"]++;
            else if (tokens <= 500) distribution["401-500"]++;
            else if (tokens <= 600) distribution["501-600"]++;
            else if (tokens <= 800) distribution["601-800"]++;
            else if (tokens <= 1000) distribution["801-1000"]++;
            else distribution["1001+"]++;
        }
        
        std::cout << "\nToken Range Distribution:\n";
        for (const auto& [range, count] : distribution) {
            double percentage = (count * 100.0) / chunks.size();
            std::cout << "  " << std::setw(10) << range << " tokens: " 
                      << std::setw(5) << count << " chunks (" 
                      << std::fixed << std::setprecision(1) << percentage << "%)\n";
        }
    }
}

int main(int argc, char* argv[]) {
    try {
        // Parse command-line arguments
        CLIOptions options = parse_arguments(argc, argv);
        
        // Handle help and version
        if (options.help) {
            print_usage(argv[0]);
            return 0;
        }
        
        if (options.version) {
            print_version();
            return 0;
        }
        
        // Validate input file exists
        if (!fs::exists(options.input_file)) {
            throw std::runtime_error("Input file not found: " + options.input_file);
        }
        
        // Ensure output directory exists
        fs::path output_path(options.output_file);
        fs::path output_dir = output_path.parent_path();
        if (!output_dir.empty() && !fs::exists(output_dir)) {
            if (options.verbose) {
                std::cout << "Creating output directory: " << output_dir << "\n";
            }
            fs::create_directories(output_dir);
        }
        
        // Configure chunker
        ChunkOptions chunk_opts;
        chunk_opts.max_tokens = options.max_chunk_size;
        chunk_opts.min_tokens = options.min_chunk_size;
        chunk_opts.overlap_tokens = options.overlap;
        chunk_opts.thread_count = options.thread_count;
        
        HierarchicalChunker chunker(chunk_opts);
        
        // Print configuration if not quiet
        if (!options.quiet) {
            std::cout << "Processing: " << options.input_file << "\n";
            std::cout << "Output: " << options.output_file << "\n";
            std::cout << "Configuration:\n";
            std::cout << "  Max chunk size: " << options.max_chunk_size << " tokens\n";
            std::cout << "  Min chunk size: " << options.min_chunk_size << " tokens\n";
            std::cout << "  Overlap: " << options.overlap << " tokens\n";
            std::cout << "  Threads: " << (options.thread_count > 0 ? 
                std::to_string(options.thread_count) : "auto (" + 
                std::to_string(std::thread::hardware_concurrency()) + ")") << "\n";
            if (options.page_limit > 0) {
                std::cout << "  Page limit: " << options.page_limit << "\n";
            }
            std::cout << "\n";
        }
        
        auto start = std::chrono::high_resolution_clock::now();
        
        // Process the PDF
        if (options.verbose) {
            std::cout << "Starting PDF processing...\n";
        }
        
        auto result = chunker.chunk_file(options.input_file, options.page_limit);
        
        if (!result.error.empty()) {
            throw std::runtime_error("Chunking failed: " + result.error);
        }
        
        auto processing_end = std::chrono::high_resolution_clock::now();
        
        if (options.verbose) {
            std::cout << "Extracted " << result.total_pages << " pages\n";
            std::cout << "Created " << result.total_chunks << " chunks\n";
        }
        
        // Analyze distribution if requested
        if (options.analyze && !options.quiet) {
            analyze_chunk_distribution(result.chunks, options.quiet);
        }
        
        // Save to JSON
        if (options.verbose) {
            std::cout << "\nSaving chunks to JSON...\n";
        }
        
        bool save_success = chunker.process_pdf_to_json(
            options.input_file, 
            options.output_file, 
            options.page_limit
        );
        
        if (!save_success) {
            throw std::runtime_error("Failed to save JSON output");
        }
        
        auto end = std::chrono::high_resolution_clock::now();
        
        // Calculate and display metrics
        auto total_duration = std::chrono::duration_cast<std::chrono::milliseconds>(end - start);
        auto processing_duration = std::chrono::duration_cast<std::chrono::milliseconds>(processing_end - start);
        
        if (!options.quiet) {
            std::cout << "\n=== Processing Complete ===\n";
            std::cout << "Pages processed: " << result.total_pages << "\n";
            std::cout << "Chunks created: " << result.total_chunks << "\n";
            std::cout << "Processing time: " << processing_duration.count() << "ms\n";
            std::cout << "Total time: " << total_duration.count() << "ms\n";
            std::cout << "Performance: " << std::fixed << std::setprecision(1) 
                      << (result.total_pages * 1000.0) / processing_duration.count() 
                      << " pages/second\n";
            std::cout << "Output saved to: " << options.output_file << "\n";
        } else {
            // In quiet mode, just output essential info in parseable format
            std::cout << "SUCCESS|" << options.input_file << "|" 
                      << result.total_pages << "|" 
                      << result.total_chunks << "|"
                      << total_duration.count() << "\n";
        }
        
        return 0;
        
    } catch (const std::invalid_argument& e) {
        std::cerr << "Error: Invalid argument - " << e.what() << "\n";
        std::cerr << "Use --help for usage information\n";
        return 1;
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << "\n";
        return 1;
    } catch (...) {
        std::cerr << "Error: Unknown error occurred\n";
        return 1;
    }
}