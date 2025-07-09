#include "vector_store.h"
#include <iostream>
#include <filesystem>
#include <vector>
#include <simdjson.h>
#include <cctype>

void test_single_document() {
    std::cout << "=== Testing Single Document ===" << std::endl;
    
    VectorStore store(20);
    
    // Create a test document manually
    std::string json_str = R"({
        "id": "test1",
        "text": "Test document for debugging",
        "metadata": {
            "embedding": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 
                         0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
            "category": "test"
        }
    })";
    
    try {
        simdjson::ondemand::parser parser;
        simdjson::padded_string padded(json_str);
        auto doc = parser.iterate(padded).value_unsafe();
        
        std::cout << "Adding document..." << std::endl;
        store.add_document(doc);
        std::cout << "Document added successfully. Store size: " << store.size() << std::endl;
        
        // Test search
        float query[20] = {0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0,
                          0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0};
        
        auto results = store.search(query, 1);
        std::cout << "Search completed. Found " << results.size() << " results" << std::endl;
        if (!results.empty()) {
            std::cout << "Top result score: " << results[0].first << std::endl;
        }
        
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << std::endl;
    }
}

void test_load_directory(const std::string& path) {
    std::cout << "\n=== Testing Load Directory ===" << std::endl;
    std::cout << "Loading from: " << path << std::endl;
    
    VectorStore store(20);
    
    try {
        // Collect JSON files
        std::vector<std::filesystem::path> json_files;
        for (const auto& entry : std::filesystem::directory_iterator(path)) {
            if (entry.path().extension() == ".json") {
                std::cout << "Found JSON file: " << entry.path() << std::endl;
                json_files.push_back(entry.path());
            }
        }
        
        std::cout << "Total JSON files found: " << json_files.size() << std::endl;
        
        // Process files
        simdjson::ondemand::parser file_parser;
        for (size_t i = 0; i < json_files.size(); ++i) {
            std::cout << "Processing file " << (i+1) << "/" << json_files.size() 
                      << ": " << json_files[i].filename() << std::endl;
            
            try {
                std::cout << "  Loading file..." << std::endl;
                auto json = simdjson::padded_string::load(json_files[i].string()).value_unsafe();
                std::cout << "  File loaded, size: " << json.size() << " bytes" << std::endl;
                
                std::cout << "  Parsing JSON..." << std::endl;
                auto json_doc = file_parser.iterate(json).value_unsafe();
                std::cout << "  JSON parsed successfully" << std::endl;
                
                // Check first character to determine if it's an array
                const char* json_start = json.data();
                while (json_start && *json_start && std::isspace(*json_start)) {
                    json_start++;
                }
                
                if (json_start && *json_start == '[') {
                    // It's an array
                    std::cout << "  Detected array of documents" << std::endl;
                    auto doc_array = json_doc.get_array().value_unsafe();
                    
                    size_t doc_count = 0;
                    for (auto doc_element : doc_array) {
                        auto doc = doc_element.get_object().value_unsafe();
                        store.add_document(doc);
                        doc_count++;
                    }
                    std::cout << "  Added " << doc_count << " documents. Current store size: " << store.size() << std::endl;
                } else {
                    // Single document
                    std::cout << "  Detected single document" << std::endl;
                    std::cout << "  Adding to store..." << std::endl;
                    store.add_document(json_doc);
                    std::cout << "  Document added. Current store size: " << store.size() << std::endl;
                }
                
            } catch (const std::exception& e) {
                std::cerr << "  Error processing file: " << e.what() << std::endl;
            }
        }
        
        std::cout << "\nAll files processed. Final store size: " << store.size() << std::endl;
        
        // Test normalization
        std::cout << "Testing normalization..." << std::endl;
        store.normalize_all();
        std::cout << "Normalization completed" << std::endl;
        
    } catch (const std::exception& e) {
        std::cerr << "Error in load directory: " << e.what() << std::endl;
    }
}

int main(int argc, char** argv) {
    std::cout << "Vector Store C++ Test Program" << std::endl;
    std::cout << "=============================" << std::endl;
    
    // Test single document first
    test_single_document();
    
    // Test directory loading
    std::string test_dir = (argc > 1) ? argv[1] : "test";
    test_load_directory(test_dir);
    
    std::cout << "\nAll tests completed!" << std::endl;
    return 0;
}