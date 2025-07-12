#pragma once
#include <string>
#include <filesystem>
#include <atomic>
#include "atomic_queue.h"

// Forward declarations
class VectorStore;
struct LoaderConfig;
struct LoaderStats;

namespace VectorStoreLoader {
namespace internal {

// File data passed through the producer-consumer queue.
// This structure holds either the file content (standard loading)
// or a memory-mapped pointer (mmap loading) but never both.
struct QueuedFile {
    std::string path;           // File path for error reporting
    std::string content;        // File contents (for standard loading)
    void* mmap_data = nullptr;  // Memory-mapped data (for mmap loading)  
    size_t size = 0;            // File size in bytes
    bool is_mmap = false;       // Loading method flag
    
    // Destructor ensures proper cleanup
    ~QueuedFile();
};

// Producer thread function: reads files sequentially.
// This maintains optimal disk I/O patterns by avoiding random access.
// Files are queued for parallel processing by consumers.
void producerThread(
    const std::vector<std::filesystem::path>& files,
    atomic_queue::AtomicQueue<QueuedFile*, 1024>& queue,
    LoaderStats& stats,
    const LoaderConfig& config
);

// Consumer thread function: processes queued files.
// Multiple consumers parse JSON in parallel, maximizing CPU usage.
// Each consumer has its own parser instance (not thread-safe).
void consumerThread(
    VectorStore& store,
    atomic_queue::AtomicQueue<QueuedFile*, 1024>& queue,
    std::atomic<bool>& producer_done,
    LoaderStats& stats
);

// Parses a JSON file containing one or more documents.
// Automatically detects whether the file contains:
// - A single document object
// - An array of document objects
// Returns true if at least one document was successfully added.
bool parseJsonFile(
    VectorStore& store,
    const QueuedFile& file,
    LoaderStats& stats
);

// Memory-maps a file for zero-copy access.
// This is beneficial for small files as it avoids allocation overhead.
// Returns nullptr if mapping fails (caller should fall back to standard I/O).
// The caller is responsible for calling memoryUnmapFile() when done.
void* memoryMapFile(
    const std::filesystem::path& path,
    size_t& size_out
);

// Unmaps a memory-mapped file.
// Must be called for every successful memoryMapFile() call.
void memoryUnmapFile(void* data, size_t size);

// Determines if a file should use memory mapping.
// Small files benefit from mmap due to:
// - Reduced allocation overhead
// - OS page cache utilization
// - Potential for lazy loading
bool shouldUseMmap(
    const std::filesystem::path& path,
    const LoaderConfig& config
);

// Reads a file using standard I/O into the provided buffer.
// The buffer is resized as needed to hold the file contents.
// Returns true on success, false on I/O error.
bool readFileStandard(
    const std::filesystem::path& path,
    std::string& buffer_out
);

// Processes a single JSON document from parsed data.
// Extracts id, text, and embedding then adds to the store.
// Updates document count in stats.
bool processDocument(
    VectorStore& store,
    simdjson::ondemand::object& doc,
    LoaderStats& stats
);

// Processes an array of JSON documents.
// Iterates through the array and processes each document.
// Returns number of successfully processed documents.
size_t processDocumentArray(
    VectorStore& store,
    simdjson::ondemand::array& arr,
    LoaderStats& stats
);

} // namespace internal
} // namespace VectorStoreLoader