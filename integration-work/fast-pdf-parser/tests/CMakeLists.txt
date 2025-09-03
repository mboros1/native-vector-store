find_package(GTest REQUIRED CONFIG)

# Test executable
add_executable(fast_pdf_parser_tests
    test_parser.cpp
    test_thread_pool.cpp
    test_json_serializer.cpp
)

target_link_libraries(fast_pdf_parser_tests
    PRIVATE
        fastpdfparser
        GTest::gtest
        GTest::gtest_main
)

# Add tests
include(GoogleTest)
gtest_discover_tests(fast_pdf_parser_tests)

# Copy test data
file(COPY ${CMAKE_CURRENT_SOURCE_DIR}/test_data
     DESTINATION ${CMAKE_CURRENT_BINARY_DIR})