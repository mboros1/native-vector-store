#include <napi.h>
#include "fast_pdf_parser/hierarchical_chunker.h"
#include <sstream>

using namespace fast_pdf_parser;

class HierarchicalChunkerWrapper : public Napi::ObjectWrap<HierarchicalChunkerWrapper> {
public:
    static Napi::Object Init(Napi::Env env, Napi::Object exports);
    HierarchicalChunkerWrapper(const Napi::CallbackInfo& info);
    ~HierarchicalChunkerWrapper();

private:
    static Napi::FunctionReference constructor;
    std::unique_ptr<HierarchicalChunker> chunker_;
    
    Napi::Value ChunkFile(const Napi::CallbackInfo& info);
    Napi::Value GetOptions(const Napi::CallbackInfo& info);
    void SetOptions(const Napi::CallbackInfo& info);
};

Napi::FunctionReference HierarchicalChunkerWrapper::constructor;

Napi::Object HierarchicalChunkerWrapper::Init(Napi::Env env, Napi::Object exports) {
    Napi::Function func = DefineClass(env, "HierarchicalChunker", {
        InstanceMethod("chunkFile", &HierarchicalChunkerWrapper::ChunkFile),
        InstanceMethod("getOptions", &HierarchicalChunkerWrapper::GetOptions),
        InstanceMethod("setOptions", &HierarchicalChunkerWrapper::SetOptions)
    });
    
    constructor = Napi::Persistent(func);
    constructor.SuppressDestruct();
    
    exports.Set("HierarchicalChunker", func);
    return exports;
}

HierarchicalChunkerWrapper::HierarchicalChunkerWrapper(const Napi::CallbackInfo& info) 
    : Napi::ObjectWrap<HierarchicalChunkerWrapper>(info) {
    
    ChunkOptions options;
    
    // Parse options if provided
    if (info.Length() > 0 && info[0].IsObject()) {
        Napi::Object opts = info[0].As<Napi::Object>();
        
        if (opts.Has("maxTokens") && opts.Get("maxTokens").IsNumber()) {
            options.max_tokens = opts.Get("maxTokens").As<Napi::Number>().Int32Value();
        }
        if (opts.Has("minTokens") && opts.Get("minTokens").IsNumber()) {
            options.min_tokens = opts.Get("minTokens").As<Napi::Number>().Int32Value();
        }
        if (opts.Has("overlapTokens") && opts.Get("overlapTokens").IsNumber()) {
            options.overlap_tokens = opts.Get("overlapTokens").As<Napi::Number>().Int32Value();
        }
        if (opts.Has("threadCount") && opts.Get("threadCount").IsNumber()) {
            options.thread_count = opts.Get("threadCount").As<Napi::Number>().Int32Value();
        }
    }
    
    chunker_ = std::make_unique<HierarchicalChunker>(options);
}

HierarchicalChunkerWrapper::~HierarchicalChunkerWrapper() = default;

Napi::Value HierarchicalChunkerWrapper::ChunkFile(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    
    if (info.Length() < 1 || !info[0].IsString()) {
        Napi::Error::New(env, "First argument must be a PDF file path").ThrowAsJavaScriptException();
        return env.Null();
    }
    
    std::string pdf_path = info[0].As<Napi::String>().Utf8Value();
    int page_limit = -1;
    
    if (info.Length() > 1 && info[1].IsNumber()) {
        page_limit = info[1].As<Napi::Number>().Int32Value();
    }
    
    try {
        ChunkingResult result = chunker_->chunk_file(pdf_path, page_limit);
        
        // Check for errors
        if (!result.error.empty()) {
            Napi::Error::New(env, result.error).ThrowAsJavaScriptException();
            return env.Null();
        }
        
        // Convert result to JavaScript object
        Napi::Object js_result = Napi::Object::New(env);
        
        // Create chunks array
        Napi::Array chunks_array = Napi::Array::New(env, result.chunks.size());
        for (size_t i = 0; i < result.chunks.size(); ++i) {
            const auto& chunk = result.chunks[i];
            
            Napi::Object chunk_obj = Napi::Object::New(env);
            chunk_obj.Set("text", Napi::String::New(env, chunk.text));
            chunk_obj.Set("tokenCount", Napi::Number::New(env, chunk.token_count));
            chunk_obj.Set("startPage", Napi::Number::New(env, chunk.start_page));
            chunk_obj.Set("endPage", Napi::Number::New(env, chunk.end_page));
            chunk_obj.Set("hasMajorHeading", Napi::Boolean::New(env, chunk.has_major_heading));
            chunk_obj.Set("minHeadingLevel", Napi::Number::New(env, chunk.min_heading_level));
            
            chunks_array.Set(i, chunk_obj);
        }
        
        js_result.Set("chunks", chunks_array);
        js_result.Set("totalPages", Napi::Number::New(env, result.total_pages));
        js_result.Set("totalChunks", Napi::Number::New(env, result.total_chunks));
        js_result.Set("processingTimeMs", Napi::Number::New(env, result.processing_time_ms));
        
        return js_result;
        
    } catch (const std::exception& e) {
        Napi::Error::New(env, e.what()).ThrowAsJavaScriptException();
        return env.Null();
    }
}

Napi::Value HierarchicalChunkerWrapper::GetOptions(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    
    ChunkOptions options = chunker_->get_options();
    
    Napi::Object js_options = Napi::Object::New(env);
    js_options.Set("maxTokens", Napi::Number::New(env, options.max_tokens));
    js_options.Set("minTokens", Napi::Number::New(env, options.min_tokens));
    js_options.Set("overlapTokens", Napi::Number::New(env, options.overlap_tokens));
    js_options.Set("threadCount", Napi::Number::New(env, options.thread_count));
    
    return js_options;
}

void HierarchicalChunkerWrapper::SetOptions(const Napi::CallbackInfo& info) {
    Napi::Env env = info.Env();
    
    if (info.Length() < 1 || !info[0].IsObject()) {
        Napi::Error::New(env, "First argument must be an options object").ThrowAsJavaScriptException();
        return;
    }
    
    Napi::Object opts = info[0].As<Napi::Object>();
    ChunkOptions options = chunker_->get_options();
    
    if (opts.Has("maxTokens") && opts.Get("maxTokens").IsNumber()) {
        options.max_tokens = opts.Get("maxTokens").As<Napi::Number>().Int32Value();
    }
    if (opts.Has("minTokens") && opts.Get("minTokens").IsNumber()) {
        options.min_tokens = opts.Get("minTokens").As<Napi::Number>().Int32Value();
    }
    if (opts.Has("overlapTokens") && opts.Get("overlapTokens").IsNumber()) {
        options.overlap_tokens = opts.Get("overlapTokens").As<Napi::Number>().Int32Value();
    }
    if (opts.Has("threadCount") && opts.Get("threadCount").IsNumber()) {
        options.thread_count = opts.Get("threadCount").As<Napi::Number>().Int32Value();
    }
    
    chunker_->set_options(options);
}

Napi::Object Init(Napi::Env env, Napi::Object exports) {
    return HierarchicalChunkerWrapper::Init(env, exports);
}

NODE_API_MODULE(fast_pdf_parser, Init)