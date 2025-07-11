#include "vector_store_loader.h"
#include "mmap_file.h"
#include "atomic_queue.h"
#include <filesystem>
#include <fstream>
#include <thread>
#include <vector>
#include <atomic>
#include <cctype>
#include <memory>

// Adaptive loader that chooses the best method per file
void VectorStoreLoader::loadDirectoryAdaptive(VectorStore* store, const std::string& path) {
    // Cannot load if already finalized
    if (store->is_finalized()) {
        return;
    }
    
    // File size threshold (5MB - files larger than this use standard loading)
    constexpr size_t SIZE_THRESHOLD = 5 * 1024 * 1024;
    
    // Collect and categorize files
    struct FileInfo {
        std::filesystem::path path;
        size_t size;
        bool use_mmap;
    };
    
    std::vector<FileInfo> file_infos;
    
    for (const auto& entry : std::filesystem::directory_iterator(path)) {
        if (entry.path().extension() == ".json") {
            std::error_code ec;
            auto size = std::filesystem::file_size(entry.path(), ec);
            if (!ec) {
                file_infos.push_back({
                    entry.path(),
                    size,
                    size < SIZE_THRESHOLD  // Use mmap for smaller files
                });
            }
        }
    }
    
    if (file_infos.empty()) {
        store->finalize();
        return;
    }
    
    // Producer-consumer queue for mixed file data
    struct MixedFileData {
        std::string filename;
        std::unique_ptr<MMapFile> mmap;  // For mmap files
        std::string content;             // For standard loaded files
        bool is_mmap;
    };
    
    // Queue with bounded capacity
    atomic_queue::AtomicQueue<MixedFileData*, 1024> queue;
    
    // Atomic flags for coordination
    std::atomic<bool> producer_done{false};
    std::atomic<size_t> files_processed{0};
    std::atomic<size_t> mmap_count{0};
    std::atomic<size_t> standard_count{0};
    
    // Producer thread - loads files using appropriate method
    std::thread producer([&]() {
        // Reusable buffer for standard loading
        std::vector<char> buffer;
        buffer.reserve(1024 * 1024); // Reserve 1MB initial capacity
        
        for (const auto& file_info : file_infos) {
            auto* data = new MixedFileData{
                file_info.path.string(),
                nullptr,
                "",
                file_info.use_mmap
            };
            
            if (file_info.use_mmap) {
                // Memory map smaller files
                auto mmap = std::make_unique<MMapFile>();
                
                if (!mmap->open(file_info.path.string())) {
                    fprintf(stderr, "Error mapping file %s\n", file_info.path.c_str());
                    delete data;
                    continue;
                }
                
                data->mmap = std::move(mmap);
                mmap_count++;
                
            } else {
                // Standard load for larger files
                // Ensure buffer has enough capacity
                if (file_info.size > buffer.capacity()) {
                    buffer.reserve(file_info.size);
                }
                buffer.resize(file_info.size);
                
                std::ifstream file(file_info.path, std::ios::binary);
                if (!file) {
                    fprintf(stderr, "Error opening %s\n", file_info.path.c_str());
                    delete data;
                    continue;
                }
                
                if (!file.read(buffer.data(), file_info.size)) {
                    fprintf(stderr, "Error reading %s\n", file_info.path.c_str());
                    delete data;
                    continue;
                }
                
                data->content = std::string(buffer.begin(), buffer.end());
                standard_count++;
            }
            
            queue.push(data);
        }
        
        producer_done = true;
        
        // Log loading stats
        fprintf(stderr, "Adaptive loader: %zu files via mmap, %zu files via standard\n",
                mmap_count.load(), standard_count.load());
    });
    
    // Consumer threads - parallel JSON parsing
    size_t num_workers = std::thread::hardware_concurrency();
    std::vector<std::thread> consumers;
    
    for (size_t w = 0; w < num_workers; ++w) {
        consumers.emplace_back([&]() {
            // Each thread needs its own parser
            simdjson::ondemand::parser doc_parser;
            MixedFileData* data = nullptr;
            
            while (true) {
                // Try to get work from queue
                if (queue.try_pop(data)) {
                    // Get JSON content based on loading method
                    simdjson::padded_string json = data->is_mmap
                        ? simdjson::padded_string(data->mmap->data(), data->mmap->size())
                        : simdjson::padded_string(data->content);
                    
                    // Check if it's an array or object
                    const char* json_start = json.data();
                    while (json_start && *json_start && std::isspace(*json_start)) {
                        json_start++;
                    }
                    bool is_array = (json_start && *json_start == '[');
                    
                    simdjson::ondemand::document doc;
                    auto error = doc_parser.iterate(json).get(doc);
                    if (error) {
                        fprintf(stderr, "Error parsing %s: %s\n", 
                                data->filename.c_str(), simdjson::error_message(error));
                        delete data;
                        continue;
                    }
                    
                    if (is_array) {
                        // Process as array
                        simdjson::ondemand::array arr;
                        error = doc.get_array().get(arr);
                        if (error) {
                            fprintf(stderr, "Error getting array from %s: %s\n", 
                                    data->filename.c_str(), simdjson::error_message(error));
                            delete data;
                            continue;
                        }
                        
                        for (auto doc_element : arr) {
                            simdjson::ondemand::object obj;
                            error = doc_element.get_object().get(obj);
                            if (!error) {
                                auto add_error = store->add_document(obj);
                                if (add_error) {
                                    fprintf(stderr, "Error adding document from %s: %s\n", 
                                           data->filename.c_str(), simdjson::error_message(add_error));
                                }
                            }
                        }
                    } else {
                        // Process as single document
                        simdjson::ondemand::object obj;
                        error = doc.get_object().get(obj);
                        if (!error) {
                            auto add_error = store->add_document(obj);
                            if (add_error) {
                                fprintf(stderr, "Error adding document from %s: %s\n", 
                                       data->filename.c_str(), simdjson::error_message(add_error));
                            }
                        }
                    }
                    
                    delete data;
                    files_processed++;
                    
                } else if (producer_done.load()) {
                    // No more work and producer is done
                    break;
                } else {
                    // Queue is empty but producer might add more
                    std::this_thread::yield();
                }
            }
        });
    }
    
    // Wait for all threads to complete
    producer.join();
    for (auto& consumer : consumers) {
        consumer.join();
    }
    
    // Finalize after batch load - normalize and switch to serving phase
    store->finalize();
}