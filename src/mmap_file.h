#pragma once
#include <string>
#include <memory>

#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#endif

// Cross-platform memory-mapped file wrapper
class MMapFile {
public:
    MMapFile() = default;
    ~MMapFile() { close(); }
    
    // Disable copy, enable move
    MMapFile(const MMapFile&) = delete;
    MMapFile& operator=(const MMapFile&) = delete;
    MMapFile(MMapFile&& other) noexcept { *this = std::move(other); }
    MMapFile& operator=(MMapFile&& other) noexcept {
        if (this != &other) {
            close();
            data_ = other.data_;
            size_ = other.size_;
            #ifdef _WIN32
            file_handle_ = other.file_handle_;
            map_handle_ = other.map_handle_;
            other.file_handle_ = INVALID_HANDLE_VALUE;
            other.map_handle_ = nullptr;
            #else
            fd_ = other.fd_;
            other.fd_ = -1;
            #endif
            other.data_ = nullptr;
            other.size_ = 0;
        }
        return *this;
    }
    
    bool open(const std::string& filepath) {
        close();
        
        #ifdef _WIN32
        // Windows implementation
        file_handle_ = CreateFileA(filepath.c_str(), GENERIC_READ, FILE_SHARE_READ,
                                   nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (file_handle_ == INVALID_HANDLE_VALUE) {
            return false;
        }
        
        LARGE_INTEGER file_size;
        if (!GetFileSizeEx(file_handle_, &file_size)) {
            CloseHandle(file_handle_);
            file_handle_ = INVALID_HANDLE_VALUE;
            return false;
        }
        size_ = static_cast<size_t>(file_size.QuadPart);
        
        if (size_ == 0) {
            return true; // Empty file, nothing to map
        }
        
        map_handle_ = CreateFileMappingA(file_handle_, nullptr, PAGE_READONLY, 0, 0, nullptr);
        if (!map_handle_) {
            CloseHandle(file_handle_);
            file_handle_ = INVALID_HANDLE_VALUE;
            return false;
        }
        
        data_ = MapViewOfFile(map_handle_, FILE_MAP_READ, 0, 0, 0);
        if (!data_) {
            CloseHandle(map_handle_);
            CloseHandle(file_handle_);
            map_handle_ = nullptr;
            file_handle_ = INVALID_HANDLE_VALUE;
            return false;
        }
        #else
        // POSIX implementation
        fd_ = ::open(filepath.c_str(), O_RDONLY);
        if (fd_ < 0) {
            return false;
        }
        
        struct stat st;
        if (fstat(fd_, &st) < 0) {
            ::close(fd_);
            fd_ = -1;
            return false;
        }
        size_ = static_cast<size_t>(st.st_size);
        
        if (size_ == 0) {
            return true; // Empty file, nothing to map
        }
        
        data_ = mmap(nullptr, size_, PROT_READ, MAP_PRIVATE, fd_, 0);
        if (data_ == MAP_FAILED) {
            ::close(fd_);
            fd_ = -1;
            data_ = nullptr;
            return false;
        }
        
        // Advise kernel about access pattern
        madvise(data_, size_, MADV_SEQUENTIAL);
        #endif
        
        return true;
    }
    
    void close() {
        if (data_) {
            #ifdef _WIN32
            UnmapViewOfFile(data_);
            #else
            munmap(data_, size_);
            #endif
            data_ = nullptr;
        }
        
        #ifdef _WIN32
        if (map_handle_) {
            CloseHandle(map_handle_);
            map_handle_ = nullptr;
        }
        if (file_handle_ != INVALID_HANDLE_VALUE) {
            CloseHandle(file_handle_);
            file_handle_ = INVALID_HANDLE_VALUE;
        }
        #else
        if (fd_ >= 0) {
            ::close(fd_);
            fd_ = -1;
        }
        #endif
        
        size_ = 0;
    }
    
    const char* data() const { return static_cast<const char*>(data_); }
    size_t size() const { return size_; }
    bool is_open() const { return data_ != nullptr || size_ == 0; }
    
private:
    void* data_ = nullptr;
    size_t size_ = 0;
    
    #ifdef _WIN32
    HANDLE file_handle_ = INVALID_HANDLE_VALUE;
    HANDLE map_handle_ = nullptr;
    #else
    int fd_ = -1;
    #endif
};