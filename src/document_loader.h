#pragma once
#include <string>
#include <vector>
#include <unordered_map>
#include <memory>
#include <string_view>
#include <atomic>

namespace nvs {

/**
 * General-purpose document loader that returns structured data.
 *
 * Ownership:
 * - All strings in Document (id, text, metadata_json) are owned and copied into the result.
 * - The loader does not retain references to input buffers or files; after return, the LoadResult
 *   fully owns its data.
 *
 * Behavior:
 * - Accepts JSON files that contain either a single document object or an array of document objects.
 * - Detects text field name on the first document ("text" vs "content") and applies that to subsequent docs.
 * - Expects metadata.embedding as an array of numbers; establishes a global dimension from the first doc and
 *   skips subsequent docs with mismatched dimensions.
 * - Populates basic BM25 statistics (postings, document_frequencies, average_document_length) using SimpleTokenizer.
 * - Threaded producer/consumer pipeline in production; tests may exercise minimal single-file flows.
 */

class DocumentLoader {
public:
    /**
     * Document owned representation.
     * - id, text, metadata_json are owned strings.
     * - embedding is owned and not borrowed from input buffers; dimensions are consistent across a LoadResult.
     */
    struct Document {
        std::string id;
        std::string text;
        std::vector<float> embedding;
        std::string metadata_json;  // Full JSON including embedding
        
        // BM25 fields
        size_t length = 0;  // Total number of tokens
        std::unordered_map<std::string, int> term_frequencies;
    };
    
    /**
     * Loader output and BM25 statistics. All containers own their data.
     */
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

        // Receipts: per-file document counts
        std::vector<std::pair<std::string, size_t>> receipts; // (filename, doc_count)
    };
    
    /**
     * Load all JSON documents from a directory.
     * Uses adaptive strategy: mmap for small files, streaming for large files.
     * On failure to parse a file or document, that item is skipped.
     */
    static LoadResult loadDirectory(const std::string& path, bool verbose = false);
    
    /** Helper to tokenize and build per-document term frequencies and length. */
    static void processDocumentText(Document& doc);
};

} // namespace nvs
