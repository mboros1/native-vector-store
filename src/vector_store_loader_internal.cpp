#include "vector_store_loader_internal.h"
#include "vector_store_loader_improved.h"
#include "vector_store_improved.h"
#include <fstream>
#include <thread>
#include <iostream>
#include <cstring>

// Platform-specific includes for memory mapping
#ifdef _WIN32
    #include <windows.h>
#else
    #include <sys/mman.h>
    #include <fcntl.h>
    #include <unistd.h>
    #include <sys/stat.h>
#endif

namespace VectorStoreLoader {
namespace internal {

// Destructor ensures proper cleanup of memory-mapped files
QueuedFile::~QueuedFile() {
    if (is_mmap && mmap_data) {
        memoryUnmapFile(mmap_data, size);
    }
}

// Producer thread reads files sequentially for optimal I/O performance.
// It uses a bounded queue to limit memory usage and provide backpressure.
void producerThread(
    const std::vector<std::filesystem::path>& files,
    atomic_queue::AtomicQueue<QueuedFile*, 1024>& queue,
    LoaderStats& stats,
    const LoaderConfig& config
) {
    // Pre-allocate a reusable buffer to avoid repeated allocations
    std::string read_buffer;
    read_buffer.reserve(1024 * 1024);  // 1MB initial capacity
    
    for (const auto& filepath : files) {
        auto* file_data = new QueuedFile;
        file_data->path = filepath.string();
        
        try {
            // Get file size first to make loading decisions
            auto file_size = std::filesystem::file_size(filepath);
            file_data->size = file_size;
            stats.bytes_processed += file_size;
            
            // Choose loading method based on file size and configuration
            bool use_mmap = config.use_adaptive_loading && 
                           shouldUseMmap(filepath, config);
            
            if (use_mmap) {
                // Attempt memory mapping for small files
                file_data->mmap_data = memoryMapFile(filepath, file_data->size);
                if (file_data->mmap_data) {
                    file_data->is_mmap = true;
                    stats.mmap_files++;
                } else {
                    // mmap failed, fall back to standard I/O
                    use_mmap = false;
                }
            }
            
            // Use standard I/O if mmap wasn't used or failed
            if (!use_mmap) {
                if (readFileStandard(filepath, read_buffer)) {
                    // Copy buffer contents (queue owns the memory)
                    file_data->content = read_buffer;
                    stats.standard_files++;
                } else {
                    delete file_data;
                    stats.files_failed++;
                    continue;
                }
            }
            
            // Queue the file for processing
            // This blocks if queue is full, implementing backpressure
            queue.push(file_data);
            
        } catch (const std::exception& e) {
            delete file_data;
            stats.files_failed++;
            if (config.verbose) {
                std::cerr << "Failed to read " << filepath << ": " 
                         << e.what() << std::endl;
            }
        }
    }
}

// Consumer thread processes files from the queue in parallel.
// Each consumer maintains its own JSON parser for thread safety.
void consumerThread(
    VectorStore& store,
    atomic_queue::AtomicQueue<QueuedFile*, 1024>& queue,
    std::atomic<bool>& producer_done,
    LoaderStats& stats
) {
    // Each consumer needs its own parser (not thread-safe)
    simdjson::ondemand::parser parser;
    
    while (true) {
        QueuedFile* file_data = nullptr;
        
        // Try to dequeue a file
        if (queue.try_pop(file_data)) {
            // Process the file
            bool success = parseJsonFile(store, *file_data, stats);
            
            if (success) {
                stats.files_loaded++;
            } else {
                stats.files_failed++;
            }
            
            // QueuedFile destructor handles mmap cleanup
            delete file_data;
            
        } else if (producer_done.load()) {
            // Producer finished and queue is empty
            break;
            
        } else {
            // Queue temporarily empty, yield to avoid spinning
            std::this_thread::yield();
        }
    }
}

// Parses JSON file and adds documents to the store.
// Handles both single documents and arrays of documents.
bool parseJsonFile(
    VectorStore& store,
    const QueuedFile& file,
    LoaderStats& stats
) {
    simdjson::ondemand::parser parser;
    
    try {
        // Create padded string from file content
        simdjson::padded_string padded;
        if (file.is_mmap) {
            // For mmap, we need to copy to padded string
            padded = simdjson::padded_string(
                std::string_view(static_cast<const char*>(file.mmap_data), file.size)
            );
        } else {
            // For standard loading, content is already in memory
            padded = simdjson::padded_string(file.content);
        }
        
        // Parse the JSON
        simdjson::ondemand::document doc = parser.iterate(padded);
        
        // Detect if root is an array or object
        bool is_array = false;
        {
            // Check first non-whitespace character
            const char* json_start = padded.data();
            while (*json_start && std::isspace(*json_start)) {
                json_start++;
            }
            is_array = (*json_start == '[');
        }
        
        size_t docs_before = stats.documents_parsed;
        
        if (is_array) {
            // Process array of documents
            simdjson::ondemand::array arr = doc.get_array();
            processDocumentArray(store, arr, stats);
        } else {
            // Process single document
            simdjson::ondemand::object obj = doc.get_object();
            processDocument(store, obj, stats);
        }
        
        // Return true if we parsed at least one document
        return stats.documents_parsed > docs_before;
        
    } catch (const simdjson::simdjson_error& e) {
        // Log error if verbose mode
        if (stats.files_failed == 0 || stats.files_failed % 100 == 0) {
            std::cerr << "JSON parse error in " << file.path 
                     << ": " << e.what() << std::endl;
        }
        return false;
    }
}

// Process a single JSON document object
bool processDocument(
    VectorStore& store,
    simdjson::ondemand::object& doc,
    LoaderStats& stats
) {
    auto error = store.add_document(doc);
    if (error == simdjson::SUCCESS) {
        stats.documents_parsed++;
        return true;
    }
    return false;
}

// Process an array of JSON documents
size_t processDocumentArray(
    VectorStore& store,
    simdjson::ondemand::array& arr,
    LoaderStats& stats
) {
    size_t count = 0;
    
    for (auto element : arr) {
        try {
            simdjson::ondemand::object obj = element.get_object();
            if (processDocument(store, obj, stats)) {
                count++;
            }
        } catch (const simdjson::simdjson_error&) {
            // Skip malformed array elements
            continue;
        }
    }
    
    return count;
}

// Read file using standard I/O
bool readFileStandard(
    const std::filesystem::path& path,
    std::string& buffer_out
) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        return false;
    }
    
    // Get size from current position (at end)
    size_t size = file.tellg();
    file.seekg(0);
    
    // Resize buffer if needed
    if (buffer_out.capacity() < size) {
        buffer_out.reserve(size);
    }
    buffer_out.resize(size);
    
    // Read entire file
    return file.read(buffer_out.data(), size).good();
}

// Determine if file should use memory mapping
bool shouldUseMmap(
    const std::filesystem::path& path,
    const LoaderConfig& config
) {
    try {
        auto size = std::filesystem::file_size(path);
        
        // Use mmap for files smaller than threshold
        // Small files benefit from reduced allocation overhead
        return size <= config.max_file_size_for_mmap;
        
    } catch (const std::filesystem::filesystem_error&) {
        return false;
    }
}

// Platform-specific memory mapping implementation
void* memoryMapFile(const std::filesystem::path& path, size_t& size_out) {
#ifdef _WIN32
    // Windows implementation
    HANDLE file = CreateFileW(
        path.c_str(),
        GENERIC_READ,
        FILE_SHARE_READ,
        nullptr,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        nullptr
    );
    
    if (file == INVALID_HANDLE_VALUE) {
        return nullptr;
    }
    
    LARGE_INTEGER file_size;
    if (!GetFileSizeEx(file, &file_size)) {
        CloseHandle(file);
        return nullptr;
    }
    
    size_out = static_cast<size_t>(file_size.QuadPart);
    
    HANDLE mapping = CreateFileMappingW(
        file,
        nullptr,
        PAGE_READONLY,
        file_size.HighPart,
        file_size.LowPart,
        nullptr
    );
    
    CloseHandle(file);
    
    if (!mapping) {
        return nullptr;
    }
    
    void* data = MapViewOfFile(
        mapping,
        FILE_MAP_READ,
        0, 0,
        size_out
    );
    
    CloseHandle(mapping);
    return data;
    
#else
    // POSIX implementation
    int fd = open(path.c_str(), O_RDONLY);
    if (fd == -1) {
        return nullptr;
    }
    
    struct stat st;
    if (fstat(fd, &st) != 0) {
        close(fd);
        return nullptr;
    }
    
    size_out = st.st_size;
    
    void* data = mmap(
        nullptr,
        size_out,
        PROT_READ,
        MAP_PRIVATE,
        fd,
        0
    );
    
    close(fd);
    
    if (data == MAP_FAILED) {
        return nullptr;
    }
    
    // Advise kernel about access pattern
    madvise(data, size_out, MADV_SEQUENTIAL);
    
    return data;
#endif
}

// Unmap memory-mapped file
void memoryUnmapFile(void* data, size_t size) {
    if (!data) return;
    
#ifdef _WIN32
    UnmapViewOfFile(data);
#else
    munmap(data, size);
#endif
}

} // namespace internal
} // namespace VectorStoreLoader