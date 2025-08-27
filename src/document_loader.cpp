#include "document_loader.h"
#include "mmap_file.h"
#include "simple_tokenizer.h"
#include "atomic_queue.h"
#include <filesystem>
#include <fstream>
#include <thread>
#include <vector>
#include <atomic>
#include <memory>
#include <iostream>
#include <simdjson.h>

namespace fs = std::filesystem;

// Forward declaration of helper function
static bool parseDocumentObject(simdjson::ondemand::object& obj, 
                                DocumentLoader::Document& doc,
                                DocumentLoader::LoadResult& result);

DocumentLoader::LoadResult DocumentLoader::loadDirectory(const std::string& path, bool verbose) {
    LoadResult result;
    
    // File size threshold (5MB - files larger than this use streaming)
    constexpr size_t SIZE_THRESHOLD = 5 * 1024 * 1024;
    
    // Collect and categorize files
    struct FileInfo {
        fs::path path;
        size_t size;
        bool use_mmap;
    };
    
    std::vector<FileInfo> file_infos;
    
    for (const auto& entry : fs::directory_iterator(path)) {
        if (entry.path().extension() == ".json") {
            std::error_code ec;
            auto size = fs::file_size(entry.path(), ec);
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
        return result;
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
    std::atomic<size_t> docs_loaded{0};
    
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
                    if (verbose) {
                        std::cerr << "Error mapping file " << file_info.path << "\n";
                    }
                    delete data;
                    continue;
                }
                
                data->mmap = std::move(mmap);
                
            } else {
                // Standard load for larger files
                // Ensure buffer has enough capacity
                if (file_info.size > buffer.capacity()) {
                    buffer.reserve(file_info.size);
                }
                buffer.resize(file_info.size);
                
                std::ifstream file(file_info.path, std::ios::binary);
                if (!file.read(buffer.data(), file_info.size)) {
                    if (verbose) {
                        std::cerr << "Error reading file " << file_info.path << "\n";
                    }
                    delete data;
                    continue;
                }
                
                data->content = std::string(buffer.data(), file_info.size);
            }
            
            // Enqueue for processing
            queue.push(data);
        }
        
        producer_done = true;
    });
    
    // Consumer threads - parse JSON and extract documents
    const size_t num_consumers = std::thread::hardware_concurrency();
    std::vector<std::thread> consumers;
    std::vector<std::vector<Document>> thread_documents(num_consumers);
    
    for (size_t i = 0; i < num_consumers; ++i) {
        consumers.emplace_back([&, thread_idx = i]() {
            simdjson::ondemand::parser parser;
            auto& local_docs = thread_documents[thread_idx];
            local_docs.reserve(100);  // Pre-allocate some space
            
            MixedFileData* data;
            while (true) {
                // Try to get data from queue
                if (!queue.try_pop(data)) {
                    if (producer_done && queue.was_empty()) {
                        break;  // No more work
                    }
                    std::this_thread::yield();
                    continue;
                }
                
                // Parse JSON based on type
                std::string_view json_content;
                if (data->is_mmap) {
                    json_content = std::string_view(
                        static_cast<const char*>(data->mmap->data()),
                        data->mmap->size()
                    );
                } else {
                    json_content = data->content;
                }
                
                // Parse the JSON
                simdjson::padded_string padded(json_content);
                simdjson::ondemand::document doc;
                
                auto error = parser.iterate(padded).get(doc);
                if (!error) {
                    // Check if it's an array or single document
                    simdjson::ondemand::array arr;
                    error = doc.get_array().get(arr);
                    
                    if (!error) {
                        // Array of documents
                        for (auto elem : arr) {
                            simdjson::ondemand::object obj;
                            if (elem.get_object().get(obj) == simdjson::SUCCESS) {
                                Document new_doc;
                                if (parseDocumentObject(obj, new_doc, result)) {
                                    processDocumentText(new_doc);
                                    local_docs.push_back(std::move(new_doc));
                                    docs_loaded++;
                                }
                            }
                        }
                    } else {
                        // Single document
                        simdjson::ondemand::object obj;
                        if (doc.get_object().get(obj) == simdjson::SUCCESS) {
                            Document new_doc;
                            if (parseDocumentObject(obj, new_doc, result)) {
                                processDocumentText(new_doc);
                                local_docs.push_back(std::move(new_doc));
                                docs_loaded++;
                            }
                        }
                    }
                }
                
                files_processed++;
                if (verbose && files_processed % 100 == 0) {
                    std::cout << "  Processed " << files_processed << " files...\r" << std::flush;
                }
                
                delete data;
            }
        });
    }
    
    // Wait for producer
    producer.join();
    
    // Wait for consumers
    for (auto& consumer : consumers) {
        consumer.join();
    }
    
    // Merge thread-local documents into result
    size_t total_docs = 0;
    for (const auto& thread_docs : thread_documents) {
        total_docs += thread_docs.size();
    }
    result.documents.reserve(total_docs);
    
    size_t doc_id = 0;
    for (const auto& thread_docs : thread_documents) {
        for (const auto& doc : thread_docs) {
            result.documents.push_back(doc);
            
            // Build BM25 index
            result.total_tokens += doc.length;
            for (const auto& [term, tf] : doc.term_frequencies) {
                result.postings[term].emplace_back(doc_id, tf);
                if (tf > 0) {
                    result.document_frequencies[term]++;
                }
            }
            doc_id++;
        }
    }
    
    // Calculate average document length
    if (!result.documents.empty()) {
        result.average_document_length = static_cast<double>(result.total_tokens) / result.documents.size();
    }
    
    if (verbose) {
        std::cout << "\nLoaded " << result.documents.size() << " documents from " 
                  << files_processed << " files\n";
    }
    
    return result;
}

// Helper function to parse a document object
static bool parseDocumentObject(simdjson::ondemand::object& obj, 
                                DocumentLoader::Document& doc,
                                DocumentLoader::LoadResult& result) {
    // Get ID
    std::string_view id_view;
    if (obj["id"].get_string().get(id_view)) {
        return false;
    }
    doc.id = std::string(id_view);
    
    // Auto-detect text field on first document
    std::string_view text_view;
    if (result.text_field == DocumentLoader::LoadResult::TextField::UNKNOWN) {
        auto text_err = obj["text"].get_string().get(text_view);
        if (!text_err) {
            result.text_field = DocumentLoader::LoadResult::TextField::TEXT;
        } else if (obj["content"].get_string().get(text_view) == simdjson::SUCCESS) {
            result.text_field = DocumentLoader::LoadResult::TextField::CONTENT;
        } else {
            return false;
        }
    } else if (result.text_field == DocumentLoader::LoadResult::TextField::TEXT) {
        if (obj["text"].get_string().get(text_view)) {
            return false;
        }
    } else {
        if (obj["content"].get_string().get(text_view)) {
            return false;
        }
    }
    doc.text = std::string(text_view);
    
    // Get embedding from metadata
    simdjson::ondemand::object metadata;
    if (obj["metadata"].get_object().get(metadata)) {
        return false;
    }
    
    simdjson::ondemand::array embedding_arr;
    if (metadata["embedding"].get_array().get(embedding_arr)) {
        return false;
    }
    
    // Parse embedding
    doc.embedding.clear();
    for (auto val : embedding_arr) {
        double d;
        if (val.get_double().get(d) == simdjson::SUCCESS) {
            doc.embedding.push_back(static_cast<float>(d));
        }
    }
    
    // Auto-detect dimensions from first document
    if (result.dimensions == 0) {
        result.dimensions = doc.embedding.size();
    } else if (doc.embedding.size() != result.dimensions) {
        return false;  // Dimension mismatch
    }
    
    // Store full metadata as JSON string
    doc.metadata_json = R"({"embedding":[)";
    for (size_t i = 0; i < doc.embedding.size(); ++i) {
        if (i > 0) doc.metadata_json += ",";
        doc.metadata_json += std::to_string(doc.embedding[i]);
    }
    doc.metadata_json += "]}";
    
    return true;
}

void DocumentLoader::processDocumentText(Document& doc) {
    SimpleTokenizer tokenizer;
    auto tokens = tokenizer.split(doc.text);
    doc.length = tokens.size();
    
    for (const auto& token : tokens) {
        doc.term_frequencies[token]++;
    }
}