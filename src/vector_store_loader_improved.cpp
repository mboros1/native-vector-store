#include "vector_store_loader_improved.h"
#include "vector_store_improved.h"
#include "atomic_queue.h"
#include <fstream>
#include <thread>
#include <chrono>
#include <iostream>
#include <cstring>

// Platform-specific includes for memory mapping
#ifdef _WIN32
    #include <windows.h>
#else
    #include <sys/mman.h>
    #include <fcntl.h>
    #include <unistd.h>
#endif

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

namespace internal {

// Producer thread: reads files sequentially and queues them.
// Sequential I/O is optimal for both HDDs and SSDs.
void producerThread(
    const std::vector<std::filesystem::path>& files,
    atomic_queue::AtomicQueue<QueuedFile*, 1024>& queue,
    LoaderStats& stats,
    const LoaderConfig& config
) {
    
    // Pre-allocate a reusable buffer for file reading
    std::string read_buffer;
    read_buffer.reserve(1024 * 1024);  // 1MB initial capacity
    
    for (const auto& filepath : files) {
        auto* file_data = new QueuedFile;
        file_data->path = filepath.string();
        
        try {
            // Get file size to decide loading method
            auto file_size = std::filesystem::file_size(filepath);
            file_data->size = file_size;
            stats.bytes_processed += file_size;
            
            // Decide whether to use memory mapping
            if (config.use_adaptive_loading && shouldUseMmap(filepath, config)) {
                // Try memory mapping for small files
                file_data->mmap_data = memoryMapFile(filepath, file_data->size);
                if (file_data->mmap_data) {
                    file_data->is_mmap = true;
                    stats.mmap_files++;
                }
            }
            
            // Fall back to standard I/O if mmap failed or wasn't chosen
            if (!file_data->is_mmap) {
                // Read file into buffer
                std::ifstream file(filepath, std::ios::binary);
                if (!file) {
                    delete file_data;
                    stats.files_failed++;
                    continue;
                }
                
                // Resize buffer if needed
                if (read_buffer.capacity() < file_size) {
                    read_buffer.reserve(file_size);
                }
                read_buffer.resize(file_size);
                
                if (!file.read(read_buffer.data(), file_size)) {
                    delete file_data;
                    stats.files_failed++;
                    continue;
                }
                
                // Copy to file data (queue owns the memory)
                file_data->content = read_buffer;
                stats.standard_files++;
            }
            
            // Queue the file for processing
            // This blocks if queue is full, providing backpressure
            queue.push(file_data);
            
        } catch (const std::exception& e) {
            delete file_data;
            stats.files_failed++;
            if (config.verbose) {
                std::cerr << "Failed to read " << filepath << ": " << e.what() << std::endl;
            }
        }
    }
}

// Consumer thread: processes queued files by parsing JSON.
// Multiple consumers work in parallel for CPU-intensive parsing.
void consumerThread(
    VectorStore& store,
    atomic_queue::AtomicQueue<QueuedFile*, 1024>& queue,
    std::atomic<bool>& producer_done,
    LoaderStats& stats
) {
    
    // Each consumer needs its own parser (they're not thread-safe)
    simdjson::ondemand::parser parser;
    
    while (true) {
        QueuedFile* file_data = nullptr;
        
        // Try to get a file from the queue
        if (queue.try_pop(file_data)) {
            // Process the file
            bool success = parseJsonFile(store, *file_data, stats);
            
            if (success) {
                stats.files_loaded++;
            } else {
                stats.files_failed++;
            }
            
            // Clean up memory-mapped files
            if (file_data->is_mmap && file_data->mmap_data) {
                memoryUnmapFile(file_data->mmap_data, file_data->size);
            }
            
            delete file_data;
        } else if (producer_done.load()) {
            // Producer is done and queue is empty, exit
            break;
        } else {
            // Queue is temporarily empty, yield CPU
            std::this_thread::yield();
        }
    }
}

// Placeholder implementations for memory mapping functions
// (These would have platform-specific implementations)
void* memoryMapFile(const std::filesystem::path& path, size_t& size_out) {
    // TODO: Implement platform-specific memory mapping
    return nullptr;
}

void memoryUnmapFile(void* data, size_t size) {
    // TODO: Implement platform-specific memory unmapping
}

bool shouldUseMmap(const std::filesystem::path& path, const LoaderConfig& config) {
    try {
        auto size = std::filesystem::file_size(path);
        return size <= config.max_file_size_for_mmap;
    } catch (...) {
        return false;
    }
}

} // namespace internal
} // namespace VectorStoreLoader