#include "vector_store_loader.h"
#include "atomic_queue.h"
#include <filesystem>
#include <fstream>
#include <thread>
#include <vector>
#include <atomic>
#include <cctype>

void VectorStoreLoader::loadDirectory(VectorStore* store, const std::string& path) {
    // Cannot load if already finalized
    if (store->is_finalized()) {
        return;
    }
    
    // Collect all JSON files
    std::vector<std::filesystem::path> json_files;
    for (const auto& entry : std::filesystem::directory_iterator(path)) {
        if (entry.path().extension() == ".json") {
            json_files.push_back(entry.path());
        }
    }
    
    if (json_files.empty()) {
        store->finalize();
        return;
    }
    
    // Producer-consumer queue for file data
    struct FileData {
        std::string filename;
        std::string content;
    };
    
    // Queue with bounded capacity (max ~100MB of buffered data)
    atomic_queue::AtomicQueue<FileData*, 1024> queue;
    
    // Atomic flags for coordination
    std::atomic<bool> producer_done{false};
    std::atomic<size_t> files_processed{0};
    
    // Producer thread - sequential file reading with optimizations
    std::thread producer([&]() {
        // Reusable buffer to avoid repeated allocations
        std::vector<char> buffer;
        // Start with reasonable capacity, will grow as needed
        buffer.reserve(1024 * 1024); // 1MB initial capacity
        
        for (const auto& filepath : json_files) {
            // Get file size without opening (one syscall)
            std::error_code ec;
            auto size = std::filesystem::file_size(filepath, ec);
            if (ec) {
                fprintf(stderr, "Error getting size of %s: %s\n", 
                        filepath.c_str(), ec.message().c_str());
                continue;
            }
            
            // Open file for reading
            std::ifstream file(filepath, std::ios::binary);
            if (!file) {
                fprintf(stderr, "Error opening %s\n", filepath.c_str());
                continue;
            }
            
            // Ensure buffer has enough capacity
            if (size > buffer.capacity()) {
                buffer.reserve(size);
            }
            // Resize buffer to exact size needed
            buffer.resize(size);
            
            // Read directly into buffer
            if (!file.read(buffer.data(), size)) {
                fprintf(stderr, "Error reading %s\n", filepath.c_str());
                continue;
            }
            
            // Create file data with string move-constructed from buffer
            auto* data = new FileData{
                filepath.string(), 
                std::string(buffer.begin(), buffer.end())
            };
            queue.push(data);
        }
        producer_done = true;
    });
    
    // Consumer threads - parallel JSON parsing
    size_t num_workers = std::thread::hardware_concurrency();
    std::vector<std::thread> consumers;
    
    for (size_t w = 0; w < num_workers; ++w) {
        consumers.emplace_back([&]() {
            // Each thread needs its own parser with large capacity for big files
            simdjson::ondemand::parser doc_parser(16 * 1024 * 1024); // 16MB capacity
            FileData* data = nullptr;
            
            while (true) {
                // Try to get work from queue
                if (queue.try_pop(data)) {
                    // Process the file
                    simdjson::padded_string json(data->content);
                    
                    // Check if it's an array or object
                    const char* json_start = json.data();
                    while (json_start && *json_start && std::isspace(*json_start)) {
                        json_start++;
                    }
                    bool is_array = (json_start && *json_start == '[');
                    
                    simdjson::ondemand::document doc;
                    auto error = doc_parser.iterate(json).get(doc);
                    if (error) {
                        fprintf(stderr, "Error parsing %s: %s\n", data->filename.c_str(), simdjson::error_message(error));
                        delete data;
                        continue;
                    }
                    
                    if (is_array) {
                        // Process as array
                        simdjson::ondemand::array arr;
                        error = doc.get_array().get(arr);
                        if (error) {
                            fprintf(stderr, "Error getting array from %s: %s\n", data->filename.c_str(), simdjson::error_message(error));
                            delete data;
                            continue;
                        }
                        
                        for (auto doc_element : arr) {
                            simdjson::ondemand::object obj;
                            error = doc_element.get_object().get(obj);
                            if (!error) {
                                auto add_error = store->add_document(obj);
                                if (add_error != VectorStoreError::SUCCESS) {
                                    fprintf(stderr, "Error adding document from %s: %s\n", 
                                           data->filename.c_str(), vector_store_error_message(add_error));
                                    if (add_error == VectorStoreError::JSON_NO_SUCH_FIELD || add_error == VectorStoreError::MISSING_FIELD) {
                                        fprintf(stderr, "  Expected JSON format: {\"id\": string, \"text\": string, \"metadata\": {\"embedding\": [numbers...]}}\n");
                                        fprintf(stderr, "  Required fields: id, text (or content), metadata.embedding\n");
                                        fprintf(stderr, "  Note: 'embedding' must be inside 'metadata' object\n");
                                        fprintf(stderr, "  Note: 'text' and 'content' are interchangeable (Spring AI compatibility)\n");
                                    } else if (add_error == VectorStoreError::DIMENSION_MISMATCH) {
                                        fprintf(stderr, "  Check that all embeddings have the same dimensions\n");
                                    } else if (add_error == VectorStoreError::STORE_ALREADY_FINALIZED) {
                                        fprintf(stderr, "  Store has been finalized and cannot accept new documents\n");
                                    }
                                }
                            }
                        }
                    } else {
                        // Process as single document
                        simdjson::ondemand::object obj;
                        error = doc.get_object().get(obj);
                        if (!error) {
                            auto add_error = store->add_document(obj);
                            if (add_error != VectorStoreError::SUCCESS) {
                                fprintf(stderr, "Error adding document from %s: %s\n", 
                                       data->filename.c_str(), vector_store_error_message(add_error));
                                if (add_error == VectorStoreError::JSON_NO_SUCH_FIELD || add_error == VectorStoreError::MISSING_FIELD) {
                                    fprintf(stderr, "  Expected JSON format: {\"id\": string, \"text\": string, \"metadata\": {\"embedding\": [numbers...]}}\n");
                                    fprintf(stderr, "  Required fields: id, text (or content), metadata.embedding\n");
                                    fprintf(stderr, "  Note: 'embedding' must be inside 'metadata' object\n");
                                    fprintf(stderr, "  Note: 'text' and 'content' are interchangeable (Spring AI compatibility)\n");
                                } else if (add_error == VectorStoreError::DIMENSION_MISMATCH) {
                                    fprintf(stderr, "  Check that all embeddings have the same dimensions\n");
                                } else if (add_error == VectorStoreError::STORE_ALREADY_FINALIZED) {
                                    fprintf(stderr, "  Store has been finalized and cannot accept new documents\n");
                                }
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