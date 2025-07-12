#include "vector_store_loader_improved.h"
#include "vector_store_loader_internal.h"
#include "vector_store_improved.h"
#include <thread>
#include <chrono>
#include <iostream>
#include <algorithm>

namespace VectorStoreLoader {

using namespace internal;

// Loads all JSON files from a directory into the vector store.
// This orchestrates the entire producer-consumer loading process.
LoaderStats loadDirectory(
    VectorStore& store, 
    const std::string& directory_path,
    const LoaderConfig& config
) {
    auto start_time = std::chrono::high_resolution_clock::now();
    LoaderStats stats;
    
    // Step 1: Find all JSON files in the directory
    auto files = findJsonFiles(directory_path);
    stats.total_files = files.size();
    
    if (files.empty()) {
        if (config.verbose) {
            std::cerr << "No JSON files found in " << directory_path << std::endl;
        }
        store.finalize();  // Finalize even if empty
        return stats;
    }
    
    // Step 2: Set up the producer-consumer queue
    // The queue holds pointers to file data, bounded to limit memory usage
    using Queue = atomic_queue::AtomicQueue<QueuedFile*, 1024>;
    Queue queue;
    std::atomic<bool> producer_done{false};
    
    // Step 3: Determine number of consumer threads
    size_t num_consumers = config.consumer_threads;
    if (num_consumers == 0) {
        num_consumers = calculateOptimalConsumerThreads(files, config);
    }
    
    if (config.verbose) {
        std::cerr << "Loading " << files.size() << " files with "
                  << num_consumers << " consumer threads" << std::endl;
    }
    
    // Step 4: Start producer thread (reads files)
    std::thread producer(producerThread, 
        std::ref(files), 
        std::ref(queue), 
        std::ref(stats), 
        std::ref(config)
    );
    
    // Step 5: Start consumer threads (parse JSON)
    std::vector<std::thread> consumers;
    for (size_t i = 0; i < num_consumers; ++i) {
        consumers.emplace_back(consumerThread,
            std::ref(store),
            std::ref(queue),
            std::ref(producer_done),
            std::ref(stats)
        );
    }
    
    // Step 6: Wait for all threads to complete
    producer.join();
    producer_done.store(true);  // Signal consumers that producer is done
    
    for (auto& consumer : consumers) {
        consumer.join();
    }
    
    // Step 7: Finalize the store (normalize embeddings)
    store.finalize();
    
    // Calculate elapsed time
    auto end_time = std::chrono::high_resolution_clock::now();
    stats.elapsed_seconds = std::chrono::duration<double>(end_time - start_time).count();
    
    if (config.verbose) {
        std::cerr << "Loaded " << stats.documents_parsed << " documents in "
                  << stats.elapsed_seconds << " seconds ("
                  << stats.documents_per_second() << " docs/sec)" << std::endl;
    }
    
    return stats;
}

// Loads a single JSON file synchronously.
// This is useful for testing or when you want to load files one at a time.
bool loadFile(VectorStore& store, const std::string& file_path) {
    // Read file contents
    std::ifstream file(file_path, std::ios::binary | std::ios::ate);
    if (!file) {
        return false;
    }
    
    size_t size = file.tellg();
    file.seekg(0);
    
    std::string content(size, '\0');
    if (!file.read(content.data(), size)) {
        return false;
    }
    
    // Parse JSON
    simdjson::ondemand::parser parser;
    simdjson::padded_string padded(content);
    
    try {
        simdjson::ondemand::document doc = parser.iterate(padded);
        
        // Check if it's an array or single document
        if (doc.is_array()) {
            // Process array of documents
            for (auto element : doc.get_array()) {
                simdjson::ondemand::object obj = element.get_object();
                store.add_document(obj);
            }
        } else {
            // Process single document
            store.add_document(doc);
        }
        
        return true;
    } catch (const simdjson::simdjson_error& e) {
        return false;
    }
}

// Finds all JSON files in a directory (non-recursive).
// Returns sorted paths for deterministic loading order.
std::vector<std::filesystem::path> findJsonFiles(const std::string& directory_path) {
    std::vector<std::filesystem::path> json_files;
    
    try {
        for (const auto& entry : std::filesystem::directory_iterator(directory_path)) {
            if (entry.is_regular_file() && entry.path().extension() == ".json") {
                json_files.push_back(entry.path());
            }
        }
    } catch (const std::filesystem::filesystem_error& e) {
        // Directory doesn't exist or isn't accessible
        return json_files;
    }
    
    // Sort for deterministic order
    std::sort(json_files.begin(), json_files.end());
    
    return json_files;
}

// Calculates optimal number of consumer threads based on system resources.
// The goal is to maximize CPU utilization without overwhelming memory.
size_t calculateOptimalConsumerThreads(
    const std::vector<std::filesystem::path>& files,
    const LoaderConfig& config
) {
    // Start with hardware concurrency
    size_t hw_threads = std::thread::hardware_concurrency();
    if (hw_threads == 0) {
        hw_threads = 4;  // Fallback if detection fails
    }
    
    // For small file counts, don't create more threads than files
    size_t max_useful_threads = std::min(hw_threads, files.size());
    
    // Leave one thread for the producer
    return std::max(size_t(1), max_useful_threads - 1);
}

} // namespace VectorStoreLoader