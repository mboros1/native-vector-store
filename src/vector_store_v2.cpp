#include "vector_store_v2.h"
#include "simple_tokenizer.h"
#include <fstream>
#include <algorithm>
#include <cmath>
#include <iostream>
#include <queue>
#include <algorithm>

namespace nvs {

// MMapFile implementation
VectorStoreV2::MMapFile::MMapFile(MMapFile&& other) noexcept
    : data_(other.data_), size_(other.size_), fd_(other.fd_) {
    other.data_ = nullptr;
    other.size_ = 0;
    other.fd_ = -1;
}

VectorStoreV2::MMapFile& VectorStoreV2::MMapFile::operator=(MMapFile&& other) noexcept {
    if (this != &other) {
        close();
        data_ = other.data_;
        size_ = other.size_;
        fd_ = other.fd_;
        other.data_ = nullptr;
        other.size_ = 0;
        other.fd_ = -1;
    }
    return *this;
}

bool VectorStoreV2::MMapFile::open(const std::string& path) {
    close();
    
    fd_ = ::open(path.c_str(), O_RDONLY);
    if (fd_ < 0) {
        std::cerr << "Failed to open file: " << path << "\n";
        return false;
    }
    
    // Get file size
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
        return true;  // Empty file is OK
    }
    
    // Memory map the file
    data_ = mmap(nullptr, size_, PROT_READ, MAP_PRIVATE, fd_, 0);
    if (data_ == MAP_FAILED) {
        data_ = nullptr;
        ::close(fd_);
        fd_ = -1;
        std::cerr << "Failed to mmap file: " << path << "\n";
        return false;
    }
    
    // Advise kernel about access pattern
    madvise(data_, size_, MADV_SEQUENTIAL);
    
    return true;
}

void VectorStoreV2::MMapFile::close() {
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

// VectorStoreV2 implementation
VectorStoreV2::VectorStoreV2(const std::string& bundle_path) {
    open(bundle_path);
}

bool VectorStoreV2::open(const std::string& bundle_path) {
    if (is_open_) {
        close();
    }
    
    bundle_path_ = bundle_path;
    
    // Load manifest
    if (!load_manifest()) {
        std::cerr << "Failed to load manifest\n";
        return false;
    }
    
    // Open all data files
    if (!open_data_files()) {
        std::cerr << "Failed to open data files\n";
        return false;
    }
    
    // Load term dictionary
    if (!load_term_dictionary()) {
        std::cerr << "Failed to load term dictionary\n";
        return false;
    }
    
    // No longer pre-allocating scores buffer - using per-thread heaps
    
    is_open_ = true;
    return true;
}

void VectorStoreV2::close() {
    vectors_file_.close();
    doclen_file_.close();
    lexicon_file_.close();
    postings_file_.close();
    terms_file_.close();
    meta_idx_file_.close();
    meta_file_.close();
    
    vectors_ = nullptr;
    doclen_ = nullptr;
    meta_idx_entries_ = nullptr;
    lexicon_ = nullptr;
    
    term_to_id_.clear();
    
    is_open_ = false;
}

bool VectorStoreV2::load_manifest() {
    std::ifstream file(bundle_path_ + "/manifest.json");
    if (!file) {
        return false;
    }
    
    std::string json_str((std::istreambuf_iterator<char>(file)),
                        std::istreambuf_iterator<char>());
    
    simdjson::ondemand::parser parser;
    simdjson::padded_string padded(json_str);
    simdjson::ondemand::document doc;
    
    auto error = parser.iterate(padded).get(doc);
    if (error) return false;
    
    // Parse manifest fields
    std::string_view format;
    if (doc["format"].get_string().get(format)) return false;
    info_.format = std::string(format);
    
    uint64_t num_docs, dim;
    if (doc["num_docs"].get_uint64().get(num_docs)) return false;
    if (doc["dim"].get_uint64().get(dim)) return false;
    info_.num_docs = num_docs;
    info_.dim = dim;
    
    simdjson::ondemand::object embedding;
    if (doc["embedding"].get_object().get(embedding)) return false;
    
    std::string_view model;
    if (embedding["model"].get_string().get(model)) return false;
    info_.embedding_model = std::string(model);
    
    std::string_view dtype;
    if (embedding["dtype"].get_string().get(dtype)) return false;
    info_.dtype = std::string(dtype);
    
    simdjson::ondemand::object bm25;
    if (doc["bm25"].get_object().get(bm25)) return false;
    
    if (bm25["avgdl"].get_double().get(info_.bm25_avgdl)) return false;
    if (bm25["k1"].get_double().get(info_.bm25_k1)) return false;
    if (bm25["b"].get_double().get(info_.bm25_b)) return false;
    
    return true;
}

bool VectorStoreV2::open_data_files() {
    // Open vectors
    std::string vectors_path = bundle_path_ + "/" + 
        (info_.dtype == "f16" ? "vectors.f16" : "vectors.f32");
    if (!vectors_file_.open(vectors_path)) {
        return false;
    }
    vectors_ = vectors_file_.as<float>();
    
    // Open document lengths
    if (!doclen_file_.open(bundle_path_ + "/doclen.u32")) {
        return false;
    }
    doclen_ = doclen_file_.as<uint32_t>();
    
    // Open lexicon
    if (!lexicon_file_.open(bundle_path_ + "/lexicon.bin")) {
        return false;
    }
    lexicon_ = lexicon_file_.as<LexiconEntry>();
    
    // Open postings
    if (!postings_file_.open(bundle_path_ + "/postings.bin")) {
        return false;
    }
    
    // Open metadata index (block-based)
    if (!meta_idx_file_.open(bundle_path_ + "/meta.idx")) {
        return false;
    }
    meta_idx_entries_ = meta_idx_file_.as<MetaIdxEntry>();
    
    // Open metadata blocks
    if (!meta_file_.open(bundle_path_ + "/meta.blocks")) {
        return false;
    }
    // Parse block header
    if (meta_file_.size() < sizeof(uint32_t)) return false;
    const uint8_t* base = static_cast<const uint8_t*>(meta_file_.data());
    meta_block_count_ = *reinterpret_cast<const uint32_t*>(base);
    // Each block header is 4 u32 = 16 bytes
    size_t header_size = sizeof(uint32_t) + static_cast<size_t>(meta_block_count_) * 16;
    // Derive block size from file size and count (assumes fixed-size blocks padded to equal size)
    if (meta_block_count_ == 0) return false;
    size_t remaining = meta_file_.size() > header_size ? (meta_file_.size() - header_size) : 0;
    if (remaining == 0) return false;
    meta_block_size_ = static_cast<uint32_t>(remaining / meta_block_count_);
    
    return true;
}

bool VectorStoreV2::load_term_dictionary() {
    if (!terms_file_.open(bundle_path_ + "/terms.dict")) {
        return false;
    }
    
    // Handle empty dictionary
    if (terms_file_.size() == 0) {
        return true;
    }
    
    const uint8_t* data = static_cast<const uint8_t*>(terms_file_.data());
    const uint8_t* end = data + terms_file_.size();
    
    size_t term_id = 0;
    while (data < end) {
        // Read term length
        if (data + sizeof(uint32_t) > end) break;
        uint32_t term_len = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        
        // Read term
        if (data + term_len > end) break;
        std::string term(reinterpret_cast<const char*>(data), term_len);
        data += term_len;
        
        term_to_id_[term] = term_id++;
    }
    
    return true;
}

std::vector<VectorStoreV2::SearchResult> 
VectorStoreV2::search(const float* query, size_t k) const {
    if (!is_open_ || k == 0 || !query) return {};
    
    const size_t n = info_.num_docs;
    if (n == 0) return {};  // No documents to search
    
    const size_t dim = info_.dim;
    
    // Calculate aligned row size
    size_t row_size = dim * sizeof(float);
    size_t aligned_row_size = ((row_size + 63) / 64) * 64;
    
    k = std::min(k, n);  // Ensure k doesn't exceed document count
    
    // Use per-thread heaps to avoid shared memory contention
    const int num_threads = omp_get_max_threads();
    std::vector<TopK> thread_heaps;
    thread_heaps.reserve(num_threads);
    for (int i = 0; i < num_threads; ++i) {
        thread_heaps.emplace_back(k);  // in-place construction
    }
    
    std::vector<std::pair<float, size_t>> final_results;
    
    #pragma omp parallel
    {
        const int tid = omp_get_thread_num();
        TopK& local_heap = thread_heaps[tid];
        
        #pragma omp for schedule(static)
        for (int i = 0; i < static_cast<int>(n); ++i) {
            const float* vec = reinterpret_cast<const float*>(
                reinterpret_cast<const char*>(vectors_) + i * aligned_row_size
            );
            
            float score = 0.0f;
            #pragma omp simd reduction(+:score)
            for (size_t j = 0; j < dim; ++j) {
                score += query[j] * vec[j];
            }
            
            local_heap.push(score, i);
        }
        
        #pragma omp barrier
        #pragma omp single
        {
            // Merge all thread-local heaps
            TopK final_heap(k);
            for (auto& th : thread_heaps) {
                final_heap.merge(th);
            }
            final_results = std::move(final_heap.heap);
        }
    }
    
    // Sort by score (highest first)
    std::sort(final_results.begin(), final_results.end(), 
              [](const auto& a, const auto& b) { return a.first > b.first; });
    
    // Convert to SearchResult format
    std::vector<SearchResult> results;
    results.reserve(final_results.size());
    
    for (const auto& [score, doc_id] : final_results) {
        SearchResult result;
        result.doc_id = doc_id;
        result.score = score;
        get_document_metadata(doc_id, result.id, result.text, result.metadata_json);
        results.push_back(std::move(result));
    }
    
    return results;
}

std::vector<VectorStoreV2::SearchResult>
VectorStoreV2::search_bm25(const std::vector<std::string>& query_terms, size_t k) const {
    if (!is_open_ || k == 0) return {};
    
    // Tokenize query terms
    SimpleTokenizer tokenizer;
    std::unordered_map<std::string, int> query_tf;
    for (const auto& term : query_terms) {
        auto tokens = tokenizer.split(term);
        for (const auto& token : tokens) {
            query_tf[token]++;
        }
    }
    
    const size_t n = info_.num_docs;
    k = std::min(k, n);
    
    // Use a single heap to track top-k documents as we score them
    // This avoids allocating a full scores array
    using ScoreDoc = std::pair<double, size_t>;
    std::priority_queue<ScoreDoc, std::vector<ScoreDoc>, std::greater<ScoreDoc>> heap;
    std::unordered_map<size_t, double> doc_scores;  // Only track docs that have matches
    
    // Score documents by iterating through posting lists
    for (const auto& [term, qtf] : query_tf) {
        auto it = term_to_id_.find(term);
        if (it == term_to_id_.end()) continue;
        
        size_t term_id = it->second;
        const LexiconEntry& lex = lexicon_[term_id];
        
        // Calculate IDF once per term
        const size_t N = info_.num_docs;
        double idf = std::log((N - lex.df + 0.5) / (lex.df + 0.5));
        
        // Decode and score posting list
        const uint8_t* data = static_cast<const uint8_t*>(postings_file_.data()) + lex.offset;
        uint32_t prev_doc = 0;
        
        for (size_t i = 0; i < lex.length; ++i) {
            uint32_t delta = *reinterpret_cast<const uint32_t*>(data);
            data += sizeof(uint32_t);
            
            uint32_t tf = *reinterpret_cast<const uint32_t*>(data);
            data += sizeof(uint32_t);
            
            uint32_t doc_id = prev_doc + delta;
            
            // BM25 scoring inline
            double doc_len = doclen_[doc_id];
            double tf_component = (tf * (info_.bm25_k1 + 1)) / 
                                 (tf + info_.bm25_k1 * (1 - info_.bm25_b + info_.bm25_b * doc_len / info_.bm25_avgdl));
            doc_scores[doc_id] += idf * tf_component * qtf;
            
            prev_doc = doc_id;
        }
    }
    
    // Now build heap from scored documents only
    for (const auto& [doc_id, score] : doc_scores) {
        if (heap.size() < k) {
            heap.push({score, doc_id});
        } else if (score > heap.top().first) {
            heap.pop();
            heap.push({score, doc_id});
        }
    }
    
    // Extract results
    std::vector<SearchResult> results;
    results.reserve(heap.size());
    
    while (!heap.empty()) {
        auto [score, doc_id] = heap.top();
        heap.pop();
        
        SearchResult result;
        result.doc_id = doc_id;
        result.score = score;
        get_document_metadata(doc_id, result.id, result.text, result.metadata_json);
        
        results.push_back(std::move(result));
    }
    
    std::reverse(results.begin(), results.end());
    return results;
}

std::vector<VectorStoreV2::SearchResult>
VectorStoreV2::search_hybrid(const float* query_vector,
                            const std::vector<std::string>& query_terms,
                            size_t k,
                            double vector_weight) const {
    if (!is_open_) return {};
    
    // Get vector and BM25 results
    auto vector_results = search(query_vector, k * 2);
    auto bm25_results = search_bm25(query_terms, k * 2);
    
    // Reciprocal Rank Fusion
    std::unordered_map<size_t, double> combined_scores;
    
    // Add vector scores
    for (size_t i = 0; i < vector_results.size(); ++i) {
        size_t doc_id = vector_results[i].doc_id;
        double rrf_score = 1.0 / (60.0 + i + 1);  // RRF constant = 60
        combined_scores[doc_id] += vector_weight * rrf_score;
    }
    
    // Add BM25 scores
    double bm25_weight = 1.0 - vector_weight;
    for (size_t i = 0; i < bm25_results.size(); ++i) {
        size_t doc_id = bm25_results[i].doc_id;
        double rrf_score = 1.0 / (60.0 + i + 1);
        combined_scores[doc_id] += bm25_weight * rrf_score;
    }
    
    // Sort by combined score
    std::vector<std::pair<double, size_t>> final_scores;
    for (const auto& [doc_id, score] : combined_scores) {
        final_scores.emplace_back(score, doc_id);
    }
    
    std::sort(final_scores.begin(), final_scores.end(), std::greater<>());
    
    // Build final results
    size_t actual_k = std::min(k, final_scores.size());
    std::vector<SearchResult> results;
    results.reserve(actual_k);
    
    for (size_t i = 0; i < actual_k; ++i) {
        SearchResult result;
        result.doc_id = final_scores[i].second;
        result.score = final_scores[i].first;
        get_document_metadata(result.doc_id, result.id, result.text, result.metadata_json);
        results.push_back(std::move(result));
    }
    
    return results;
}

bool VectorStoreV2::get_document(size_t doc_id, SearchResult& result) const {
    if (!is_open_ || doc_id >= info_.num_docs) {
        return false;
    }
    
    result.doc_id = doc_id;
    result.score = 0.0;
    return get_document_metadata(doc_id, result.id, result.text, result.metadata_json);
}

bool VectorStoreV2::get_document_metadata(size_t doc_id, 
                                         std::string& id, 
                                         std::string& text,
                                         std::string& metadata) const {
    if (doc_id >= info_.num_docs) return false;
    
    // Lookup index entry
    const MetaIdxEntry& idx = meta_idx_entries_[doc_id];
    // Compute start of block data region (after global header and block headers)
    const uint8_t* base = static_cast<const uint8_t*>(meta_file_.data());
    size_t blocks_header_size = sizeof(uint32_t) + static_cast<size_t>(meta_block_count_) * 16;
    const uint8_t* block0 = base + blocks_header_size;
    const uint8_t* block_begin = block0 + static_cast<size_t>(idx.block_id) * meta_block_size_;
    const uint8_t* data = block_begin + idx.offset_in_block;
    const uint8_t* end = static_cast<const uint8_t*>(meta_file_.data()) + meta_file_.size();
    
    // Read ID
    if (data + sizeof(uint32_t) > end) return false;
    uint32_t id_len = *reinterpret_cast<const uint32_t*>(data);
    data += sizeof(uint32_t);
    
    if (data + id_len > end) return false;
    id = std::string(reinterpret_cast<const char*>(data), id_len);
    data += id_len;
    
    // Read text
    if (data + sizeof(uint32_t) > end) return false;
    uint32_t text_len = *reinterpret_cast<const uint32_t*>(data);
    data += sizeof(uint32_t);
    
    if (data + text_len > end) return false;
    text = std::string(reinterpret_cast<const char*>(data), text_len);
    data += text_len;
    
    // Read metadata JSON
    if (data + sizeof(uint32_t) > end) return false;
    uint32_t meta_len = *reinterpret_cast<const uint32_t*>(data);
    data += sizeof(uint32_t);
    
    if (data + meta_len > end) return false;
    metadata = std::string(reinterpret_cast<const char*>(data), meta_len);
    
    return true;
}

double VectorStoreV2::compute_bm25_score(size_t doc_id, 
                                        const std::unordered_map<std::string, int>& query_tf) const {
    double score = 0.0;
    const double k1 = info_.bm25_k1;
    const double b = info_.bm25_b;
    const double avgdl = info_.bm25_avgdl;
    const double doc_len = doclen_[doc_id];
    const size_t N = info_.num_docs;
    
    for (const auto& [term, qtf] : query_tf) {
        auto it = term_to_id_.find(term);
        if (it == term_to_id_.end()) continue;
        
        size_t term_id = it->second;
        const LexiconEntry& lex = lexicon_[term_id];
        
        // Get term frequency in document
        auto postings = decode_posting_list(term_id);
        int doc_tf = 0;
        for (const auto& [pid, tf] : postings) {
            if (pid == doc_id) {
                doc_tf = tf;
                break;
            }
            if (pid > doc_id) break;  // Postings are sorted
        }
        
        if (doc_tf > 0) {
            // BM25 formula
            double idf = std::log((N - lex.df + 0.5) / (lex.df + 0.5));
            double tf_component = (doc_tf * (k1 + 1)) / 
                                 (doc_tf + k1 * (1 - b + b * doc_len / avgdl));
            score += idf * tf_component;
        }
    }
    
    return score;
}

float VectorStoreV2::compute_similarity(const float* a, const float* b) const {
    float score = 0.0f;
    
    #pragma omp simd reduction(+:score)
    for (size_t i = 0; i < info_.dim; ++i) {
        score += a[i] * b[i];
    }
    
    return score;
}

std::vector<std::pair<uint32_t, uint32_t>> 
VectorStoreV2::decode_posting_list(size_t term_id) const {
    const LexiconEntry& lex = lexicon_[term_id];
    std::vector<std::pair<uint32_t, uint32_t>> postings;
    postings.reserve(lex.length);
    
    const uint8_t* data = static_cast<const uint8_t*>(postings_file_.data()) + lex.offset;
    uint32_t prev_doc = 0;
    
    for (size_t i = 0; i < lex.length; ++i) {
        uint32_t delta = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        
        uint32_t tf = *reinterpret_cast<const uint32_t*>(data);
        data += sizeof(uint32_t);
        
        uint32_t doc_id = prev_doc + delta;
        postings.emplace_back(doc_id, tf);
        prev_doc = doc_id;
    }
    
    return postings;
}

} // namespace nvs

// Unit tests - only compiled when tests are enabled
#if 0 // NVS_ENABLE_INLINE_TESTS (disabled; use src/test suites)
#include "doctest/doctest.h"
#include <random>
#include <cmath>
#include <filesystem>
#include <thread>
#include <chrono>

// Helper function to create normalized random vectors
static std::vector<float> create_random_vector(size_t dim, std::mt19937& gen) {
    std::uniform_real_distribution<float> dist(-1.0f, 1.0f);
    std::vector<float> vec(dim);
    float norm = 0.0f;
    for (size_t i = 0; i < dim; ++i) {
        vec[i] = dist(gen);
        norm += vec[i] * vec[i];
    }
    norm = std::sqrt(norm);
    for (auto& v : vec) {
        v /= norm;
    }
    return vec;
}

TEST_CASE("VectorStoreV2 MMapFile") {
    using namespace nvs;
    namespace fs = std::filesystem;
    
    SUBCASE("Basic file operations") {
        auto temp_dir = fs::temp_directory_path();
        auto temp_file = temp_dir / "test_mmap.bin";
        
        // Create test file with known content
        std::ofstream out(temp_file, std::ios::binary);
        std::string data = "Hello, MMap! This is test data.";
        out.write(data.c_str(), data.size());
        out.close();
        
        VectorStoreV2::MMapFile mmap;
        
        // Test opening file
        CHECK(mmap.open(temp_file.string()));
        CHECK(mmap.is_open());
        CHECK(mmap.size() == data.size());
        CHECK(mmap.data() != nullptr);
        
        // Verify content matches
        std::string read_data(mmap.data(), mmap.size());
        CHECK(read_data == data);
        
        // Test close
        mmap.close();
        CHECK_FALSE(mmap.is_open());
        CHECK(mmap.data() == nullptr);
        CHECK(mmap.size() == 0);
        
        // Cleanup
        fs::remove(temp_file);
    }
    
    SUBCASE("Error handling") {
        VectorStoreV2::MMapFile mmap;
        
        // Non-existent file should fail
        CHECK_FALSE(mmap.open("/non/existent/file.bin"));
        CHECK_FALSE(mmap.is_open());
        
        // Empty path should fail
        CHECK_FALSE(mmap.open(""));
        CHECK_FALSE(mmap.is_open());
    }
    
    SUBCASE("Move semantics") {
        auto temp_dir = fs::temp_directory_path();
        auto temp_file = temp_dir / "test_move.bin";
        
        std::ofstream out(temp_file, std::ios::binary);
        out << "test data for move";
        out.close();
        
        VectorStoreV2::MMapFile mmap1;
        mmap1.open(temp_file.string());
        size_t original_size = mmap1.size();
        auto original_data = mmap1.data();
        
        // Move constructor
        VectorStoreV2::MMapFile mmap2(std::move(mmap1));
        CHECK(mmap2.size() == original_size);
        CHECK(mmap2.data() == original_data);
        CHECK(mmap2.is_open());
        CHECK_FALSE(mmap1.is_open());
        CHECK(mmap1.data() == nullptr);
        
        // Move assignment
        VectorStoreV2::MMapFile mmap3;
        mmap3 = std::move(mmap2);
        CHECK(mmap3.size() == original_size);
        CHECK(mmap3.data() == original_data);
        CHECK(mmap3.is_open());
        CHECK_FALSE(mmap2.is_open());
        
        // Cleanup
        fs::remove(temp_file);
    }
}

TEST_CASE("VectorStoreV2 similarity computation") {
    using namespace nvs;
    
    SUBCASE("Basic cosine similarity") {
        VectorStoreV2 store;
        store.dimensions_ = 3;
        
        // Test vectors
        float vec1[] = {1.0f, 0.0f, 0.0f};
        float vec2[] = {1.0f, 0.0f, 0.0f};
        float vec3[] = {0.0f, 1.0f, 0.0f};
        float vec4[] = {-1.0f, 0.0f, 0.0f};
        
        // Identical vectors -> similarity = 1.0
        CHECK(store.compute_similarity(vec1, vec2) == doctest::Approx(1.0f));
        
        // Orthogonal vectors -> similarity = 0.0
        CHECK(store.compute_similarity(vec1, vec3) == doctest::Approx(0.0f));
        
        // Opposite vectors -> similarity = -1.0
        CHECK(store.compute_similarity(vec1, vec4) == doctest::Approx(-1.0f));
    }
    
    SUBCASE("High-dimensional similarity") {
        VectorStoreV2 store;
        const size_t dim = 1536; // Common embedding dimension
        store.dimensions_ = dim;
        
        // Create random vectors
        std::mt19937 gen(42);
        auto vec1 = create_random_vector(dim, gen);
        auto vec2 = create_random_vector(dim, gen);
        
        // Self-similarity should be 1.0 (normalized vectors)
        float self_sim = store.compute_similarity(vec1.data(), vec1.data());
        CHECK(self_sim == doctest::Approx(1.0f).epsilon(0.001));
        
        // Different vectors should have similarity in [-1, 1]
        float cross_sim = store.compute_similarity(vec1.data(), vec2.data());
        CHECK(cross_sim >= -1.0f);
        CHECK(cross_sim <= 1.0f);
    }
}

TEST_CASE("VectorStoreV2 BM25 scoring") {
    using namespace nvs;
    
    SUBCASE("Basic BM25 score") {
        VectorStoreV2 store;
        
        // Set up test parameters
        store.num_docs_ = 100;
        store.bm25_avgdl_ = 50.0;
        store.bm25_k1_ = 1.2;
        store.bm25_b_ = 0.75;
        
        // Create mock document lengths
        store.doc_lengths_ = new uint32_t[1]{60};  // Document 0 has 60 tokens
        
        // Create query term frequencies
        std::unordered_map<std::string, int> query_tf;
        query_tf["test"] = 1;
        
        // Mock term statistics
        store.term_doc_freqs_["test"] = 10;  // Term appears in 10 docs
        
        // Calculate BM25 score
        double score = store.compute_bm25_score(0, query_tf);
        
        // Score should be positive for matching terms
        CHECK(score > 0.0);
        
        // Cleanup
        delete[] store.doc_lengths_;
        store.doc_lengths_ = nullptr;
    }
    
    SUBCASE("BM25 with no matching terms") {
        VectorStoreV2 store;
        
        store.num_docs_ = 100;
        store.bm25_avgdl_ = 50.0;
        store.bm25_k1_ = 1.2;
        store.bm25_b_ = 0.75;
        
        store.doc_lengths_ = new uint32_t[1]{60};
        
        // Query with non-existent term
        std::unordered_map<std::string, int> query_tf;
        query_tf["nonexistent"] = 1;
        
        double score = store.compute_bm25_score(0, query_tf);
        
        // Score should be 0 for non-matching terms
        CHECK(score == 0.0);
        
        // Cleanup
        delete[] store.doc_lengths_;
        store.doc_lengths_ = nullptr;
    }
}

TEST_CASE("VectorStoreV2 bundle loading") {
    using namespace nvs;
    namespace fs = std::filesystem;
    
    // Create a minimal test bundle
    auto temp_dir = fs::temp_directory_path() / "nvs_test_bundle";
    fs::create_directories(temp_dir);
    
    SUBCASE("Load non-existent bundle") {
        VectorStoreV2 store;
        bool result = store.open("/non/existent/bundle");
        
        CHECK_FALSE(result);
        CHECK_FALSE(store.is_open());
    }
    
    SUBCASE("Create and load minimal bundle") {
        // Create manifest
        std::ofstream manifest(temp_dir / "manifest.json");
        manifest << R"({
            "format": "nvs.v1",
            "num_docs": 2,
            "dim": 3,
            "embedding": {
                "model": "test",
                "dtype": "f32"
            },
            "bm25": {
                "avgdl": 10.0,
                "k1": 1.2,
                "b": 0.75
            },
            "files": {
                "vectors": {"path": "vectors.f32", "dtype": "f32", "rows": 2, "cols": 3},
                "doclen": {"path": "doclen.u32", "dtype": "u32", "rows": 2},
                "lexicon": {"path": "lexicon.bin"},
                "postings": {"path": "postings.bin"},
                "terms": {"path": "terms.dict"},
                "meta_idx": {"path": "meta.idx"},
                "meta": {"path": "meta.bin"}
            }
        })";
        manifest.close();
        
        // Create vectors file (2 vectors of dimension 3)
        std::ofstream vectors(temp_dir / "vectors.f32", std::ios::binary);
        float vec_data[] = {1.0f, 0.0f, 0.0f, 0.0f, 1.0f, 0.0f};
        vectors.write(reinterpret_cast<char*>(vec_data), sizeof(vec_data));
        vectors.close();
        
        // Create doclen file
        std::ofstream doclen(temp_dir / "doclen.u32", std::ios::binary);
        uint32_t lengths[] = {10, 12};
        doclen.write(reinterpret_cast<char*>(lengths), sizeof(lengths));
        doclen.close();
        
        // Create empty lexicon
        std::ofstream lexicon(temp_dir / "lexicon.bin", std::ios::binary);
        uint32_t zero = 0;
        lexicon.write(reinterpret_cast<char*>(&zero), sizeof(zero));
        lexicon.close();
        
        // Create empty postings
        std::ofstream postings(temp_dir / "postings.bin", std::ios::binary);
        postings.close();
        
        // Create empty terms
        std::ofstream terms(temp_dir / "terms.dict", std::ios::binary);
        terms.close();
        
        // Create metadata index
        std::ofstream meta_idx(temp_dir / "meta.idx", std::ios::binary);
        struct {
            uint64_t offset;
            uint32_t size;
            uint32_t padding;
        } idx_entries[] = {
            {0, 10, 0},
            {10, 12, 0}
        };
        meta_idx.write(reinterpret_cast<char*>(idx_entries), sizeof(idx_entries));
        meta_idx.close();
        
        // Create metadata
        std::ofstream meta(temp_dir / "meta.bin", std::ios::binary);
        meta << "doc1_meta_doc2_meta_";
        meta.close();
        
        // Try to load the bundle
        VectorStoreV2 store;
        bool result = store.open(temp_dir.string());
        
        CHECK(result);
        CHECK(store.is_open());
        CHECK(store.num_docs() == 2);
        CHECK(store.dimensions() == 3);
        
        store.close();
        CHECK_FALSE(store.is_open());
    }
    
    // Cleanup
    fs::remove_all(temp_dir);
}

TEST_CASE("VectorStoreV2 search functionality") {
    using namespace nvs;
    namespace fs = std::filesystem;
    
    SUBCASE("Search on closed store") {
        VectorStoreV2 store;
        float query[] = {1.0f, 0.0f, 0.0f};
        
        auto results = store.search(query, 10);
        
        CHECK(results.empty());
    }
    
    SUBCASE("Search with invalid k") {
        VectorStoreV2 store;
        // Even with closed store, k validation should work
        float query[] = {1.0f};
        
        auto results = store.search(query, 0);
        CHECK(results.empty());
    }
}

TEST_CASE("VectorStoreV2 hybrid search") {
    using namespace nvs;
    
    SUBCASE("Empty query terms") {
        VectorStoreV2 store;
        float query_vec[] = {1.0f, 0.0f, 0.0f};
        std::vector<std::string> query_terms;
        
        auto results = store.search_hybrid(query_vec, query_terms, 10, 0.5);
        
        // Should fall back to vector-only search
        CHECK(results.empty());  // Store is closed, so no results
    }
    
    SUBCASE("RRF weight validation") {
        VectorStoreV2 store;
        float query_vec[] = {1.0f};
        std::vector<std::string> terms = {"test"};
        
        // Weight should be clamped to [0, 1]
        auto results1 = store.search_hybrid(query_vec, terms, 10, -0.5);
        auto results2 = store.search_hybrid(query_vec, terms, 10, 1.5);
        
        // Both should complete without crash (even if empty due to closed store)
        CHECK(results1.empty());
        CHECK(results2.empty());
    }
}

TEST_CASE("VectorStoreV2 document retrieval") {
    using namespace nvs;
    
    SUBCASE("Get document from closed store") {
        VectorStoreV2 store;
        VectorStoreV2::SearchResult result;
        
        bool success = store.get_document(0, result);
        
        CHECK_FALSE(success);
    }
    
    SUBCASE("Get document with out-of-bounds ID") {
        VectorStoreV2 store;
        store.num_docs_ = 10;  // Simulate 10 documents
        
        VectorStoreV2::SearchResult result;
        bool success = store.get_document(15, result);
        
        CHECK_FALSE(success);
    }
}

TEST_CASE("VectorStoreV2 posting list decoding") {
    using namespace nvs;
    
    SUBCASE("Decode empty posting list") {
        VectorStoreV2 store;
        store.lexicon_ = new uint32_t[5]{0, 0, 0, 0, 0};  // Empty lexicon entry
        
        auto postings = store.decode_posting_list(0);
        
        CHECK(postings.empty());
        
        delete[] reinterpret_cast<uint32_t*>(store.lexicon_);
        store.lexicon_ = nullptr;
    }
    
    SUBCASE("Decode delta-encoded postings") {
        VectorStoreV2 store;
        
        // Create mock postings data
        // Format: [delta1, tf1, delta2, tf2, ...]
        uint32_t postings_data[] = {
            0, 2,  // doc 0, term freq 2
            3, 1,  // doc 3 (0+3), term freq 1
            2, 3   // doc 5 (3+2), term freq 3
        };
        
        store.postings_ = reinterpret_cast<uint8_t*>(postings_data);
        
        // Create lexicon entry pointing to this data
        // Format: [term_id, doc_freq, postings_offset, postings_size]
        uint32_t lexicon_entry[] = {0, 3, 0, 0, sizeof(postings_data)};
        store.lexicon_ = reinterpret_cast<uint8_t*>(lexicon_entry);
        
        auto decoded = store.decode_posting_list(0);
        
        CHECK(decoded.size() == 3);
        CHECK(decoded[0].first == 0);
        CHECK(decoded[0].second == 2);
        CHECK(decoded[1].first == 3);
        CHECK(decoded[1].second == 1);
        CHECK(decoded[2].first == 5);
        CHECK(decoded[2].second == 3);
        
        store.postings_ = nullptr;
        store.lexicon_ = nullptr;
    }
}

#endif

// New inline doctest suite (enabled with NVS_ENABLE_INLINE_TESTS)
#ifdef NVS_ENABLE_INLINE_TESTS
#include "doctest/doctest.h"
#include <vector>
#include <string>
#include <cstdlib>
#include <algorithm>

static std::string nvs_resolve_test_bundle() {
    if (const char* env = std::getenv("NVS_TEST_BUNDLE")) {
        return std::string(env);
    }
    return std::string("test-bundle");
}

TEST_CASE("VectorStoreV2 default state") {
    nvs::VectorStoreV2 store;
    CHECK(store.is_open() == false);
    CHECK(store.size() == 0);
    CHECK(store.dimensions() == 0);

    float dummy = 0.0f;
    auto results = store.search(&dummy, 5);
    CHECK(results.empty());
}

TEST_CASE("VectorStoreV2 open invalid path") {
    nvs::VectorStoreV2 store;
    CHECK_FALSE(store.open("__nonexistent_bundle_dir__"));
    CHECK_FALSE(store.is_open());
}

#ifdef NVS_TEST_V2_BUNDLE
TEST_CASE("VectorStoreV2 bundle integration") {
    using nvs::VectorStoreV2;
    VectorStoreV2 store;
    std::string bundle = nvs_resolve_test_bundle();

    SUBCASE("open and basic properties") {
        if (!store.open(bundle)) {
            MESSAGE("Bundle not found at '" << bundle << "' — skipping");
            return;
        }
        CHECK(store.is_open());
        CHECK(store.size() > 0);
        CHECK(store.dimensions() > 0);
    }

    SUBCASE("get first document") {
        if (!store.open(bundle)) {
            MESSAGE("Bundle not found — skipping");
            return;
        }
        VectorStoreV2::SearchResult doc;
        CHECK(store.get_document(0, doc));
        CHECK(doc.id.size() > 0);
        CHECK(doc.metadata_json.size() >= 0);
    }

    SUBCASE("vector search properties and stability") {
        if (!store.open(bundle)) {
            MESSAGE("Bundle not found — skipping");
            return;
        }
        const size_t dim = store.dimensions();
        std::vector<float> q(dim, 0.0f);
        if (dim > 0) q[0] = 1.0f;

        auto r0 = store.search(q.data(), 0);
        CHECK(r0.empty());

        auto r_over = store.search(q.data(), store.size() + 10);
        CHECK(r_over.size() == store.size());

        auto r1 = store.search(q.data(), std::min<size_t>(store.size(), 10));
        if (!r1.empty()) {
            for (size_t i = 1; i < r1.size(); ++i) {
                CHECK(r1[i-1].score >= r1[i].score);
            }
            auto r1b = store.search(q.data(), r1.size());
            CHECK(r1b.size() == r1.size());
            for (size_t i = 0; i < r1.size(); ++i) {
                CHECK(r1[i].doc_id == r1b[i].doc_id);
                CHECK(r1[i].score == doctest::Approx(r1b[i].score));
            }
        }
    }

    SUBCASE("bm25 edge cases and hybrid extremes") {
        if (!store.open(bundle)) {
            MESSAGE("Bundle not found — skipping");
            return;
        }
        std::vector<std::string> empty_terms;
        auto bm_empty = store.search_bm25(empty_terms, 5);
        CHECK(bm_empty.empty());

        std::vector<std::string> nonsense = {"zzzzzzzzzz"};
        auto bm_none = store.search_bm25(nonsense, 5);
        CHECK(bm_none.size() <= 5);

        const size_t dim = store.dimensions();
        std::vector<float> q(dim, 0.0f);
        if (dim > 0) q[0] = 1.0f;

        auto v = store.search(q.data(), 5);
        auto h_vec = store.search_hybrid(q.data(), {"__unused__"}, 5, 1.0);
        if (!v.empty() && !h_vec.empty()) {
            size_t m = std::min(v.size(), h_vec.size());
            for (size_t i = 0; i < m; ++i) {
                CHECK(v[i].doc_id == h_vec[i].doc_id);
            }
        }

        auto bm = store.search_bm25({"document"}, 5);
        auto h_bm = store.search_hybrid(q.data(), {"document"}, 5, 0.0);
        if (!bm.empty() && !h_bm.empty()) {
            size_t m = std::min(bm.size(), h_bm.size());
            for (size_t i = 0; i < m; ++i) {
                CHECK(bm[i].doc_id == h_bm[i].doc_id);
            }
        }
    }

    SUBCASE("reopen equivalence") {
        if (!store.open(bundle)) {
            MESSAGE("Bundle not found — skipping");
            return;
        }
        const size_t dim = store.dimensions();
        std::vector<float> q(dim, 0.0f);
        if (dim > 0) q[0] = 1.0f;
        auto before = store.search(q.data(), std::min<size_t>(store.size(), 10));
        store.close();
        REQUIRE(store.open(bundle));
        auto after = store.search(q.data(), before.size());
        CHECK(after.size() == before.size());
        for (size_t i = 0; i < before.size(); ++i) {
            CHECK(before[i].doc_id == after[i].doc_id);
        }
    }
}
#ifdef NVS_TEST_V2_CONCURRENCY
TEST_CASE("VectorStoreV2 basic concurrency smoke") {
    using nvs::VectorStoreV2;
    VectorStoreV2 store;
    std::string bundle = nvs_resolve_test_bundle();
    if (!store.open(bundle)) {
        MESSAGE("Bundle not found — skipping");
        return;
    }
    const size_t dim = store.dimensions();
    std::vector<float> q(dim, 0.0f);
    if (dim > 0) q[0] = 1.0f;

    std::atomic<int> ok{0};
    std::thread t1([&]{ for (int i=0;i<10;++i){ auto r=store.search(q.data(), 5); if (r.size()<=5) ++ok; }});
    std::thread t2([&]{ for (int i=0;i<10;++i){ auto r=store.search(q.data(), 5); if (r.size()<=5) ++ok; }});
    t1.join(); t2.join();
    CHECK(ok == 20);
}
#endif // NVS_TEST_V2_CONCURRENCY
#endif // NVS_TEST_V2_BUNDLE
#endif // NVS_ENABLE_INLINE_TESTS
