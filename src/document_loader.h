#pragma once
#include <string>
#include <vector>
#include <unordered_map>
#include <memory>
#include <string_view>
#include <atomic>

namespace nvs {

// General-purpose document loader that returns structured data
// This replaces vector_store_loader and provides data for both packing and direct use

class DocumentLoader {
public:
    // Document structure matching VectorStore's internal format
    struct Document {
        std::string id;
        std::string text;
        std::vector<float> embedding;
        std::string metadata_json;  // Full JSON including embedding
        
        // BM25 fields
        size_t length = 0;  // Total number of tokens
        std::unordered_map<std::string, int> term_frequencies;
    };
    
    struct LoadResult {
        std::vector<Document> documents;
        size_t dimensions = 0;
        
        // BM25 statistics
        std::unordered_map<std::string, std::vector<std::pair<size_t, int>>> postings; // term -> [(docid, tf)]
        std::unordered_map<std::string, size_t> document_frequencies;  // term -> df
        double average_document_length = 0.0;
        size_t total_tokens = 0;
        
        // Detected configuration
        enum class TextField { UNKNOWN, TEXT, CONTENT };
        TextField text_field = TextField::UNKNOWN;
    };
    
    // Load all JSON documents from a directory
    // Uses adaptive strategy: mmap for small files, streaming for large files
    static LoadResult loadDirectory(const std::string& path, bool verbose = false);
    
private:
    // File loading strategies
    static bool loadFileMMap(const std::string& path, std::vector<Document>& documents, LoadResult& result);
    static bool loadFileStream(const std::string& path, std::vector<Document>& documents, LoadResult& result);
    static bool parseDocument(const std::string& json_content, std::vector<Document>& documents, LoadResult& result);
    
    // Helper to tokenize and build term frequencies
    static void processDocumentText(Document& doc);
};

} // namespace nvs