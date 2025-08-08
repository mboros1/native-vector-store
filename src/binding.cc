#include <napi.h>
#include "vector_store.h"
#include "vector_store_loader.h"
#include "simple_tokenizer.h"
#include <cmath>
#include <algorithm>
#include <cctype>

class VectorStoreWrapper : public Napi::ObjectWrap<VectorStoreWrapper> {
    std::unique_ptr<VectorStore> store_;
    size_t dim_;
    
public:
    static Napi::Object Init(Napi::Env env, Napi::Object exports) {
        Napi::Function func = DefineClass(env, "VectorStore", {
            InstanceMethod("loadDir", &VectorStoreWrapper::LoadDir),
            InstanceMethod("loadDirMMap", &VectorStoreWrapper::LoadDirMMap),
            InstanceMethod("loadDirAdaptive", &VectorStoreWrapper::LoadDirAdaptive),
            InstanceMethod("addDocument", &VectorStoreWrapper::AddDocument),
            InstanceMethod("search", &VectorStoreWrapper::Search),
            InstanceMethod("searchVector", &VectorStoreWrapper::SearchVector),
            InstanceMethod("searchBM25", &VectorStoreWrapper::SearchBM25),
            InstanceMethod("searchHybrid", &VectorStoreWrapper::SearchHybrid),
            InstanceMethod("normalize", &VectorStoreWrapper::Normalize),
            InstanceMethod("finalize", &VectorStoreWrapper::FinalizeStore),
            InstanceMethod("isFinalized", &VectorStoreWrapper::IsFinalized),
            InstanceMethod("size", &VectorStoreWrapper::Size),
            InstanceMethod("setBM25Parameters", &VectorStoreWrapper::SetBM25Parameters)
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
        // Use adaptive loader as default for best performance
        VectorStoreLoader::loadDirectoryAdaptive(store_.get(), path);
    }
    
    void LoadDirMMap(const Napi::CallbackInfo& info) {
        std::string path = info[0].As<Napi::String>();
        VectorStoreLoader::loadDirectoryMMap(store_.get(), path);
    }
    
    void LoadDirAdaptive(const Napi::CallbackInfo& info) {
        std::string path = info[0].As<Napi::String>();
        VectorStoreLoader::loadDirectoryAdaptive(store_.get(), path);
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
        if (add_error != VectorStoreError::SUCCESS) {
            Napi::Error::New(info.Env(), 
                std::string("Document add error: ") + vector_store_error_message(add_error))
                .ThrowAsJavaScriptException();
            return;
        }
    }
    
    Napi::Value Search(const Napi::CallbackInfo& info) {
        // Default search - uses hybrid if query text is provided, otherwise vector-only
        Napi::Env env = info.Env();
        Napi::Float32Array query_array = info[0].As<Napi::Float32Array>();
        size_t k = info[1].As<Napi::Number>().Uint32Value();
        
        // Check for optional query text (for hybrid search)
        std::string query_text;
        if (info.Length() > 2 && info[2].IsString()) {
            query_text = info[2].As<Napi::String>().Utf8Value();
        }
        
        // If query text provided, use hybrid search
        if (!query_text.empty()) {
            // Tokenize query text
            SimpleTokenizer tokenizer;
            std::vector<std::string> tokens = tokenizer.split(query_text);
            
            // Convert to lowercase
            std::vector<std::string> query_terms;
            for (const auto& token : tokens) {
                std::string lower_token = token;
                std::transform(lower_token.begin(), lower_token.end(), lower_token.begin(), ::tolower);
                if (!lower_token.empty()) {
                    query_terms.push_back(lower_token);
                }
            }
            
            // Use hybrid search with default weights (0.5/0.5)
            std::vector<float> query(query_array.Data(), 
                                     query_array.Data() + query_array.ElementLength());
            
            // Normalize query vector
            float sum = 0.0f;
            for (float v : query) sum += v * v;
            if (sum > 1e-10f) {
                float inv_norm = 1.0f / std::sqrt(sum);
                for (float& v : query) v *= inv_norm;
            }
            
            auto results = store_->search_hybrid(query.data(), query_terms, 0.5, 0.5, k);
            
            Napi::Array output = Napi::Array::New(env, results.size());
            for (size_t i = 0; i < results.size(); ++i) {
                const auto& entry = store_->get_entry(results[i].first);
                
                Napi::Object result = Napi::Object::New(env);
                result.Set("score", results[i].second);
                result.Set("id", std::string(entry.doc.id));
                result.Set("text", std::string(entry.doc.text));
                result.Set("metadata_json", std::string(entry.doc.metadata_json));
                
                output[i] = result;
            }
            
            return output;
        } else {
            // Fall back to vector-only search
            return SearchVector(info);
        }
    }
    
    Napi::Value SearchVector(const Napi::CallbackInfo& info) {
        // Pure vector search (original implementation)
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
            result.Set("metadata_json", std::string(entry.doc.metadata_json));
            
            output[i] = result;
        }
        
        return output;
    }
    
    Napi::Value SearchBM25(const Napi::CallbackInfo& info) {
        // Pure BM25 text search
        Napi::Env env = info.Env();
        std::vector<std::string> query_terms;
        
        // Accept either string or array of strings
        if (info[0].IsString()) {
            std::string query_text = info[0].As<Napi::String>().Utf8Value();
            SimpleTokenizer tokenizer;
            std::vector<std::string> tokens = tokenizer.split(query_text);
            
            for (const auto& token : tokens) {
                std::string lower_token = token;
                std::transform(lower_token.begin(), lower_token.end(), lower_token.begin(), ::tolower);
                if (!lower_token.empty()) {
                    query_terms.push_back(lower_token);
                }
            }
        } else if (info[0].IsArray()) {
            Napi::Array terms_array = info[0].As<Napi::Array>();
            for (uint32_t i = 0; i < terms_array.Length(); ++i) {
                std::string term = terms_array.Get(i).As<Napi::String>().Utf8Value();
                std::transform(term.begin(), term.end(), term.begin(), ::tolower);
                if (!term.empty()) {
                    query_terms.push_back(term);
                }
            }
        }
        
        auto results = store_->search_bm25(query_terms);
        
        Napi::Array output = Napi::Array::New(env, results.size());
        for (size_t i = 0; i < results.size(); ++i) {
            const auto& entry = store_->get_entry(results[i].first);
            
            Napi::Object result = Napi::Object::New(env);
            result.Set("score", results[i].second);
            result.Set("id", std::string(entry.doc.id));
            result.Set("text", std::string(entry.doc.text));
            result.Set("metadata_json", std::string(entry.doc.metadata_json));
            
            output[i] = result;
        }
        
        return output;
    }
    
    Napi::Value SearchHybrid(const Napi::CallbackInfo& info) {
        // Explicit hybrid search with configurable weights
        Napi::Env env = info.Env();
        Napi::Float32Array query_array = info[0].As<Napi::Float32Array>();
        std::string query_text = info[1].As<Napi::String>().Utf8Value();
        size_t k = info[2].As<Napi::Number>().Uint32Value();
        
        // Optional weights (default 0.5/0.5)
        double vector_weight = 0.5;
        double bm25_weight = 0.5;
        if (info.Length() > 3) {
            vector_weight = info[3].As<Napi::Number>().DoubleValue();
        }
        if (info.Length() > 4) {
            bm25_weight = info[4].As<Napi::Number>().DoubleValue();
        }
        
        // Tokenize query text
        SimpleTokenizer tokenizer;
        std::vector<std::string> tokens = tokenizer.split(query_text);
        std::vector<std::string> query_terms;
        
        for (const auto& token : tokens) {
            std::string lower_token = token;
            std::transform(lower_token.begin(), lower_token.end(), lower_token.begin(), ::tolower);
            if (!lower_token.empty()) {
                query_terms.push_back(lower_token);
            }
        }
        
        // Prepare query vector
        std::vector<float> query(query_array.Data(), 
                                 query_array.Data() + query_array.ElementLength());
        
        // Normalize query vector
        float sum = 0.0f;
        for (float v : query) sum += v * v;
        if (sum > 1e-10f) {
            float inv_norm = 1.0f / std::sqrt(sum);
            for (float& v : query) v *= inv_norm;
        }
        
        auto results = store_->search_hybrid(query.data(), query_terms, vector_weight, bm25_weight, k);
        
        Napi::Array output = Napi::Array::New(env, results.size());
        for (size_t i = 0; i < results.size(); ++i) {
            const auto& entry = store_->get_entry(results[i].first);
            
            Napi::Object result = Napi::Object::New(env);
            result.Set("score", results[i].second);
            result.Set("id", std::string(entry.doc.id));
            result.Set("text", std::string(entry.doc.text));
            result.Set("metadata_json", std::string(entry.doc.metadata_json));
            
            output[i] = result;
        }
        
        return output;
    }
    
    void SetBM25Parameters(const Napi::CallbackInfo& info) {
        double k1 = info[0].As<Napi::Number>().DoubleValue();
        double b = info[1].As<Napi::Number>().DoubleValue();
        double delta = info.Length() > 2 ? info[2].As<Napi::Number>().DoubleValue() : 1.0;
        
        store_->set_bm25_parameters(k1, b, delta);
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