# MuPDF and Parallel Processing Code Snippets

## 1. MuPDF Core Text Extraction

### Creating MuPDF Context (text_extractor.cpp:13-19)
```cpp
// Initialize MuPDF context with unlimited store
ctx = fz_new_context(NULL, NULL, FZ_STORE_UNLIMITED);
if (!ctx) {
    throw std::runtime_error("Failed to create MuPDF context");
}
fz_register_document_handlers(ctx);
```

### Opening and Processing a PDF Page (text_extractor.cpp:43-65)
```cpp
// Open document
doc = fz_open_document(ctx, pdf_path.c_str());

// Load specific page
page = fz_load_page(ctx, doc, page_number);

// Configure text extraction options
fz_stext_options opts = { 0 };
opts.flags = FZ_STEXT_PRESERVE_LIGATURES | FZ_STEXT_PRESERVE_WHITESPACE;

// Extract structured text from page
stext = fz_new_stext_page_from_page(ctx, page, &opts);

// Convert to JSON (custom function)
result = stext_to_json(stext, options);
```

### MuPDF Error Handling Pattern (text_extractor.cpp:43-74)
```cpp
fz_try(ctx) {
    // MuPDF operations that might throw
    doc = fz_open_document(ctx, pdf_path.c_str());
    page = fz_load_page(ctx, doc, page_number);
    // ... more operations
}
fz_always(ctx) {
    // Cleanup - always executed
    if (stext) fz_drop_stext_page(ctx, stext);
    if (page) fz_drop_page(ctx, page);
    if (doc) fz_drop_document(ctx, doc);
}
fz_catch(ctx) {
    // Handle MuPDF errors
    throw std::runtime_error("MuPDF error during text extraction");
}
```

### Extracting Text from Structure (text_extractor.cpp:132-186)
```cpp
// Iterate through text blocks
for (fz_stext_block *block = stext->first_block; block; block = block->next) {
    if (block->type == FZ_STEXT_BLOCK_TEXT) {
        // Process text lines
        for (fz_stext_line *line = block->u.t.first_line; line; line = line->next) {
            std::string line_text;
            
            // Process individual characters
            for (fz_stext_char *ch = line->first_char; ch; ch = ch->next) {
                // Convert Unicode to UTF-8
                char utf8[5] = {0};
                int len = fz_runetochar(utf8, ch->c);
                line_text += utf8;
                
                // Extract position, font, size if needed
                if (options.extract_positions) {
                    char_json["bbox"] = quad_to_json(ch->quad);
                    char_json["origin_x"] = ch->origin.x;
                }
                if (options.extract_fonts) {
                    char_json["font"] = font_to_json(ch->font);
                    char_json["size"] = ch->size;
                }
            }
        }
    }
}
```

## 2. Thread Pool Implementation

### Thread Pool Class Definition (thread_pool.h:14-38)
```cpp
class ThreadPool {
public:
    explicit ThreadPool(size_t num_threads);
    ~ThreadPool();
    
    // Enqueue work with futures for results
    template<typename F, typename... Args>
    auto enqueue(F&& f, Args&&... args) 
        -> std::future<typename std::invoke_result<F, Args...>::type>;

private:
    std::vector<std::thread> workers;
    std::queue<std::function<void()>> tasks;
    
    mutable std::mutex queue_mutex;
    std::condition_variable condition;
    std::atomic<bool> stop{false};
    std::atomic<size_t> active_tasks{0};
};
```

### Thread Pool Enqueue Implementation (thread_pool.h:40-59)
```cpp
template<typename F, typename... Args>
auto ThreadPool::enqueue(F&& f, Args&&... args) 
    -> std::future<typename std::invoke_result<F, Args...>::type> {
    
    using return_type = typename std::invoke_result<F, Args...>::type;

    // Create packaged task
    auto task = std::make_shared<std::packaged_task<return_type()>>(
        std::bind(std::forward<F>(f), std::forward<Args>(args)...)
    );
    
    // Get future for result
    std::future<return_type> res = task->get_future();
    
    // Add to queue (thread-safe)
    {
        std::unique_lock<std::mutex> lock(queue_mutex);
        if(stop) {
            throw std::runtime_error("enqueue on stopped ThreadPool");
        }
        active_tasks++;
        tasks.emplace([task](){ (*task)(); });
    }
    
    // Wake up a worker thread
    condition.notify_one();
    return res;
}
```

## 3. Parallel PDF Processing

### Initializing Thread Pool (fast_pdf_parser.cpp:14-16)
```cpp
class FastPdfParser::Impl {
public:
    Impl(const ParseOptions& options) 
        : options_(options), 
          thread_pool_(options.thread_count) {  // Initialize with N threads
    }
```

### Parallel Page Processing with Batching (fast_pdf_parser.cpp:72-115)
```cpp
void parse_streaming(const std::string& pdf_path, PageCallback callback) {
    // Get total page count
    int page_count = extractor.get_page_count(pdf_path);
    
    // Process pages in parallel batches
    std::vector<std::future<PageResult>> futures;
    
    for (size_t i = 0; i < page_count; i += options_.batch_size) {
        size_t batch_end = std::min(i + options_.batch_size, 
                                   static_cast<size_t>(page_count));
        
        // Enqueue parallel tasks for batch
        for (size_t page_idx = i; page_idx < batch_end; ++page_idx) {
            futures.push_back(
                thread_pool_.enqueue([this, pdf_path, page_idx, extract_opts]() {
                    PageResult result;
                    result.page_number = page_idx;
                    
                    try {
                        // Each thread creates its own extractor
                        TextExtractor page_extractor;
                        result.content = page_extractor.extract_page(
                            pdf_path, page_idx, extract_opts
                        );
                        result.success = true;
                    } catch (const std::exception& e) {
                        result.error = e.what();
                        result.success = false;
                    }
                    
                    return result;
                })
            );
        }
        
        // Wait for batch completion
        bool should_continue = true;
        for (auto& future : futures) {
            if (should_continue) {
                should_continue = callback(future.get());
            } else {
                future.get();  // Still drain to avoid dangling threads
            }
        }
        futures.clear();
        
        // Early exit if callback returns false
        if (!should_continue) break;
    }
}
```

### Worker Thread Pattern (thread_pool.cpp - conceptual)
```cpp
// Each worker thread runs this loop
void worker_thread() {
    while (true) {
        std::function<void()> task;
        
        // Wait for and get task
        {
            std::unique_lock<std::mutex> lock(queue_mutex);
            condition.wait(lock, [this] { 
                return stop || !tasks.empty(); 
            });
            
            if (stop && tasks.empty()) return;
            
            task = std::move(tasks.front());
            tasks.pop();
        }
        
        // Execute task outside lock
        task();
        active_tasks--;
        finished.notify_all();
    }
}
```

## 4. Key Design Patterns

### Pattern 1: Thread-Safe MuPDF Usage
- **Each thread gets its own MuPDF context** (TextExtractor instance)
- No sharing of MuPDF objects between threads
- This avoids thread safety issues with MuPDF

### Pattern 2: Batch Processing
```cpp
// Process in batches to control memory usage
for (size_t i = 0; i < total_items; i += batch_size) {
    // Enqueue batch of work
    for (size_t j = i; j < min(i + batch_size, total_items); ++j) {
        futures.push_back(thread_pool.enqueue(work_function));
    }
    // Wait for batch before continuing
    for (auto& future : futures) {
        process_result(future.get());
    }
    futures.clear();
}
```

### Pattern 3: Future-Based Result Collection
```cpp
// Enqueue work and get future
auto future = thread_pool.enqueue([](){ 
    return do_work(); 
});

// Later, get result (blocks if not ready)
auto result = future.get();
```

### Pattern 4: Early Exit with Callback
```cpp
bool should_continue = callback(result);
if (!should_continue) {
    // Stop processing but still clean up remaining futures
    for (auto& f : remaining_futures) {
        f.get();  // Drain to avoid dangling threads
    }
    break;
}
```

## 5. Performance Characteristics

### Parallelization Strategy
1. **Thread Pool Size**: Default to `std::thread::hardware_concurrency()`
2. **Batch Size**: Configurable, controls memory usage
3. **Work Distribution**: Pages processed independently
4. **Result Collection**: Futures allow async result gathering

### Memory Management
- Each thread allocates its own MuPDF context
- Pages processed and freed individually
- Batch processing prevents excessive memory use
- `FZ_STORE_UNLIMITED` allows MuPDF to cache aggressively

### Scalability
- Linear scaling up to I/O or memory bandwidth limits
- 140+ pages/second achieved with 8 threads on medical PDFs
- Thread pool reuses threads to avoid creation overhead

## Usage Example

```cpp
// Configure parser
ParseOptions options;
options.thread_count = 8;  // 8 parallel threads
options.batch_size = 10;   // Process 10 pages at a time

// Create parser
FastPdfParser parser(options);

// Process PDF with streaming callback
parser.parse_streaming("document.pdf", [](const PageResult& result) {
    if (result.success) {
        process_page_data(result.content);
    }
    return true;  // Continue processing
});
```