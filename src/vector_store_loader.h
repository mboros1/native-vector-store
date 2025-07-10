#pragma once
#include "vector_store.h"
#include <string>

// Clean interface for loading documents from a directory
class VectorStoreLoader {
public:
    // Load all JSON files from a directory into the vector store
    // Automatically calls finalize() when complete
    static void loadDirectory(VectorStore* store, const std::string& path);
    
    // Memory-mapped version for better performance with large files
    // Uses zero-copy I/O via mmap/MapViewOfFile
    static void loadDirectoryMMap(VectorStore* store, const std::string& path);
    
    // Adaptive loader that chooses the best method per file
    // Uses mmap for files <5MB, standard loading for larger files
    static void loadDirectoryAdaptive(VectorStore* store, const std::string& path);
};