#pragma once
#include "vector_store.h"
#include <string>

// Clean interface for loading documents from a directory
class VectorStoreLoader {
public:
    // Load all JSON files from a directory into the vector store
    // Automatically calls finalize() when complete
    static void loadDirectory(VectorStore* store, const std::string& path);
};