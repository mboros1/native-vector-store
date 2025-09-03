#include "fast_pdf_parser/thread_pool.h"

namespace fast_pdf_parser {

ThreadPool::ThreadPool(size_t num_threads) {
    for(size_t i = 0; i < num_threads; ++i) {
        workers.emplace_back([this] {
            for(;;) {
                std::function<void()> task;
                
                {
                    std::unique_lock<std::mutex> lock(this->queue_mutex);
                    this->condition.wait(lock, [this]{ 
                        return this->stop || !this->tasks.empty(); 
                    });
                    
                    if(this->stop && this->tasks.empty()) {
                        return;
                    }
                    
                    task = std::move(this->tasks.front());
                    this->tasks.pop();
                }
                
                task();
                
                active_tasks--;
                finished.notify_all();
            }
        });
    }
}

ThreadPool::~ThreadPool() {
    {
        std::unique_lock<std::mutex> lock(queue_mutex);
        stop = true;
    }
    
    condition.notify_all();
    
    for(std::thread &worker: workers) {
        worker.join();
    }
}

void ThreadPool::wait_all() {
    std::unique_lock<std::mutex> lock(queue_mutex);
    finished.wait(lock, [this]{ 
        return tasks.empty() && active_tasks == 0; 
    });
}

size_t ThreadPool::queue_size() const {
    std::unique_lock<std::mutex> lock(queue_mutex);
    return tasks.size();
}

size_t ThreadPool::active_threads() const {
    return active_tasks.load();
}

} // namespace fast_pdf_parser

// Unit tests
#ifdef ENABLE_TESTS
#include "../deps/doctest.h"
#include <chrono>
#include <atomic>

TEST_CASE("ThreadPool basic functionality") {
    using namespace fast_pdf_parser;
    using namespace std::chrono_literals;
    
    SUBCASE("Construction and destruction") {
        // Should construct and destruct without issues
        {
            ThreadPool pool(4);
        }
        CHECK(true); // If we got here, no crash occurred
    }
    
    SUBCASE("Single task execution") {
        ThreadPool pool(2);
        std::atomic<int> counter{0};
        
        auto future = pool.enqueue([&counter]() {
            counter++;
            return 42;
        });
        
        int result = future.get();
        CHECK(result == 42);
        CHECK(counter == 1);
    }
    
    SUBCASE("Multiple tasks execution") {
        ThreadPool pool(4);
        std::atomic<int> counter{0};
        std::vector<std::future<int>> futures;
        
        // Enqueue 10 tasks
        for (int i = 0; i < 10; ++i) {
            futures.push_back(pool.enqueue([&counter, i]() {
                counter++;
                return i * 2;
            }));
        }
        
        // Verify results
        for (int i = 0; i < 10; ++i) {
            CHECK(futures[i].get() == i * 2);
        }
        CHECK(counter == 10);
    }
    
    SUBCASE("wait_all functionality") {
        ThreadPool pool(2);
        std::atomic<int> counter{0};
        
        // Enqueue tasks with delays
        for (int i = 0; i < 5; ++i) {
            pool.enqueue([&counter]() {
                std::this_thread::sleep_for(10ms);
                counter++;
            });
        }
        
        pool.wait_all();
        CHECK(counter == 5);
    }
    
    SUBCASE("Queue size tracking") {
        ThreadPool pool(1); // Single thread to ensure queueing
        std::mutex blocker;
        blocker.lock();
        
        // First task will block
        pool.enqueue([&blocker]() {
            std::lock_guard<std::mutex> lock(blocker);
        });
        
        // These will queue
        pool.enqueue([]() {});
        pool.enqueue([]() {});
        
        // Allow time for first task to start
        std::this_thread::sleep_for(10ms);
        
        CHECK(pool.queue_size() >= 2);
        
        blocker.unlock(); // Let tasks complete
        pool.wait_all();
    }
}

TEST_CASE("ThreadPool exception handling") {
    using namespace fast_pdf_parser;
    
    SUBCASE("Task throwing exception") {
        ThreadPool pool(2);
        
        auto future = pool.enqueue([]() {
            throw std::runtime_error("Test exception");
            return 1;
        });
        
        CHECK_THROWS_AS(future.get(), std::runtime_error);
    }
    
    SUBCASE("Multiple exceptions") {
        ThreadPool pool(4);
        std::vector<std::future<int>> futures;
        
        for (int i = 0; i < 5; ++i) {
            futures.push_back(pool.enqueue([i]() {
                if (i % 2 == 0) {
                    throw std::runtime_error("Even number");
                }
                return i;
            }));
        }
        
        for (int i = 0; i < 5; ++i) {
            if (i % 2 == 0) {
                CHECK_THROWS(futures[i].get());
            } else {
                CHECK(futures[i].get() == i);
            }
        }
    }
}

TEST_CASE("ThreadPool performance characteristics") {
    using namespace fast_pdf_parser;
    using namespace std::chrono;
    
    SUBCASE("Parallel execution verification") {
        ThreadPool pool(4);
        auto start = high_resolution_clock::now();
        
        std::vector<std::future<void>> futures;
        // 4 tasks that each take 50ms should complete in ~50ms with 4 threads
        for (int i = 0; i < 4; ++i) {
            futures.push_back(pool.enqueue([]() {
                std::this_thread::sleep_for(milliseconds(50));
            }));
        }
        
        for (auto& f : futures) {
            f.get();
        }
        
        auto end = high_resolution_clock::now();
        auto duration = duration_cast<milliseconds>(end - start).count();
        
        // Should complete in less than 100ms if parallel (would be 200ms if serial)
        CHECK(duration < 100);
    }
}
#endif // ENABLE_TESTS