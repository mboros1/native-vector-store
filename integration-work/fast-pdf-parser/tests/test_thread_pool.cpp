#include <gtest/gtest.h>
#include <fast_pdf_parser/thread_pool.h>
#include <chrono>
#include <atomic>

TEST(ThreadPoolTest, BasicConstruction) {
    EXPECT_NO_THROW(fast_pdf_parser::ThreadPool pool(4));
}

TEST(ThreadPoolTest, SingleTask) {
    fast_pdf_parser::ThreadPool pool(2);
    
    auto future = pool.enqueue([]() { return 42; });
    EXPECT_EQ(future.get(), 42);
}

TEST(ThreadPoolTest, MultipleTasks) {
    fast_pdf_parser::ThreadPool pool(4);
    std::vector<std::future<int>> futures;
    
    for (int i = 0; i < 10; ++i) {
        futures.push_back(pool.enqueue([i]() { return i * i; }));
    }
    
    for (int i = 0; i < 10; ++i) {
        EXPECT_EQ(futures[i].get(), i * i);
    }
}

TEST(ThreadPoolTest, ConcurrentExecution) {
    fast_pdf_parser::ThreadPool pool(4);
    std::atomic<int> counter{0};
    std::vector<std::future<void>> futures;
    
    auto start = std::chrono::high_resolution_clock::now();
    
    // Submit tasks that sleep briefly
    for (int i = 0; i < 8; ++i) {
        futures.push_back(pool.enqueue([&counter]() {
            std::this_thread::sleep_for(std::chrono::milliseconds(50));
            counter++;
        }));
    }
    
    // Wait for all tasks
    for (auto& future : futures) {
        future.get();
    }
    
    auto end = std::chrono::high_resolution_clock::now();
    auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(end - start);
    
    EXPECT_EQ(counter, 8);
    // With 4 threads, 8 tasks of 50ms each should take ~100ms
    EXPECT_LT(duration.count(), 200);
}

TEST(ThreadPoolTest, WaitAll) {
    fast_pdf_parser::ThreadPool pool(2);
    std::atomic<int> completed{0};
    
    for (int i = 0; i < 5; ++i) {
        pool.enqueue([&completed]() {
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
            completed++;
        });
    }
    
    pool.wait_all();
    EXPECT_EQ(completed, 5);
}

TEST(ThreadPoolTest, QueueSize) {
    fast_pdf_parser::ThreadPool pool(1);
    
    // Block the pool with a long task
    pool.enqueue([]() {
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    });
    
    // Add more tasks
    for (int i = 0; i < 5; ++i) {
        pool.enqueue([]() {});
    }
    
    // Queue should have tasks waiting
    EXPECT_GT(pool.queue_size(), 0);
}

TEST(ThreadPoolTest, ExceptionHandling) {
    fast_pdf_parser::ThreadPool pool(2);
    
    auto future = pool.enqueue([]() {
        throw std::runtime_error("Test exception");
        return 42;
    });
    
    EXPECT_THROW(future.get(), std::runtime_error);
}

TEST(ThreadPoolTest, DifferentReturnTypes) {
    fast_pdf_parser::ThreadPool pool(2);
    
    auto int_future = pool.enqueue([]() { return 42; });
    auto string_future = pool.enqueue([]() { return std::string("hello"); });
    auto void_future = pool.enqueue([]() { /* no return */ });
    
    EXPECT_EQ(int_future.get(), 42);
    EXPECT_EQ(string_future.get(), "hello");
    EXPECT_NO_THROW(void_future.get());
}