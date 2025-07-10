#include <napi.h>
#include "vector_store.h"
#include "vector_store_loader.h"
#include <cmath>

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
        VectorStoreLoader::loadDirectory(store_.get(), path);
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