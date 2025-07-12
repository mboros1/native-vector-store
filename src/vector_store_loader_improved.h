#pragma once
#include <string>
#include <vector>
#include <filesystem>

// Forward declarations
class VectorStore;

// Configuration for the file loading process
struct LoaderConfig {
    size_t queue_capacity = 1024;        // Max items in producer-consumer queue
    size_t consumer_threads = 0;         // 0 = hardware_concurrency()
    size_t max_file_size_for_mmap = 5 * 1024 * 1024;  // 5MB threshold
    bool use_adaptive_loading = true;    // Enable mmap for small files
    bool verbose = false;                // Print loading statistics
};

// Statistics gathered during loading
struct LoaderStats {
    size_t total_files = 0;
    size_t files_loaded = 0;
    size_t files_failed = 0;
    size_t bytes_processed = 0;
    size_t documents_parsed = 0;
    size_t mmap_files = 0;               // Files loaded via memory mapping
    size_t standard_files = 0;           // Files loaded via standard I/O
    double elapsed_seconds = 0.0;
    
    // Computed metrics
    double documents_per_second() const {
        return elapsed_seconds > 0 ? documents_parsed / elapsed_seconds : 0;
    }
    
    double megabytes_per_second() const {
        return elapsed_seconds > 0 ? (bytes_processed / 1024.0 / 1024.0) / elapsed_seconds : 0;
    }
};

// VectorStoreLoader handles parallel loading of JSON documents into a VectorStore.
// It uses a producer-consumer pattern to optimize I/O and CPU usage:
// - Producer thread: Reads files sequentially (optimal for disk I/O)
// - Consumer threads: Parse JSON in parallel (uses all CPU cores)
namespace VectorStoreLoader {
    
    // === Main Loading Functions ===
    
    // Loads all JSON files from a directory into the vector store.
    // Automatically finalizes the store after loading.
    // Returns statistics about the loading process.
    LoaderStats loadDirectory(
        VectorStore* store, 
        const std::string& directory_path,
        const LoaderConfig& config = LoaderConfig{}
    );
    
    // Loads a single JSON file into the vector store.
    // File can contain either a single document or an array of documents.
    // Does NOT finalize the store (caller must do it).
    bool loadFile(
        VectorStore* store,
        const std::string& file_path
    );
    
    // === Helper Functions ===
    
    // Collects all JSON files in a directory (non-recursive).
    // Returns paths sorted by name for deterministic loading order.
    std::vector<std::filesystem::path> findJsonFiles(
        const std::string& directory_path
    );
    
    // Estimates the optimal number of consumer threads based on:
    // - Hardware concurrency
    // - Average file size
    // - Available memory
    size_t calculateOptimalConsumerThreads(
        const std::vector<std::filesystem::path>& files,
        const LoaderConfig& config
    );
    
    // === Internal Implementation Functions ===
    // (These would typically be in a .cpp file)
    
    namespace internal {
        
        // File data passed through the producer-consumer queue
        struct QueuedFile {
            std::string path;        // File path for error reporting
            std::string content;     // File contents (for standard loading)
            void* mmap_data = nullptr;  // Memory-mapped data (for mmap loading)
            size_t size = 0;         // File size in bytes
            bool is_mmap = false;    // Loading method flag
        };
        
        // Producer thread function: reads files and queues them.
        // Stops when all files are read or queue is terminated.
        void producerThread(
            const std::vector<std::filesystem::path>& files,
            void* queue,  // atomic_queue::AtomicQueue<QueuedFile*>*
            LoaderStats* stats,
            const LoaderConfig& config
        );
        
        // Consumer thread function: dequeues files and parses JSON.
        // Stops when producer is done and queue is empty.
        void consumerThread(
            VectorStore* store,
            void* queue,  // atomic_queue::AtomicQueue<QueuedFile*>*
            std::atomic<bool>* producer_done,
            LoaderStats* stats
        );
        
        // Parses a JSON file that may contain one or many documents.
        // Handles both single document and array formats.
        // Updates stats with number of documents parsed.
        bool parseJsonFile(
            VectorStore* store,
            const QueuedFile& file,
            LoaderStats* stats
        );
        
        // Memory-maps a file for zero-copy access.
        // Returns nullptr if mapping fails (falls back to standard I/O).
        void* memoryMapFile(
            const std::filesystem::path& path,
            size_t& size_out
        );
        
        // Unmaps a memory-mapped file.
        void memoryUnmapFile(void* data, size_t size);
        
        // Determines if a file should use memory mapping based on size.
        // Small files benefit from mmap due to reduced allocation overhead.
        bool shouldUseMmap(
            const std::filesystem::path& path,
            const LoaderConfig& config
        );
    }
}