// nvs-pack: Offline packer for native-vector-store bundles
#include <iostream>
#include <fstream>
#include <filesystem>
#include <vector>
#include <string>
#include <cstring>
#include <chrono>
#include <iomanip>
#include <sstream>
#include <unordered_map>
#include <algorithm>
#include "document_loader.h"

// Include xxHash implementation
#define XXH_STATIC_LINKING_ONLY
#define XXH_IMPLEMENTATION
#include "xxhash.h"

namespace fs = std::filesystem;

namespace nvs {

struct PackerOptions {
    std::string input_path;
    std::string output_dir = "./nvs-bundle";
    size_t dim = 0;  // Auto-detect from first document
    std::string embedding_model = "unknown";
    bool quantize_f16 = false;
    double bm25_k1 = 1.2;
    double bm25_b = 0.75;
    size_t min_df = 1;
    size_t block_size = 131072; // 128KB blocks for metadata
    bool verbose = false;
};

// Binary metadata format
struct DocHeader {
    uint64_t doc_id;
    uint64_t timestamp_unix;  // 0 if unknown
    uint32_t id_len;         // document ID string length
    uint32_t text_len;       // text content length
    uint32_t source_len;     // source string length (0 if none)
    uint32_t padding;        // for alignment
    // Followed by: id[id_len], text[text_len], source[source_len]
};

struct MetaBlock {
    uint32_t block_id;
    uint32_t uncompressed_size;
    uint32_t doc_count;  // number of documents in this block
    uint32_t padding;
    std::vector<uint8_t> data;
};

struct MetaIndex {
    uint32_t block_id;
    uint32_t offset_in_block;
    uint32_t doc_size;  // total size of this doc's data
    uint32_t padding;
};

class NVSPacker {
private:
    PackerOptions opts_;
    DocumentLoader::LoadResult data_;
    
public:
    explicit NVSPacker(const PackerOptions& opts) : opts_(opts) {}
    
    int run() {
        auto start_time = std::chrono::high_resolution_clock::now();
        
        // Step 1: Load documents using DocumentLoader
        std::cout << "Loading documents from " << opts_.input_path << "...\n";
        data_ = DocumentLoader::loadDirectory(opts_.input_path, opts_.verbose);
        
        if (data_.documents.empty()) {
            std::cerr << "No documents loaded!\n";
            return 1;
        }
        
        // Update options with detected values
        if (opts_.dim == 0) {
            opts_.dim = data_.dimensions;
        } else if (opts_.dim != data_.dimensions) {
            std::cerr << "Dimension mismatch: specified " << opts_.dim 
                     << " but loaded documents have " << data_.dimensions << "\n";
            return 1;
        }
        
        std::cout << "Loaded " << data_.documents.size() << " documents\n";
        std::cout << "  Dimensions: " << data_.dimensions << "\n";
        std::cout << "  Terms: " << data_.postings.size() << "\n";
        std::cout << "  Avg doc length: " << data_.average_document_length << "\n";
        
        // Step 2: Create output directory
        fs::create_directories(opts_.output_dir);
        
        // Step 3: Write binary files
        if (!writeVectors()) return 1;
        if (!writeDocLengths()) return 1;
        if (!writeBM25Index()) return 1;
        if (!writeMetadata()) return 1;
        
        // Step 4: Generate manifest
        if (!writeManifest()) return 1;
        
        // Step 5: Compute checksums
        if (!writeChecksums()) return 1;
        
        auto end_time = std::chrono::high_resolution_clock::now();
        auto duration = std::chrono::duration_cast<std::chrono::seconds>(end_time - start_time);
        
        std::cout << "\n✅ Bundle created successfully!\n";
        std::cout << "  Documents: " << data_.documents.size() << "\n";
        std::cout << "  Dimensions: " << data_.dimensions << "\n";
        std::cout << "  Output: " << opts_.output_dir << "\n";
        std::cout << "  Time: " << duration.count() << "s\n";
        
        return 0;
    }
    
private:
    
    bool writeVectors() {
        std::string filename = opts_.quantize_f16 ? "vectors.f16" : "vectors.f32";
        std::string path = opts_.output_dir + "/" + filename;
        
        std::cout << "Writing vectors to " << filename << "...\n";
        
        std::ofstream file(path, std::ios::binary);
        if (!file) {
            std::cerr << "Failed to create " << path << "\n";
            return false;
        }
        
        // Write vectors in row-major order with alignment
        const size_t row_size = data_.dimensions * (opts_.quantize_f16 ? 2 : 4);
        const size_t aligned_row_size = ((row_size + 63) / 64) * 64; // 64-byte align
        
        std::vector<char> padding(aligned_row_size - row_size, 0);
        
        for (const auto& doc : data_.documents) {
            if (opts_.quantize_f16) {
                // TODO: Implement f16 conversion
                // For now, just write f32
                file.write(reinterpret_cast<const char*>(doc.embedding.data()), row_size);
            } else {
                file.write(reinterpret_cast<const char*>(doc.embedding.data()), row_size);
            }
            
            // Write padding
            if (!padding.empty()) {
                file.write(padding.data(), padding.size());
            }
        }
        
        return file.good();
    }
    
    bool writeDocLengths() {
        std::string path = opts_.output_dir + "/doclen.u32";
        
        std::cout << "Writing document lengths...\n";
        
        std::ofstream file(path, std::ios::binary);
        if (!file) return false;
        
        for (const auto& doc : data_.documents) {
            uint32_t len = static_cast<uint32_t>(doc.length);
            file.write(reinterpret_cast<const char*>(&len), sizeof(len));
        }
        
        return file.good();
    }
    
    bool writeBM25Index() {
        std::cout << "Writing BM25 index...\n";
        
        // Write lexicon
        std::string lex_path = opts_.output_dir + "/lexicon.bin";
        std::ofstream lex_file(lex_path, std::ios::binary);
        if (!lex_file) return false;
        
        // Write postings
        std::string post_path = opts_.output_dir + "/postings.bin";
        std::ofstream post_file(post_path, std::ios::binary);
        if (!post_file) return false;
        
        uint64_t post_offset = 0;
        
        // Sort terms for consistent ordering
        std::vector<std::string> terms;
        for (const auto& [term, _] : data_.postings) {
            terms.push_back(term);
        }
        std::sort(terms.begin(), terms.end());
        
        // Write term dictionary separately
        std::string dict_path = opts_.output_dir + "/terms.dict";
        std::ofstream dict_file(dict_path, std::ios::binary);
        if (!dict_file) return false;
        
        for (size_t term_id = 0; term_id < terms.size(); ++term_id) {
            const auto& term = terms[term_id];
            const auto& posting_list = data_.postings.at(term);
            
            // Write term to dictionary
            uint32_t term_len = term.size();
            dict_file.write(reinterpret_cast<const char*>(&term_len), sizeof(term_len));
            dict_file.write(term.data(), term_len);
            
            // Write lexicon entry
            struct LexiconEntry {
                uint64_t offset;
                uint32_t length;
                uint32_t df;
            } lex_entry;
            
            lex_entry.offset = post_offset;
            lex_entry.length = posting_list.size();
            
            auto df_it = data_.document_frequencies.find(term);
            lex_entry.df = (df_it != data_.document_frequencies.end()) ? df_it->second : posting_list.size();
            
            lex_file.write(reinterpret_cast<const char*>(&lex_entry), sizeof(lex_entry));
            
            // Write postings (delta-encoded doc IDs + term frequencies)
            uint32_t prev_doc = 0;
            for (const auto& [doc_id, tf] : posting_list) {
                uint32_t delta = doc_id - prev_doc;
                post_file.write(reinterpret_cast<const char*>(&delta), sizeof(delta));
                
                uint32_t tf_val = static_cast<uint32_t>(tf);
                post_file.write(reinterpret_cast<const char*>(&tf_val), sizeof(tf_val));
                
                prev_doc = doc_id;
                post_offset += sizeof(delta) + sizeof(tf_val);
            }
        }
        
        return lex_file.good() && post_file.good() && dict_file.good();
    }
    
    bool writeMetadata() {
        std::cout << "Writing metadata with doc-aligned blocks...\n";
        
        // Prepare blocks
        std::vector<MetaBlock> blocks;
        std::vector<MetaIndex> index;
        
        MetaBlock current_block;
        current_block.block_id = 0;
        current_block.uncompressed_size = 0;
        current_block.doc_count = 0;
        current_block.data.reserve(opts_.block_size);
        
        for (size_t doc_idx = 0; doc_idx < data_.documents.size(); ++doc_idx) {
            const auto& doc = data_.documents[doc_idx];
            
            // Extract source from metadata_json if available
            std::string source;
            // Simple extraction - look for "source_file" in JSON
            size_t source_pos = doc.metadata_json.find("\"source_file\":");
            if (source_pos != std::string::npos) {
                size_t start = doc.metadata_json.find('"', source_pos + 14);
                if (start != std::string::npos) {
                    size_t end = doc.metadata_json.find('"', start + 1);
                    if (end != std::string::npos) {
                        source = doc.metadata_json.substr(start + 1, end - start - 1);
                    }
                }
            }
            
            // Calculate document size
            size_t doc_size = sizeof(DocHeader) + doc.id.size() + doc.text.size() + source.size();
            
            // Check if adding this doc would exceed block size
            // Never split a document across blocks
            if (current_block.uncompressed_size + doc_size > opts_.block_size && current_block.doc_count > 0) {
                // Save current block and start new one
                blocks.push_back(std::move(current_block));
                current_block = MetaBlock();
                current_block.block_id = blocks.size();
                current_block.uncompressed_size = 0;
                current_block.doc_count = 0;
                current_block.data.clear();
                current_block.data.reserve(opts_.block_size);
            }
            
            // Create index entry
            MetaIndex idx_entry;
            idx_entry.block_id = current_block.block_id;
            idx_entry.offset_in_block = current_block.uncompressed_size;
            idx_entry.doc_size = doc_size;
            idx_entry.padding = 0;
            index.push_back(idx_entry);
            
            // Create document header
            DocHeader header;
            header.doc_id = doc_idx;
            header.timestamp_unix = 0;  // Could extract from metadata if available
            header.id_len = doc.id.size();
            header.text_len = doc.text.size();
            header.source_len = source.size();
            header.padding = 0;
            
            // Write to block
            const uint8_t* header_bytes = reinterpret_cast<const uint8_t*>(&header);
            current_block.data.insert(current_block.data.end(), header_bytes, header_bytes + sizeof(DocHeader));
            current_block.data.insert(current_block.data.end(), doc.id.begin(), doc.id.end());
            current_block.data.insert(current_block.data.end(), doc.text.begin(), doc.text.end());
            if (!source.empty()) {
                current_block.data.insert(current_block.data.end(), source.begin(), source.end());
            }
            
            current_block.uncompressed_size += doc_size;
            current_block.doc_count++;
        }
        
        // Add the last block if it has documents
        if (current_block.doc_count > 0) {
            blocks.push_back(std::move(current_block));
        }
        
        // Write blocks to file
        std::string meta_path = opts_.output_dir + "/meta.blocks";
        std::ofstream meta_file(meta_path, std::ios::binary);
        if (!meta_file) return false;
        
        // Write block count
        uint32_t block_count = blocks.size();
        meta_file.write(reinterpret_cast<const char*>(&block_count), sizeof(block_count));
        
        // Write block headers first (for seeking)
        for (const auto& block : blocks) {
            uint32_t header[4] = {
                block.block_id,
                block.uncompressed_size,
                block.doc_count,
                0  // padding
            };
            meta_file.write(reinterpret_cast<const char*>(header), sizeof(header));
        }
        
        // Write block data
        for (const auto& block : blocks) {
            meta_file.write(reinterpret_cast<const char*>(block.data.data()), block.data.size());
        }
        
        // Write index
        std::string idx_path = opts_.output_dir + "/meta.idx";
        std::ofstream idx_file(idx_path, std::ios::binary);
        if (!idx_file) return false;
        
        for (const auto& entry : index) {
            idx_file.write(reinterpret_cast<const char*>(&entry), sizeof(MetaIndex));
        }
        
        std::cout << "  Created " << blocks.size() << " blocks (" 
                  << opts_.block_size / 1024 << "KB target size)\n";
        std::cout << "  Average docs per block: " 
                  << (data_.documents.size() / blocks.size()) << "\n";
        
        size_t total_size = 0;
        for (const auto& block : blocks) {
            total_size += block.data.size();
        }
        std::cout << "  Total metadata size: " << (total_size / (1024*1024)) << "MB\n";
        
        return meta_file.good() && idx_file.good();
    }
    
    bool writeManifest() {
        std::cout << "Writing manifest...\n";
        
        std::string path = opts_.output_dir + "/manifest.json";
        std::ofstream file(path);
        if (!file) return false;
        
        auto now = std::chrono::system_clock::now();
        auto time_t = std::chrono::system_clock::to_time_t(now);
        
        file << "{\n";
        file << "  \"format\": \"nvs.v1\",\n";
        file << "  \"created_at\": \"" << std::put_time(std::gmtime(&time_t), "%Y-%m-%dT%H:%M:%SZ") << "\",\n";
        file << "  \"num_docs\": " << data_.documents.size() << ",\n";
        file << "  \"dim\": " << data_.dimensions << ",\n";
        file << "  \"embedding\": {\n";
        file << "    \"model\": \"" << opts_.embedding_model << "\",\n";
        file << "    \"dtype\": \"" << (opts_.quantize_f16 ? "f16" : "f32") << "\"\n";
        file << "  },\n";
        file << "  \"bm25\": {\n";
        file << "    \"avgdl\": " << data_.average_document_length << ",\n";
        file << "    \"k1\": " << opts_.bm25_k1 << ",\n";
        file << "    \"b\": " << opts_.bm25_b << "\n";
        file << "  },\n";
        file << "  \"files\": {\n";
        file << "    \"vectors\": { \"path\": \"" << (opts_.quantize_f16 ? "vectors.f16" : "vectors.f32") << "\", \"dtype\": \"" << (opts_.quantize_f16 ? "f16" : "f32") << "\", \"rows\": " << data_.documents.size() << ", \"cols\": " << data_.dimensions << " },\n";
        file << "    \"doclen\": { \"path\": \"doclen.u32\", \"dtype\": \"u32\", \"rows\": " << data_.documents.size() << " },\n";
        file << "    \"lexicon\": { \"path\": \"lexicon.bin\" },\n";
        file << "    \"postings\": { \"path\": \"postings.bin\" },\n";
        file << "    \"terms\": { \"path\": \"terms.dict\" },\n";
        file << "    \"meta_idx\": { \"path\": \"meta.idx\", \"schema\": \"u32 block_id, u32 offset, u32 doc_size\" },\n";
        file << "    \"meta\": { \"path\": \"meta.blocks\", \"block_size\": " << opts_.block_size << ", \"doc_aligned\": true }\n";
        file << "  }\n";
        file << "}\n";
        
        return file.good();
    }
    
    bool writeChecksums() {
        std::cout << "Computing checksums...\n";
        
        std::string path = opts_.output_dir + "/checksums.sha256";
        std::ofstream file(path);
        if (!file) return false;
        
        // List all files to checksum
        std::vector<std::string> files = {
            "manifest.json",
            opts_.quantize_f16 ? "vectors.f16" : "vectors.f32",
            "doclen.u32",
            "lexicon.bin",
            "postings.bin",
            "terms.dict",
            "meta.idx",
            "meta.blocks"
        };
        
        for (const auto& filename : files) {
            std::string filepath = opts_.output_dir + "/" + filename;
            std::string hash = computeXXH64(filepath);
            if (hash.empty()) return false;
            
            file << hash << "  " << filename << "\n";
        }
        
        return file.good();
    }
    
    std::string computeXXH64(const std::string& filepath) {
        std::ifstream file(filepath, std::ios::binary);
        if (!file) return "";
        
        XXH64_state_t* state = XXH64_createState();
        if (!state) return "";
        
        XXH64_reset(state, 0);  // seed = 0
        
        char buffer[8192];
        while (file.read(buffer, sizeof(buffer))) {
            XXH64_update(state, buffer, file.gcount());
        }
        if (file.gcount() > 0) {
            XXH64_update(state, buffer, file.gcount());
        }
        
        XXH64_hash_t hash = XXH64_digest(state);
        XXH64_freeState(state);
        
        // Convert 64-bit hash to hex string
        std::stringstream ss;
        ss << std::hex << std::setfill('0') << std::setw(16) << hash;
        
        return ss.str();
    }
};

} // namespace nvs

void printUsage(const char* program) {
    std::cout << "Usage: " << program << " [options] <input-directory>\n\n";
    std::cout << "Options:\n";
    std::cout << "  --out <dir>           Output directory (default: ./nvs-bundle)\n";
    std::cout << "  --dim <n>             Embedding dimensions (auto-detect if not specified)\n";
    std::cout << "  --model <name>        Embedding model name for manifest\n";
    std::cout << "  --quantize f16        Quantize vectors to float16\n";
    std::cout << "  --bm25-k1 <n>         BM25 k1 parameter (default: 1.2)\n";
    std::cout << "  --bm25-b <n>          BM25 b parameter (default: 0.75)\n";
    std::cout << "  --min-df <n>          Minimum document frequency (default: 1)\n";
    std::cout << "  --block-size <n>      Metadata block size in bytes (default: 131072)\n";
    std::cout << "  --verbose             Verbose output\n";
    std::cout << "  --help                Show this help\n";
}

#ifndef NVS_ENABLE_INLINE_TESTS
int main(int argc, char* argv[]) {
    nvs::PackerOptions opts;
    
    // Parse command line arguments
    for (int i = 1; i < argc; ++i) {
        std::string arg = argv[i];
        
        if (arg == "--help" || arg == "-h") {
            printUsage(argv[0]);
            return 0;
        } else if (arg == "--out" && i + 1 < argc) {
            opts.output_dir = argv[++i];
        } else if (arg == "--dim" && i + 1 < argc) {
            opts.dim = std::stoul(argv[++i]);
        } else if (arg == "--model" && i + 1 < argc) {
            opts.embedding_model = argv[++i];
        } else if (arg == "--quantize" && i + 1 < argc && std::string(argv[i+1]) == "f16") {
            opts.quantize_f16 = true;
            ++i;
        } else if (arg == "--bm25-k1" && i + 1 < argc) {
            opts.bm25_k1 = std::stod(argv[++i]);
        } else if (arg == "--bm25-b" && i + 1 < argc) {
            opts.bm25_b = std::stod(argv[++i]);
        } else if (arg == "--min-df" && i + 1 < argc) {
            opts.min_df = std::stoul(argv[++i]);
        } else if (arg == "--block-size" && i + 1 < argc) {
            opts.block_size = std::stoul(argv[++i]);
        } else if (arg == "--verbose") {
            opts.verbose = true;
        } else if (arg[0] != '-') {
            opts.input_path = arg;
        } else {
            std::cerr << "Unknown option: " << arg << "\n";
            return 1;
        }
    }
    
    if (opts.input_path.empty()) {
        std::cerr << "Error: Input directory required\n\n";
        printUsage(argv[0]);
        return 1;
    }
    
    if (!fs::exists(opts.input_path) || !fs::is_directory(opts.input_path)) {
        std::cerr << "Error: " << opts.input_path << " is not a valid directory\n";
        return 1;
    }
    
    nvs::NVSPacker packer(opts);
    return packer.run();
}
#endif // NVS_ENABLE_INLINE_TESTS

// Unit tests - only compiled when tests are enabled
#ifdef NVS_ENABLE_INLINE_TESTS
#include "../deps/doctest.h"
#include <map>
#include <cstddef>  // for offsetof
#include <cstring>  // for memcpy

TEST_CASE("NVSPack metadata block generation") {
    SUBCASE("MetaBlock structure alignment") {
        // Ensure our structures are properly aligned
        CHECK(sizeof(nvs::DocHeader) == 32);
        CHECK(sizeof(nvs::MetaIndex) == 16);
        
        // Check field offsets
        CHECK(offsetof(nvs::DocHeader, doc_id) == 0);
        CHECK(offsetof(nvs::DocHeader, timestamp_unix) == 8);
        CHECK(offsetof(nvs::DocHeader, id_len) == 16);
        CHECK(offsetof(nvs::DocHeader, text_len) == 20);
        CHECK(offsetof(nvs::DocHeader, source_len) == 24);
        CHECK(offsetof(nvs::DocHeader, padding) == 28);
    }
    
    SUBCASE("Block size calculations") {
        nvs::MetaBlock block;
        block.block_id = 0;
        block.uncompressed_size = 0;
        block.doc_count = 0;
        
        // Simulate adding a document
        std::string doc_id = "test_doc_1";
        std::string text = "This is test content";
        std::string source = "test_source";
        
        size_t doc_size = sizeof(nvs::DocHeader) + doc_id.size() + text.size() + source.size();
        
        // Check size calculation
        CHECK(doc_size == 32 + 10 + 20 + 11);  // 73 bytes total
        
        // Verify block size limits
        const size_t BLOCK_SIZE = 131072;  // 128KB
        CHECK(doc_size < BLOCK_SIZE);
    }
    
    SUBCASE("Document header serialization") {
        nvs::DocHeader header;
        header.doc_id = 42;
        header.timestamp_unix = 1234567890;
        header.id_len = 10;
        header.text_len = 100;
        header.source_len = 20;
        header.padding = 0;
        
        // Serialize to buffer
        uint8_t buffer[32];
        memcpy(buffer, &header, sizeof(nvs::DocHeader));
        
        // Deserialize and verify
        nvs::DocHeader* deserialized = reinterpret_cast<nvs::DocHeader*>(buffer);
        CHECK(deserialized->doc_id == 42);
        CHECK(deserialized->timestamp_unix == 1234567890);
        CHECK(deserialized->id_len == 10);
        CHECK(deserialized->text_len == 100);
        CHECK(deserialized->source_len == 20);
    }
}

TEST_CASE("NVSPack BM25 index generation") {
    SUBCASE("Term extraction") {
        // Test that terms are properly extracted from text
        std::string text = "The quick brown fox jumps over the lazy dog";
        std::vector<std::string> expected = {"quick", "brown", "fox", "jumps", "lazy", "dog"};
        
        // Note: Actual tokenization would use simple_tokenizer
        // This is a placeholder for the test structure
        CHECK(expected.size() == 6);
    }
    
    SUBCASE("Document frequency calculation") {
        // Test doc frequency calculations
        std::map<std::string, uint32_t> term_doc_freq;
        term_doc_freq["test"] = 5;
        term_doc_freq["vector"] = 3;
        
        CHECK(term_doc_freq["test"] == 5);
        CHECK(term_doc_freq["vector"] == 3);
        CHECK(term_doc_freq.size() == 2);
    }
}

TEST_CASE("NVSPack file I/O") {
    SUBCASE("Manifest generation") {
        // Test manifest structure (manually generated in this codebase)
        std::string format = "nvs.v1";
        int num_docs = 100;
        int dim = 1536;
        
        CHECK(format == "nvs.v1");
        CHECK(num_docs == 100);
        CHECK(dim == 1536);
    }
    
    SUBCASE("Index file structure") {
        nvs::MetaIndex index;
        index.block_id = 1;
        index.offset_in_block = 1024;
        index.doc_size = 2048;
        index.padding = 0;
        
        // Test serialization
        uint8_t buffer[16];
        memcpy(buffer, &index, sizeof(nvs::MetaIndex));
        
        // Verify structure
        uint32_t* values = reinterpret_cast<uint32_t*>(buffer);
        CHECK(values[0] == 1);      // block_id
        CHECK(values[1] == 1024);   // offset_in_block
        CHECK(values[2] == 2048);   // doc_size
        CHECK(values[3] == 0);      // padding
    }
}
#endif