// Test program to verify bundle format integrity
#include <iostream>
#include <fstream>
#include <filesystem>
#include <vector>
#include <string>
#include <cstring>
#include <cmath>
#include <unordered_map>
#include <cassert>
#include <simdjson.h>
#include <sys/mman.h>
#include <fcntl.h>
#include <unistd.h>

namespace fs = std::filesystem;

// Color output for test results
#define GREEN "\033[32m"
#define RED "\033[31m"
#define YELLOW "\033[33m"
#define RESET "\033[0m"

void test_pass(const std::string& test_name) {
    std::cout << GREEN << "✓ " << RESET << test_name << std::endl;
}

void test_fail(const std::string& test_name, const std::string& reason) {
    std::cout << RED << "✗ " << RESET << test_name << ": " << reason << std::endl;
}

// Memory-mapped file helper
class MMapFile {
    void* data_ = nullptr;
    size_t size_ = 0;
    int fd_ = -1;
    
public:
    ~MMapFile() { close(); }
    
    bool open(const std::string& path) {
        close();
        fd_ = ::open(path.c_str(), O_RDONLY);
        if (fd_ < 0) return false;
        
        off_t file_size = lseek(fd_, 0, SEEK_END);
        if (file_size < 0) {
            ::close(fd_);
            fd_ = -1;
            return false;
        }
        
        size_ = static_cast<size_t>(file_size);
        if (size_ == 0) {
            ::close(fd_);
            fd_ = -1;
            return true;
        }
        
        data_ = mmap(nullptr, size_, PROT_READ, MAP_PRIVATE, fd_, 0);
        if (data_ == MAP_FAILED) {
            data_ = nullptr;
            ::close(fd_);
            fd_ = -1;
            return false;
        }
        
        return true;
    }
    
    void close() {
        if (data_ && data_ != MAP_FAILED) {
            munmap(data_, size_);
            data_ = nullptr;
        }
        if (fd_ >= 0) {
            ::close(fd_);
            fd_ = -1;
        }
        size_ = 0;
    }
    
    void* data() const { return data_; }
    size_t size() const { return size_; }
    
    template<typename T>
    const T* as() const { return static_cast<const T*>(data_); }
};

// Test manifest loading
bool test_manifest(const std::string& bundle_path) {
    std::cout << "\n" << YELLOW << "Testing Manifest..." << RESET << "\n";
    
    std::ifstream file(bundle_path + "/manifest.json");
    if (!file) {
        test_fail("Manifest exists", "File not found");
        return false;
    }
    
    std::string json_str((std::istreambuf_iterator<char>(file)),
                        std::istreambuf_iterator<char>());
    
    simdjson::ondemand::parser parser;
    simdjson::padded_string padded(json_str);
    simdjson::ondemand::document doc;
    
    auto error = parser.iterate(padded).get(doc);
    if (error) {
        test_fail("Manifest parse", "JSON parse error");
        return false;
    }
    
    // Check required fields
    uint64_t num_docs, dim;
    std::string_view format;
    
    if (doc["format"].get_string().get(format)) {
        test_fail("Manifest format field", "Missing or invalid");
        return false;
    }
    test_pass("Manifest format: " + std::string(format));
    
    if (doc["num_docs"].get_uint64().get(num_docs)) {
        test_fail("Manifest num_docs", "Missing or invalid");
        return false;
    }
    test_pass("Manifest num_docs: " + std::to_string(num_docs));
    
    if (doc["dim"].get_uint64().get(dim)) {
        test_fail("Manifest dim", "Missing or invalid");
        return false;
    }
    test_pass("Manifest dimensions: " + std::to_string(dim));
    
    // Check BM25 params
    simdjson::ondemand::object bm25;
    double avgdl;
    if (!doc["bm25"].get_object().get(bm25) && !bm25["avgdl"].get_double().get(avgdl)) {
        test_pass("BM25 avgdl: " + std::to_string(avgdl));
    }
    
    return true;
}

// Test vector data
bool test_vectors(const std::string& bundle_path, size_t expected_docs, size_t expected_dim) {
    std::cout << "\n" << YELLOW << "Testing Vectors..." << RESET << "\n";
    
    MMapFile vectors;
    if (!vectors.open(bundle_path + "/vectors.f32")) {
        test_fail("Vectors file open", "Failed to open vectors.f32");
        return false;
    }
    test_pass("Vectors file opened");
    
    // Calculate expected size with alignment
    size_t row_size = expected_dim * sizeof(float);
    size_t aligned_row_size = ((row_size + 63) / 64) * 64;  // 64-byte alignment
    size_t expected_size = aligned_row_size * expected_docs;
    
    if (vectors.size() != expected_size) {
        test_fail("Vectors file size", 
                 "Expected " + std::to_string(expected_size) + 
                 " bytes, got " + std::to_string(vectors.size()));
        return false;
    }
    test_pass("Vectors file size correct: " + std::to_string(vectors.size()) + " bytes");
    
    // Test alignment
    if ((uintptr_t)vectors.data() % 64 != 0) {
        std::cout << YELLOW << "⚠ Vector data not 64-byte aligned in memory (OK for mmap)" << RESET << "\n";
    } else {
        test_pass("Vector data is 64-byte aligned");
    }
    
    // Sample first and last vectors
    const float* first_vec = vectors.as<float>();
    const float* last_vec = reinterpret_cast<const float*>(
        static_cast<const char*>(vectors.data()) + aligned_row_size * (expected_docs - 1)
    );
    
    // Check if vectors are normalized (should sum to ~1 for unit vectors)
    float first_norm = 0, last_norm = 0;
    for (size_t i = 0; i < expected_dim; ++i) {
        first_norm += first_vec[i] * first_vec[i];
        last_norm += last_vec[i] * last_vec[i];
    }
    first_norm = std::sqrt(first_norm);
    last_norm = std::sqrt(last_norm);
    
    std::cout << "  First vector L2 norm: " << first_norm << "\n";
    std::cout << "  Last vector L2 norm: " << last_norm << "\n";
    
    if (std::abs(first_norm - 1.0) < 0.1) {
        test_pass("Vectors appear to be normalized");
    } else {
        std::cout << YELLOW << "⚠ Vectors may not be normalized (OK for raw embeddings)" << RESET << "\n";
    }
    
    return true;
}

// Test document lengths
bool test_doclengths(const std::string& bundle_path, size_t expected_docs) {
    std::cout << "\n" << YELLOW << "Testing Document Lengths..." << RESET << "\n";
    
    MMapFile doclen;
    if (!doclen.open(bundle_path + "/doclen.u32")) {
        test_fail("DocLen file open", "Failed to open doclen.u32");
        return false;
    }
    test_pass("DocLen file opened");
    
    size_t expected_size = expected_docs * sizeof(uint32_t);
    if (doclen.size() != expected_size) {
        test_fail("DocLen file size", 
                 "Expected " + std::to_string(expected_size) + 
                 " bytes, got " + std::to_string(doclen.size()));
        return false;
    }
    test_pass("DocLen file size correct: " + std::to_string(doclen.size()) + " bytes");
    
    // Sample some document lengths
    const uint32_t* lengths = doclen.as<uint32_t>();
    uint64_t total_length = 0;
    uint32_t min_len = UINT32_MAX, max_len = 0;
    
    for (size_t i = 0; i < expected_docs; ++i) {
        total_length += lengths[i];
        min_len = std::min(min_len, lengths[i]);
        max_len = std::max(max_len, lengths[i]);
    }
    
    double avg_length = static_cast<double>(total_length) / expected_docs;
    
    std::cout << "  Min doc length: " << min_len << "\n";
    std::cout << "  Max doc length: " << max_len << "\n";
    std::cout << "  Avg doc length: " << avg_length << "\n";
    
    test_pass("Document lengths validated");
    return true;
}

// Test BM25 index structures
bool test_bm25_index(const std::string& bundle_path) {
    std::cout << "\n" << YELLOW << "Testing BM25 Index..." << RESET << "\n";
    
    // Load term dictionary
    MMapFile terms_file;
    if (!terms_file.open(bundle_path + "/terms.dict")) {
        test_fail("Terms dictionary open", "Failed to open terms.dict");
        return false;
    }
    test_pass("Terms dictionary opened");
    
    // Parse terms
    std::vector<std::string> terms;
    const uint8_t* data = static_cast<const uint8_t*>(terms_file.data());
    const uint8_t* end = data + terms_file.size();
    
    while (data < end) {
        if (data + sizeof(uint32_t) > end) break;
        uint32_t term_len = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        
        if (data + term_len > end) break;
        terms.emplace_back(reinterpret_cast<const char*>(data), term_len);
        data += term_len;
    }
    
    std::cout << "  Total terms: " << terms.size() << "\n";
    test_pass("Terms loaded: " + std::to_string(terms.size()));
    
    // Load lexicon
    MMapFile lexicon;
    if (!lexicon.open(bundle_path + "/lexicon.bin")) {
        test_fail("Lexicon open", "Failed to open lexicon.bin");
        return false;
    }
    
    struct LexiconEntry {
        uint64_t offset;
        uint32_t length;
        uint32_t df;
    };
    
    size_t expected_lex_size = terms.size() * sizeof(LexiconEntry);
    if (lexicon.size() != expected_lex_size) {
        test_fail("Lexicon size", "Size mismatch with term count");
        return false;
    }
    test_pass("Lexicon size matches term count");
    
    // Load postings
    MMapFile postings;
    if (!postings.open(bundle_path + "/postings.bin")) {
        test_fail("Postings open", "Failed to open postings.bin");
        return false;
    }
    test_pass("Postings file opened: " + std::to_string(postings.size()) + " bytes");
    
    // Test delta encoding for a sample term
    const LexiconEntry* lex_entries = lexicon.as<LexiconEntry>();
    
    // Find a more common term for better demonstration
    size_t sample_term_id = 0;
    std::string sample_term;
    
    // Look for a term that appears in multiple documents
    for (size_t i = 0; i < terms.size(); ++i) {
        if (lex_entries[i].df > 5 && lex_entries[i].df < 50) {  // Find moderately common term
            sample_term_id = i;
            sample_term = terms[i];
            break;
        }
    }
    
    // If no good term found, use the middle one
    if (sample_term_id == 0) {
        sample_term_id = terms.size() / 2;
        sample_term = terms[sample_term_id];
    }
    
    const auto& sample_lex = lex_entries[sample_term_id];
    
    std::cout << "\n  Testing term '" << sample_term << "':\n";
    std::cout << "    Document frequency: " << sample_lex.df << "\n";
    std::cout << "    Posting list length: " << sample_lex.length << " docs\n";
    std::cout << "    Postings offset: " << sample_lex.offset << "\n";
    
    // Decode posting list to verify delta encoding
    const uint8_t* post_data = static_cast<const uint8_t*>(postings.data()) + sample_lex.offset;
    std::vector<std::pair<uint32_t, uint32_t>> doc_tf_pairs;  // (doc_id, term_freq)
    uint32_t prev_doc = 0;
    
    for (size_t i = 0; i < sample_lex.length && i < 10; ++i) {  // Process up to 10
        uint32_t delta = *reinterpret_cast<const uint32_t*>(post_data);
        post_data += sizeof(uint32_t);
        
        uint32_t tf = *reinterpret_cast<const uint32_t*>(post_data);
        post_data += sizeof(uint32_t);
        
        uint32_t doc_id = prev_doc + delta;
        doc_tf_pairs.push_back({doc_id, tf});
        
        if (i < 3) {  // Print first 3
            std::cout << "    Doc " << doc_id << " (delta=" << delta << "), tf=" << tf << "\n";
        }
        
        prev_doc = doc_id;
    }
    
    // Verify deltas are working (doc IDs should be ascending)
    bool ascending = true;
    for (size_t i = 1; i < doc_tf_pairs.size(); ++i) {
        if (doc_tf_pairs[i].first <= doc_tf_pairs[i-1].first) {
            ascending = false;
            break;
        }
    }
    
    if (ascending) {
        test_pass("Delta encoding verified - doc IDs are ascending");
    } else {
        test_fail("Delta encoding", "Doc IDs not in ascending order");
        return false;
    }
    
    // Now link back to actual document text to verify the term appears
    std::cout << "\n  Tracing term '" << sample_term << "' back to documents:\n";
    
    // Load metadata files to retrieve document text
    MMapFile meta_idx_trace, meta_trace;
    if (meta_idx_trace.open(bundle_path + "/meta.idx") && 
        meta_trace.open(bundle_path + "/meta.bin")) {
        
        const uint64_t* offsets = meta_idx_trace.as<uint64_t>();
        
        // Show first document containing this term
        if (!doc_tf_pairs.empty()) {
            uint32_t first_doc_id = doc_tf_pairs[0].first;
            uint32_t term_freq = doc_tf_pairs[0].second;
            
            // Retrieve document metadata
            uint64_t offset = offsets[first_doc_id];
            const uint8_t* data = static_cast<const uint8_t*>(meta_trace.data()) + offset;
            
            // Read ID
            uint32_t id_len = *reinterpret_cast<const uint32_t*>(data);
            data += sizeof(uint32_t);
            std::string doc_id(reinterpret_cast<const char*>(data), id_len);
            data += id_len;
            
            // Read text
            uint32_t text_len = *reinterpret_cast<const uint32_t*>(data);
            data += sizeof(uint32_t);
            std::string text(reinterpret_cast<const char*>(data), text_len);
            
            std::cout << "    Document #" << first_doc_id << " (ID: " << doc_id << "):\n";
            std::cout << "      Term '" << sample_term << "' appears " << term_freq << " time(s)\n";
            
            // Find and highlight the term in the text
            size_t pos = text.find(sample_term);
            if (pos != std::string::npos) {
                // Show context around the term
                size_t start = (pos > 20) ? pos - 20 : 0;
                size_t end = std::min(pos + sample_term.length() + 20, text.length());
                std::cout << "      Context: \"..." << text.substr(start, end - start) << "...\"\n";
                test_pass("Term found in document text");
            } else {
                // Term might be lowercase in index but different case in text
                std::cout << "      Text preview: \"" << text.substr(0, 100) << "...\"\n";
                std::cout << YELLOW << "      Note: Term may differ in case or be part of a larger word" << RESET << "\n";
            }
        }
    }
    
    return true;
}

// Test metadata
bool test_metadata(const std::string& bundle_path, size_t expected_docs) {
    std::cout << "\n" << YELLOW << "Testing Metadata..." << RESET << "\n";
    
    MMapFile meta_idx, meta;
    if (!meta_idx.open(bundle_path + "/meta.idx")) {
        test_fail("Meta index open", "Failed to open meta.idx");
        return false;
    }
    if (!meta.open(bundle_path + "/meta.bin")) {
        test_fail("Meta data open", "Failed to open meta.bin");
        return false;
    }
    
    test_pass("Metadata files opened");
    
    // Check index size
    size_t expected_idx_size = expected_docs * sizeof(uint64_t);
    if (meta_idx.size() != expected_idx_size) {
        test_fail("Meta index size", "Size doesn't match document count");
        return false;
    }
    test_pass("Meta index size correct");
    
    // Sample first and last documents
    const uint64_t* offsets = meta_idx.as<uint64_t>();
    
    for (size_t doc_idx : {size_t(0), expected_docs - 1}) {
        uint64_t offset = offsets[doc_idx];
        const uint8_t* data = static_cast<const uint8_t*>(meta.data()) + offset;
        
        // Read ID
        uint32_t id_len = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        std::string id(reinterpret_cast<const char*>(data), id_len);
        data += id_len;
        
        // Read text
        uint32_t text_len = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        std::string text(reinterpret_cast<const char*>(data), text_len);
        data += text_len;
        
        // Read metadata JSON
        uint32_t meta_len = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        std::string meta_json(reinterpret_cast<const char*>(data), meta_len);
        
        std::cout << "  Doc " << doc_idx << ":\n";
        std::cout << "    ID: " << id << "\n";
        std::cout << "    Text length: " << text_len << " chars\n";
        std::cout << "    Text preview: " << text.substr(0, 50) << "...\n";
        std::cout << "    Metadata size: " << meta_len << " bytes\n";
    }
    
    test_pass("Metadata structure verified");
    return true;
}

int main(int argc, char* argv[]) {
    if (argc != 2) {
        std::cerr << "Usage: " << argv[0] << " <bundle-directory>\n";
        return 1;
    }
    
    std::string bundle_path = argv[1];
    if (!fs::exists(bundle_path) || !fs::is_directory(bundle_path)) {
        std::cerr << "Error: " << bundle_path << " is not a valid directory\n";
        return 1;
    }
    
    std::cout << "Testing bundle at: " << bundle_path << "\n";
    std::cout << "=====================================\n";
    
    // Load manifest to get expected values
    std::ifstream manifest_file(bundle_path + "/manifest.json");
    if (!manifest_file) {
        std::cerr << "Failed to open manifest.json\n";
        return 1;
    }
    
    std::string manifest_json((std::istreambuf_iterator<char>(manifest_file)),
                              std::istreambuf_iterator<char>());
    
    simdjson::ondemand::parser parser;
    simdjson::padded_string padded(manifest_json);
    simdjson::ondemand::document manifest;
    
    if (parser.iterate(padded).get(manifest)) {
        std::cerr << "Failed to parse manifest\n";
        return 1;
    }
    
    uint64_t num_docs, dim;
    manifest["num_docs"].get_uint64().get(num_docs);
    manifest["dim"].get_uint64().get(dim);
    
    // Run tests
    bool all_passed = true;
    
    all_passed &= test_manifest(bundle_path);
    all_passed &= test_vectors(bundle_path, num_docs, dim);
    all_passed &= test_doclengths(bundle_path, num_docs);
    all_passed &= test_bm25_index(bundle_path);
    all_passed &= test_metadata(bundle_path, num_docs);
    
    std::cout << "\n=====================================\n";
    if (all_passed) {
        std::cout << GREEN << "✅ All tests passed!" << RESET << "\n";
        return 0;
    } else {
        std::cout << RED << "❌ Some tests failed" << RESET << "\n";
        return 1;
    }
}