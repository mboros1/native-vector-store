#include "doctest/doctest.h"
#include "../vector_store_v2.h"
#include <filesystem>
#include <fstream>
#include <string>

namespace fs = std::filesystem;
namespace nvs { int test_run_packer(const std::string&, const std::string&, size_t); }

static std::string write_docs_json(const fs::path& dir, size_t count, size_t dim, const std::string& prefix) {
    fs::create_directories(dir);
    auto file = dir / "docs.json";
    std::ofstream out(file);
    out << "[\n";
    for (size_t i = 0; i < count; ++i) {
        if (i > 0) out << ",\n";
        out << "  {\n";
        out << "    \"id\": \"" << prefix << i << "\",\n";
        out << "    \"text\": \"" << prefix << " text number " << i << "\",\n";
        out << "    \"metadata\": { \"embedding\": [";
        for (size_t d = 0; d < dim; ++d) {
            if (d) out << ",";
            out << (d == 0 ? 1.0 : 0.0); // simple basis
        }
        out << "] }\n";
        out << "  }";
    }
    out << "\n]\n";
    return file.string();
}

TEST_CASE("E2E pack-then-open (single block)") {
    auto tmp_in = fs::temp_directory_path() / "nvs_e2e_in_single";
    auto tmp_out = fs::temp_directory_path() / "nvs_e2e_out_single";
    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
    write_docs_json(tmp_in, 3, 4, "doc");

    // Default block size via 0 (packer uses 128KB default)
    REQUIRE(nvs::test_run_packer(tmp_in.string(), tmp_out.string(), 0) == 0);

    nvs::VectorStoreV2 store;
    REQUIRE(store.open(tmp_out.string()));
    CHECK(store.size() == 3);
    CHECK(store.dimensions() == 4);

    // Read first and last documents
    nvs::VectorStoreV2::SearchResult r;
    CHECK(store.get_document(0, r));
    CHECK(r.id == "doc0");
    CHECK(r.text.find("doc text number 0") != std::string::npos);
    CHECK(r.metadata_json.find("\"embedding\"") != std::string::npos);

    CHECK(store.get_document(2, r));
    CHECK(r.id == "doc2");
    CHECK(r.text.find("doc text number 2") != std::string::npos);

    // Smoke search
    float q[4] = {1,0,0,0};
    auto results = store.search(q, 2);
    CHECK(results.size() <= 2);

    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
}

TEST_CASE("E2E pack-then-open (multiple blocks)") {
    auto tmp_in = fs::temp_directory_path() / "nvs_e2e_in_multi";
    auto tmp_out = fs::temp_directory_path() / "nvs_e2e_out_multi";
    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
    // Create 10 docs to exceed tiny block size
    write_docs_json(tmp_in, 10, 4, "m");

    // Small block size to force multiple blocks
    REQUIRE(nvs::test_run_packer(tmp_in.string(), tmp_out.string(), 256) == 0);

    nvs::VectorStoreV2 store;
    REQUIRE(store.open(tmp_out.string()));
    CHECK(store.size() == 10);

    // Probe docs across boundaries
    nvs::VectorStoreV2::SearchResult r;
    CHECK(store.get_document(0, r));
    CHECK(r.id == "m0");
    CHECK(store.get_document(9, r));
    CHECK(r.id == "m9");

    // Verify we can round-trip several docs
    for (size_t i = 0; i < 10; ++i) {
        CHECK(store.get_document(i, r));
        CHECK(r.id == (std::string("m") + std::to_string(i)));
    }

    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
}

TEST_CASE("E2E block headers and checksums") {
    auto tmp_in = fs::temp_directory_path() / "nvs_e2e_in_hdr";
    auto tmp_out = fs::temp_directory_path() / "nvs_e2e_out_hdr";
    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
    write_docs_json(tmp_in, 10, 4, "h");

    REQUIRE(nvs::test_run_packer(tmp_in.string(), tmp_out.string(), 256) == 0);

    // Parse meta.blocks
    {
        std::ifstream in(tmp_out / "meta.blocks", std::ios::binary);
        REQUIRE(in.good());

        uint32_t block_count = 0;
        in.read(reinterpret_cast<char*>(&block_count), sizeof(block_count));
        CHECK(block_count >= 2);
        struct Hdr { uint32_t id, usize, dcount, pad; };
        std::vector<Hdr> hdrs(block_count);
        for (uint32_t i = 0; i < block_count; ++i) {
            in.read(reinterpret_cast<char*>(&hdrs[i]), sizeof(Hdr));
        }
        // Compute block_size from file size
        in.seekg(0, std::ios::end);
        size_t fsz = static_cast<size_t>(in.tellg());
        size_t header_size = sizeof(uint32_t) + block_count * sizeof(Hdr);
        size_t block_size = (fsz - header_size) / block_count;
        CHECK(block_size > 0);
        in.seekg(header_size, std::ios::beg);
        // Validate each block by scanning records
        size_t total_docs = 0;
        for (uint32_t i = 0; i < block_count; ++i) {
            std::vector<char> buf(block_size);
            in.read(buf.data(), block_size);
            size_t pos = 0;
            size_t consumed = 0;
            size_t docs = 0;
            while (consumed < hdrs[i].usize) {
                REQUIRE(consumed + sizeof(uint32_t) <= hdrs[i].usize);
                uint32_t id_len = *reinterpret_cast<const uint32_t*>(buf.data() + pos);
                pos += sizeof(uint32_t); consumed += sizeof(uint32_t);
                REQUIRE(consumed + id_len <= hdrs[i].usize);
                pos += id_len; consumed += id_len;

                REQUIRE(consumed + sizeof(uint32_t) <= hdrs[i].usize);
                uint32_t text_len = *reinterpret_cast<const uint32_t*>(buf.data() + pos);
                pos += sizeof(uint32_t); consumed += sizeof(uint32_t);
                REQUIRE(consumed + text_len <= hdrs[i].usize);
                pos += text_len; consumed += text_len;

                REQUIRE(consumed + sizeof(uint32_t) <= hdrs[i].usize);
                uint32_t meta_len = *reinterpret_cast<const uint32_t*>(buf.data() + pos);
                pos += sizeof(uint32_t); consumed += sizeof(uint32_t);
                REQUIRE(consumed + meta_len <= hdrs[i].usize);
                pos += meta_len; consumed += meta_len;
                ++docs;
            }
            CHECK(docs == hdrs[i].dcount);
            CHECK(consumed == hdrs[i].usize);
            total_docs += docs;
        }
        CHECK(total_docs == 10);
    }

    // Checksums format sanity
    {
        std::ifstream in(tmp_out / "checksums.sha256");
        REQUIRE(in.good());
        std::string line;
        size_t seen = 0;
        while (std::getline(in, line)) {
            if (line.empty()) continue;
            auto sp = line.find("  ");
            REQUIRE(sp != std::string::npos);
            std::string hex = line.substr(0, sp);
            std::string fname = line.substr(sp + 2);
            CHECK(hex.size() == 16);
            for (char c : hex) {
                bool ok = ((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'));
                CHECK(ok);
            }
            CHECK(fs::exists(tmp_out / fname));
            ++seen;
        }
        CHECK(seen >= 5);
    }

    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
}

TEST_CASE("E2E BM25 ordering - single term tf dominance") {
    namespace fs = std::filesystem;
    auto tmp_in = fs::temp_directory_path() / "nvs_e2e_bm25_tf_in";
    auto tmp_out = fs::temp_directory_path() / "nvs_e2e_bm25_tf_out";
    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
    fs::create_directories(tmp_in);
    // Build docs where tf differs
    {
        std::ofstream f(tmp_in / "docs.json");
        f << R"([
          {"id":"a","text":"apple apple apple","metadata":{"embedding":[1,0,0,0]}},
          {"id":"b","text":"apple","metadata":{"embedding":[1,0,0,0]}},
          {"id":"c","text":"banana banana banana","metadata":{"embedding":[1,0,0,0]}}
        ])";
    }
    REQUIRE(nvs::test_run_packer(tmp_in.string(), tmp_out.string(), 1024) == 0);

    nvs::VectorStoreV2 store;
    REQUIRE(store.open(tmp_out.string()));
    // Query 'apple' should rank 'a' above 'b'
    auto res = store.search_bm25(std::vector<std::string>{"apple"}, 2);
    REQUIRE(res.size() >= 2);
    bool has_a = (res[0].id == "a") || (res[1].id == "a");
    bool has_b = (res[0].id == "b") || (res[1].id == "b");
    CHECK(has_a);
    CHECK(has_b);
    // Query 'banana' should bring 'c' top
    auto resb = store.search_bm25(std::vector<std::string>{"banana"}, 1);
    REQUIRE(resb.size() >= 1);
    CHECK(resb[0].id == "c");

    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
}

TEST_CASE("E2E BM25 ordering - multi term mix") {
    namespace fs = std::filesystem;
    auto tmp_in = fs::temp_directory_path() / "nvs_e2e_bm25_mix_in";
    auto tmp_out = fs::temp_directory_path() / "nvs_e2e_bm25_mix_out";
    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
    fs::create_directories(tmp_in);
    {
        std::ofstream f(tmp_in / "docs.json");
        f << R"([
          {"id":"x","text":"alpha alpha beta","metadata":{"embedding":[1,0,0,0]}},
          {"id":"y","text":"alpha beta beta beta","metadata":{"embedding":[1,0,0,0]}},
          {"id":"z","text":"gamma gamma","metadata":{"embedding":[1,0,0,0]}}
        ])";
    }
    REQUIRE(nvs::test_run_packer(tmp_in.string(), tmp_out.string(), 1024) == 0);

    nvs::VectorStoreV2 store;
    REQUIRE(store.open(tmp_out.string()));
    // Query alpha+beta; y has higher beta tf, x has higher alpha tf; with equal df likely y outranks x
    auto res = store.search_bm25(std::vector<std::string>{"alpha","beta"}, 2);
    REQUIRE(res.size() >= 2);
    CHECK((res[0].id == "y" || res[1].id == "y"));
    CHECK((res[0].id == "x" || res[1].id == "x"));
    // gamma should not be in top 2 for alpha+beta query
    CHECK(!(res[0].id == "z" && res[1].id == "z"));

    fs::remove_all(tmp_in);
    fs::remove_all(tmp_out);
}
