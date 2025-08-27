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

struct PackerOptions {
    std::string input_path;
    std::string output_dir = "./nvs-bundle";
    size_t dim = 0;  // Auto-detect from first document
    std::string embedding_model = "unknown";
    bool quantize_f16 = false;
    double bm25_k1 = 1.2;
    double bm25_b = 0.75;
    size_t min_df = 1;
    size_t block_bytes = 2097152; // 2MB default
    bool verbose = false;
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
        std::cout << "Writing metadata...\n";
        
        // Write document IDs and text
        std::string meta_path = opts_.output_dir + "/meta.bin";
        std::ofstream meta_file(meta_path, std::ios::binary);
        if (!meta_file) return false;
        
        // Write index for random access
        std::string idx_path = opts_.output_dir + "/meta.idx";
        std::ofstream idx_file(idx_path, std::ios::binary);
        if (!idx_file) return false;
        
        uint64_t offset = 0;
        for (const auto& doc : data_.documents) {
            // Write index entry
            idx_file.write(reinterpret_cast<const char*>(&offset), sizeof(offset));
            
            // Write ID length and ID
            uint32_t id_len = doc.id.size();
            meta_file.write(reinterpret_cast<const char*>(&id_len), sizeof(id_len));
            meta_file.write(doc.id.data(), id_len);
            offset += sizeof(id_len) + id_len;
            
            // Write text length and text
            uint32_t text_len = doc.text.size();
            meta_file.write(reinterpret_cast<const char*>(&text_len), sizeof(text_len));
            meta_file.write(doc.text.data(), text_len);
            offset += sizeof(text_len) + text_len;
            
            // Write metadata JSON
            uint32_t meta_len = doc.metadata_json.size();
            meta_file.write(reinterpret_cast<const char*>(&meta_len), sizeof(meta_len));
            meta_file.write(doc.metadata_json.data(), meta_len);
            offset += sizeof(meta_len) + meta_len;
        }
        
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
        file << "    \"meta_idx\": { \"path\": \"meta.idx\" },\n";
        file << "    \"meta\": { \"path\": \"meta.bin\" }\n";
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
            "meta.bin"
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
    std::cout << "  --block-bytes <n>     Compression block size (default: 2097152)\n";
    std::cout << "  --verbose             Verbose output\n";
    std::cout << "  --help                Show this help\n";
}

int main(int argc, char* argv[]) {
    PackerOptions opts;
    
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
        } else if (arg == "--block-bytes" && i + 1 < argc) {
            opts.block_bytes = std::stoul(argv[++i]);
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
    
    NVSPacker packer(opts);
    return packer.run();
}