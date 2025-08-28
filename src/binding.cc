#include <napi.h>
#include "vector_store_v2.h"
#include "simple_tokenizer.h"
#include <cmath>
#include <algorithm>
#include <cctype>
#include <memory>

class VectorStoreWrapper : public Napi::ObjectWrap<VectorStoreWrapper> {
    std::unique_ptr<nvs::VectorStoreV2> store_;
    
public:
    static Napi::Object Init(Napi::Env env, Napi::Object exports) {
        Napi::Function func = DefineClass(env, "VectorStore", {
            InstanceMethod("open", &VectorStoreWrapper::Open),
            InstanceMethod("close", &VectorStoreWrapper::Close),
            InstanceMethod("isOpen", &VectorStoreWrapper::IsOpen),
            InstanceMethod("search", &VectorStoreWrapper::Search),
            InstanceMethod("searchBM25", &VectorStoreWrapper::SearchBM25),
            InstanceMethod("searchHybrid", &VectorStoreWrapper::SearchHybrid),
            InstanceMethod("size", &VectorStoreWrapper::Size),
            InstanceMethod("dimensions", &VectorStoreWrapper::Dimensions),
            InstanceMethod("getDocument", &VectorStoreWrapper::GetDocument)
        });
        
        exports.Set("VectorStore", func);
        return exports;
    }
    
    VectorStoreWrapper(const Napi::CallbackInfo& info) 
        : Napi::ObjectWrap<VectorStoreWrapper>(info) {
        store_ = std::make_unique<nvs::VectorStoreV2>();
        
        // If a bundle path is provided, open it immediately
        if (info.Length() > 0 && info[0].IsString()) {
            std::string bundlePath = info[0].As<Napi::String>();
            if (!store_->open(bundlePath)) {
                Napi::Error::New(info.Env(), "Failed to open bundle: " + bundlePath).ThrowAsJavaScriptException();
            }
        }
    }
    
    void Open(const Napi::CallbackInfo& info) {
        if (info.Length() < 1 || !info[0].IsString()) {
            Napi::TypeError::New(info.Env(), "Bundle path string expected").ThrowAsJavaScriptException();
            return;
        }
        
        std::string bundlePath = info[0].As<Napi::String>();
        if (!store_->open(bundlePath)) {
            Napi::Error::New(info.Env(), "Failed to open bundle: " + bundlePath).ThrowAsJavaScriptException();
        }
    }
    
    void Close(const Napi::CallbackInfo& info) {
        store_->close();
    }
    
    Napi::Value IsOpen(const Napi::CallbackInfo& info) {
        return Napi::Boolean::New(info.Env(), store_->is_open());
    }
    
    Napi::Value Size(const Napi::CallbackInfo& info) {
        return Napi::Number::New(info.Env(), store_->size());
    }
    
    Napi::Value Dimensions(const Napi::CallbackInfo& info) {
        return Napi::Number::New(info.Env(), store_->dimensions());
    }
    
    Napi::Value Search(const Napi::CallbackInfo& info) {
        if (!store_->is_open()) {
            Napi::Error::New(info.Env(), "Store is not open").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        if (info.Length() < 2 || !info[0].IsTypedArray() || !info[1].IsNumber()) {
            Napi::TypeError::New(info.Env(), "Expected Float32Array and number k").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        auto queryArray = info[0].As<Napi::Float32Array>();
        size_t k = info[1].As<Napi::Number>().Uint32Value();
        
        if (queryArray.ElementLength() != store_->dimensions()) {
            Napi::Error::New(info.Env(), 
                "Query dimensions mismatch. Expected " + std::to_string(store_->dimensions()) + 
                " but got " + std::to_string(queryArray.ElementLength())).ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        float* query = queryArray.Data();
        auto results = store_->search(query, k);
        
        Napi::Array jsResults = Napi::Array::New(info.Env(), results.size());
        for (size_t i = 0; i < results.size(); i++) {
            Napi::Object result = Napi::Object::New(info.Env());
            result.Set("id", results[i].id);
            result.Set("score", results[i].score);
            result.Set("text", results[i].text);
            result.Set("metadata", results[i].metadata_json);
            jsResults[i] = result;
        }
        
        return jsResults;
    }
    
    Napi::Value SearchBM25(const Napi::CallbackInfo& info) {
        if (!store_->is_open()) {
            Napi::Error::New(info.Env(), "Store is not open").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        if (info.Length() < 2 || !info[0].IsString() || !info[1].IsNumber()) {
            Napi::TypeError::New(info.Env(), "Expected query string and number k").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        std::string query = info[0].As<Napi::String>();
        size_t k = info[1].As<Napi::Number>().Uint32Value();
        
        // Tokenize query
        SimpleTokenizer tokenizer;
        auto queryTerms = tokenizer.split(query);
        
        auto results = store_->search_bm25(queryTerms, k);
        
        Napi::Array jsResults = Napi::Array::New(info.Env(), results.size());
        for (size_t i = 0; i < results.size(); i++) {
            Napi::Object result = Napi::Object::New(info.Env());
            result.Set("id", results[i].id);
            result.Set("score", results[i].score);
            result.Set("text", results[i].text);
            result.Set("metadata", results[i].metadata_json);
            jsResults[i] = result;
        }
        
        return jsResults;
    }
    
    Napi::Value SearchHybrid(const Napi::CallbackInfo& info) {
        if (!store_->is_open()) {
            Napi::Error::New(info.Env(), "Store is not open").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        if (info.Length() < 3 || !info[0].IsTypedArray() || !info[1].IsString() || !info[2].IsNumber()) {
            Napi::TypeError::New(info.Env(), "Expected Float32Array, query string, and number k").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        auto queryArray = info[0].As<Napi::Float32Array>();
        std::string textQuery = info[1].As<Napi::String>();
        size_t k = info[2].As<Napi::Number>().Uint32Value();
        
        // Optional vector weight (default 0.7)
        double vectorWeight = 0.7;
        if (info.Length() > 3 && info[3].IsNumber()) {
            vectorWeight = info[3].As<Napi::Number>().DoubleValue();
        }
        
        if (queryArray.ElementLength() != store_->dimensions()) {
            Napi::Error::New(info.Env(), 
                "Query dimensions mismatch. Expected " + std::to_string(store_->dimensions()) + 
                " but got " + std::to_string(queryArray.ElementLength())).ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        float* query = queryArray.Data();
        
        // Tokenize text query
        SimpleTokenizer tokenizer;
        auto queryTerms = tokenizer.split(textQuery);
        
        auto results = store_->search_hybrid(query, queryTerms, k, vectorWeight);
        
        Napi::Array jsResults = Napi::Array::New(info.Env(), results.size());
        for (size_t i = 0; i < results.size(); i++) {
            Napi::Object result = Napi::Object::New(info.Env());
            result.Set("id", results[i].id);
            result.Set("score", results[i].score);
            result.Set("text", results[i].text);
            result.Set("metadata", results[i].metadata_json);
            jsResults[i] = result;
        }
        
        return jsResults;
    }
    
    Napi::Value GetDocument(const Napi::CallbackInfo& info) {
        if (!store_->is_open()) {
            Napi::Error::New(info.Env(), "Store is not open").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        if (info.Length() < 1 || !info[0].IsNumber()) {
            Napi::TypeError::New(info.Env(), "Document ID number expected").ThrowAsJavaScriptException();
            return info.Env().Null();
        }
        
        size_t docId = info[0].As<Napi::Number>().Uint32Value();
        nvs::VectorStoreV2::SearchResult result;
        
        if (!store_->get_document(docId, result)) {
            return info.Env().Null();
        }
        
        Napi::Object jsResult = Napi::Object::New(info.Env());
        jsResult.Set("id", result.id);
        jsResult.Set("text", result.text);
        jsResult.Set("metadata", result.metadata_json);
        
        return jsResult;
    }
};

// Initialize the module
Napi::Object Init(Napi::Env env, Napi::Object exports) {
    VectorStoreWrapper::Init(env, exports);
    return exports;
}

NODE_API_MODULE(NODE_GYP_MODULE_NAME, Init)