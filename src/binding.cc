#include <napi.h>
#include "vector_store.h"
#include "atomic_queue.h"
#include <dirent.h>
#include <filesystem>
#include <fstream>
#include <cmath>
#include <cctype>
#include <vector>
#include <thread>
#include <atomic>

class VectorStoreWrapper : public Napi::ObjectWrap<VectorStoreWrapper> {
    std::unique_ptr<VectorStore> store_;
    size_t dim_;
    
public:
    static Napi::Object Init(Napi::Env env, Napi::Object exports) {
        Napi::Function func = DefineClass(env, "VectorStore", {
            InstanceMethod("loadDir", &VectorStoreWrapper::LoadDir),
            InstanceMethod("addDocument", &VectorStoreWrapper::AddDocument),
            InstanceMethod("search", &VectorStoreWrapper::Search),
            InstanceMethod("normalize", &VectorStoreWrapper::Normalize),
            InstanceMethod("finalize", &VectorStoreWrapper::FinalizeStore),
            InstanceMethod("isFinalized", &VectorStoreWrapper::IsFinalized),
            InstanceMethod("size", &VectorStoreWrapper::Size)
        });
        
        exports.Set("VectorStore", func);
        return exports;
    }
    
    VectorStoreWrapper(const Napi::CallbackInfo& info) 
        : Napi::ObjectWrap<VectorStoreWrapper>(info) {
        dim_ = info[0].As<Napi::Number>().Uint32Value();
        store_ = std::make_unique<VectorStore>(dim_);
    }
    
    void LoadDir(const Napi::CallbackInfo& info) {
        std::string path = info[0].As<Napi::String>();
        
        // Collect all JSON files
        std::vector<std::filesystem::path> json_files;
        for (const auto& entry : std::filesystem::directory_iterator(path)) {
            if (entry.path().extension() == ".json") {
                json_files.push_back(entry.path());
            }
        }
        
        if (json_files.empty()) {
            store_->finalize();
            return;
        }
        
        // Producer-consumer queue for file data
        struct FileData {
            std::string filename;
            std::string content;
        };
        
        // Queue with bounded capacity (max ~100MB of buffered data)
        // Assuming average file size of 100KB, this allows ~1000 files in queue
        atomic_queue::AtomicQueue<FileData*, 1024> queue;
        
        // Atomic flags for coordination
        std::atomic<bool> producer_done{false};
        std::atomic<size_t> files_processed{0};
        
        // Producer thread - sequential file reading
        std::thread producer([&]() {
            for (const auto& filepath : json_files) {
                try {
                    // Read file content
                    std::ifstream file(filepath, std::ios::binary | std::ios::ate);
                    if (!file) {
                        fprintf(stderr, "Error opening %s\n", filepath.c_str());
                        continue;
                    }
                    
                    std::streamsize size = file.tellg();
                    file.seekg(0, std::ios::beg);
                    
                    std::string content(size, '\0');
                    if (!file.read(content.data(), size)) {
                        fprintf(stderr, "Error reading %s\n", filepath.c_str());
                        continue;
                    }
                    
                    // Create file data and push to queue
                    auto* data = new FileData{filepath.string(), std::move(content)};
                    queue.push(data);
                    
                } catch (const std::exception& e) {
                    fprintf(stderr, "Error processing %s: %s\n", filepath.c_str(), e.what());
                }
            }
            producer_done = true;
        });
        
        // Consumer threads - parallel JSON parsing
        size_t num_workers = std::thread::hardware_concurrency();
        std::vector<std::thread> consumers;
        
        for (size_t w = 0; w < num_workers; ++w) {
            consumers.emplace_back([&]() {
                // Each thread needs its own parser
                simdjson::ondemand::parser doc_parser;
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
                                    auto add_error = store_->add_document(obj);
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
                                auto add_error = store_->add_document(obj);
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
        store_->finalize();
    }
    
    void AddDocument(const Napi::CallbackInfo& info) {
        Napi::Object doc = info[0].As<Napi::Object>();
        
        // Convert JS object to JSON string
        std::string json_str = "{";
        json_str += "\"id\":\"" + doc.Get("id").As<Napi::String>().Utf8Value() + "\",";
        json_str += "\"text\":\"" + doc.Get("text").As<Napi::String>().Utf8Value() + "\",";
        json_str += "\"metadata\":{\"embedding\":[";
        
        // Get embedding from metadata
        Napi::Object metadata = doc.Get("metadata").As<Napi::Object>();
        Napi::Array embedding = metadata.Get("embedding").As<Napi::Array>();
        
        for (uint32_t i = 0; i < embedding.Length(); ++i) {
            if (i > 0) json_str += ",";
            json_str += std::to_string(embedding.Get(i).As<Napi::Number>().DoubleValue());
        }
        json_str += "]}}";
        
        // Parse and add
        simdjson::ondemand::parser parser;
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document json_doc;
        auto parse_error = parser.iterate(padded).get(json_doc);
        if (parse_error) {
            Napi::Error::New(info.Env(), 
                std::string("JSON parse error: ") + simdjson::error_message(parse_error))
                .ThrowAsJavaScriptException();
            return;
        }
        
        auto add_error = store_->add_document(json_doc);
        if (add_error) {
            Napi::Error::New(info.Env(), 
                std::string("Document add error: ") + simdjson::error_message(add_error))
                .ThrowAsJavaScriptException();
            return;
        }
    }
    
    Napi::Value Search(const Napi::CallbackInfo& info) {
        Napi::Env env = info.Env();
        Napi::Float32Array query_array = info[0].As<Napi::Float32Array>();
        size_t k = info[1].As<Napi::Number>().Uint32Value();
        
        // Normalize query if requested
        bool normalize_query = info.Length() > 2 ? info[2].As<Napi::Boolean>() : true;
        
        std::vector<float> query(query_array.Data(), 
                                 query_array.Data() + query_array.ElementLength());
        
        if (normalize_query) {
            float sum = 0.0f;
            for (float v : query) sum += v * v;
            if (sum > 1e-10f) {
                float inv_norm = 1.0f / std::sqrt(sum);
                for (float& v : query) v *= inv_norm;
            }
        }
        
        auto results = store_->search(query.data(), k);
        
        Napi::Array output = Napi::Array::New(env, results.size());
        for (size_t i = 0; i < results.size(); ++i) {
            const auto& entry = store_->get_entry(results[i].second);
            
            Napi::Object result = Napi::Object::New(env);
            result.Set("score", results[i].first);
            result.Set("id", std::string(entry.doc.id));
            result.Set("text", std::string(entry.doc.text));
            
            // For now, return metadata as JSON string
            result.Set("metadata_json", std::string(entry.doc.metadata_json));
            
            output[i] = result;
        }
        
        return output;
    }
    
    void Normalize(const Napi::CallbackInfo& info) {
        store_->normalize_all();
    }
    
    void FinalizeStore(const Napi::CallbackInfo& info) {
        store_->finalize();
    }
    
    Napi::Value IsFinalized(const Napi::CallbackInfo& info) {
        return Napi::Boolean::New(info.Env(), store_->is_finalized());
    }
    
    Napi::Value Size(const Napi::CallbackInfo& info) {
        return Napi::Number::New(info.Env(), store_->size());
    }
};

Napi::Object Init(Napi::Env env, Napi::Object exports) {
    return VectorStoreWrapper::Init(env, exports);
}

NODE_API_MODULE(vector_store, Init)