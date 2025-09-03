// Minimal converter: cl100k_base_data.h (hex byte array) -> compact binary mapping
// Format:
// [u32 magic 'TMAP'] [u32 version=1] [u32 count]
// repeated count times: [u32 id] [u16 len] [len bytes]
//
// Usage: ./convert_cl100k_to_bin [path/to/cl100k_base_data.h] [out.bin]

#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

static std::vector<uint8_t> extract_bytes_from_header(const std::string& content) {
    // Simple manual parse: find array decl, then braces, then scan for 0xNN tokens.
    const std::string marker = "cl100k_base_tiktoken[]";
    auto pos = content.find(marker);
    if (pos == std::string::npos) throw std::runtime_error("array decl not found");
    auto open = content.find('{', pos);
    if (open == std::string::npos) throw std::runtime_error("open brace not found");
    auto close = content.find('}', open);
    if (close == std::string::npos) throw std::runtime_error("close brace not found");
    std::string body = content.substr(open + 1, close - open - 1);
    
    std::vector<uint8_t> data;
    data.reserve(1700000);
    const char* s = body.c_str();
    const char* e = s + body.size();
    while (s < e) {
        while (s < e && *s != '0') ++s;
        if (s + 3 < e && s[0] == '0' && s[1] == 'x' && std::isxdigit((unsigned char)s[2]) && std::isxdigit((unsigned char)s[3])) {
            char hx[3] = { s[2], s[3], '\0' };
            uint8_t v = (uint8_t)std::strtoul(hx, nullptr, 16);
            data.push_back(v);
            s += 4;
        } else {
            ++s;
        }
    }
    return data;
}

static std::vector<uint8_t> b64_decode(const std::string& s) {
    static uint8_t map[256];
    static bool inited = false;
    if (!inited) {
        std::fill(std::begin(map), std::end(map), 0xFF);
        for (int i = 0; i < 26; ++i) { map['A' + i] = i; }
        for (int i = 0; i < 26; ++i) { map['a' + i] = 26 + i; }
        for (int i = 0; i < 10; ++i) { map['0' + i] = 52 + i; }
        map[(unsigned)'+'] = 62; map[(unsigned)'/'] = 63;
        inited = true;
    }
    std::vector<uint8_t> out; out.reserve(s.size() * 3 / 4);
    uint32_t val = 0; int valb = -8;
    for (unsigned char c : s) {
        if (c == '=') break;
        uint8_t d = map[c];
        if (d == 0xFF) continue;
        val = (val << 6) | d; valb += 6;
        if (valb >= 0) { out.push_back((uint8_t)((val >> valb) & 0xFF)); valb -= 8; }
    }
    return out;
}

int main(int argc, char** argv) {
    std::string header_path;
    std::string out_path;
    if (argc >= 2) header_path = argv[1];
    else header_path = std::string("integration-work/fast-pdf-parser/include/fast_pdf_parser/cl100k_base_data.h");
    if (argc >= 3) out_path = argv[2];
    else out_path = std::string("rust/crates/tokenmonster/data/cl100k_base.bin");

    // Read header
    std::ifstream in(header_path, std::ios::binary);
    if (!in) { std::cerr << "Cannot open " << header_path << "\n"; return 1; }
    std::string content((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    in.close();

    // Extract raw bytes and decode to mapping text
    auto bytes = extract_bytes_from_header(content);
    std::string text(bytes.begin(), bytes.end());

    // Parse mapping lines: <base64> <id>
    std::vector<std::pair<std::vector<uint8_t>, uint32_t>> entries;
    entries.reserve(100000);
    size_t line_start = 0;
    while (line_start < text.size()) {
        size_t line_end = text.find('\n', line_start);
        if (line_end == std::string::npos) line_end = text.size();
        std::string_view line(&text[line_start], line_end - line_start);
        // trim
        auto l = line.find_first_not_of(" \t\r");
        if (l != std::string::npos) {
            auto r = line.find_last_not_of(" \t\r");
            std::string_view t = line.substr(l, r - l + 1);
            if (!t.empty()) {
                size_t sp = t.find(' ');
                if (sp != std::string::npos) {
                    std::string b64(t.substr(0, sp));
                    std::string id_str(t.substr(sp + 1));
                    char* endp = nullptr; unsigned long id_ul = std::strtoul(id_str.c_str(), &endp, 10);
                    if (endp && *endp == '\0') {
                        auto tok = b64_decode(b64);
                        if (!tok.empty()) entries.emplace_back(std::move(tok), (uint32_t)id_ul);
                    }
                }
            }
        }
        line_start = line_end + 1;
    }

    // Ensure output directory exists (best-effort)
    {
        auto slash = out_path.find_last_of("/");
        if (slash != std::string::npos) {
            std::string dir = out_path.substr(0, slash);
            // Not creating dirs in portable C++ here; assume it exists or user created.
        }
    }

    std::ofstream out(out_path, std::ios::binary);
    if (!out) { std::cerr << "Cannot open output " << out_path << "\n"; return 2; }

    auto put_u32 = [&](uint32_t v) { out.put((char)(v & 0xFF)); out.put((char)((v >> 8) & 0xFF)); out.put((char)((v >> 16) & 0xFF)); out.put((char)((v >> 24) & 0xFF)); };
    auto put_u16 = [&](uint16_t v) { out.put((char)(v & 0xFF)); out.put((char)((v >> 8) & 0xFF)); };

    // Header
    put_u32(0x50414D54u); // 'TMAP' little-endian
    put_u32(1u);
    put_u32((uint32_t)entries.size());

    // Records
    size_t total_bytes = 0;
    for (auto& e : entries) {
        const auto& tok = e.first; uint32_t id = e.second;
        put_u32(id);
        if (tok.size() > 0xFFFF) { std::cerr << "Token too long (" << tok.size() << ")\n"; return 3; }
        put_u16((uint16_t)tok.size());
        out.write((const char*)tok.data(), (std::streamsize)tok.size());
        total_bytes += tok.size();
    }
    out.close();

    std::cout << "Wrote " << entries.size() << " entries, token-bytes=" << total_bytes << " to " << out_path << "\n";
    return 0;
}
