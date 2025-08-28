# Detect OS
UNAME_S := $(shell uname -s)

# Output directories
BIN_DIR = bin
OBJ_DIR = obj

# Compiler
ifeq ($(UNAME_S),Darwin)
    CXX = clang++
    CC = clang
else
    CXX = g++
    CC = gcc
endif

# Base flags
CXXFLAGS = -std=c++17 -g -O3 -march=native -fno-omit-frame-pointer
CFLAGS = -std=c11 -g -O3 -march=native -fno-omit-frame-pointer

# OS-specific configuration
ifeq ($(UNAME_S),Darwin)
    # macOS configuration
    INCLUDES = -I. -I../deps/simdjson -I../deps/atomic_queue -I../deps -I../deps/xxhash -I../deps/ctre -I../deps/doctest -I/opt/homebrew/opt/libomp/include
    LDFLAGS = -L/opt/homebrew/opt/libomp/lib
    LIBS = -lomp
    # Add flags for better debugging with lldb
    CXXFLAGS += -glldb -gdwarf-4
    # OpenMP flags for macOS
    CXXFLAGS += -Xpreprocessor -fopenmp
else
    # Linux configuration
    INCLUDES = -I. -I../deps/simdjson -I../deps/atomic_queue -I../deps -I../deps/xxhash -I../deps/ctre -I../deps/doctest
    LDFLAGS = -L/usr/lib
    LIBS = -lgomp
    # OpenMP flags for Linux
    CXXFLAGS += -fopenmp
endif

# Build targets
PACK_TARGET = $(BIN_DIR)/nvs-pack
BUNDLE_TEST_TARGET = $(BIN_DIR)/test_bundle
TEST_TARGET = $(BIN_DIR)/test_vector_store
STRESS_TARGET = $(BIN_DIR)/test_stress
COMP_TARGET = $(BIN_DIR)/test_comprehensive
SPLITTER_TEST_TARGET = $(BIN_DIR)/test_sentence_splitter
UNIT_TEST_TARGET = $(BIN_DIR)/unit_tests
TOKENIZER_TEST_TARGET = $(BIN_DIR)/test_tokenizer

# Source files
PACK_SOURCES = nvs_pack.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
BUNDLE_TEST_SOURCES = test/test_bundle.cpp ../deps/simdjson/simdjson.cpp
# Unit test sources - includes production code with tests enabled
UNIT_TEST_SOURCES = test_runner.cpp nvs_pack.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
TEST_SOURCES = test/test_vector_store_v2.cpp vector_store_v2.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
STRESS_SOURCES = test/test_stress_v2.cpp vector_store_v2.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
COMP_SOURCES = test/test_comprehensive_v2.cpp vector_store_v2.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
SPLITTER_TEST_SOURCES = test/test_sentence_splitter.cpp
TOKENIZER_TEST_SOURCES = test/test_tokenizer.cpp
# Object files
PACK_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(PACK_SOURCES)))
BUNDLE_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(BUNDLE_TEST_SOURCES)))
TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(TEST_SOURCES)))
STRESS_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(STRESS_SOURCES)))
COMP_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(COMP_SOURCES)))
SPLITTER_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(SPLITTER_TEST_SOURCES)))
TOKENIZER_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(TOKENIZER_TEST_SOURCES)))
# Unit test objects need special naming to avoid conflicts
UNIT_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/test_%.o,$(notdir $(UNIT_TEST_SOURCES)))

all: $(BIN_DIR) $(OBJ_DIR) $(PACK_TARGET) $(BUNDLE_TEST_TARGET) $(TEST_TARGET) $(STRESS_TARGET) $(COMP_TARGET) $(SPLITTER_TEST_TARGET)

$(BIN_DIR):
	mkdir -p $(BIN_DIR)

$(OBJ_DIR):
	mkdir -p $(OBJ_DIR)

$(PACK_TARGET): $(BIN_DIR) $(OBJ_DIR) $(PACK_OBJECTS)
	$(CXX) $(CXXFLAGS) $(PACK_OBJECTS) -o $(PACK_TARGET) $(LDFLAGS) $(LIBS)

$(BUNDLE_TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(BUNDLE_TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(BUNDLE_TEST_OBJECTS) -o $(BUNDLE_TEST_TARGET) $(LDFLAGS) $(LIBS)

$(TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(TEST_OBJECTS) -o $(TEST_TARGET) $(LDFLAGS) $(LIBS)

$(STRESS_TARGET): $(BIN_DIR) $(OBJ_DIR) $(STRESS_OBJECTS)
	$(CXX) $(CXXFLAGS) $(STRESS_OBJECTS) -o $(STRESS_TARGET) $(LDFLAGS) $(LIBS)

$(COMP_TARGET): $(BIN_DIR) $(OBJ_DIR) $(COMP_OBJECTS)
	$(CXX) $(CXXFLAGS) $(COMP_OBJECTS) -o $(COMP_TARGET) $(LDFLAGS) $(LIBS)

$(SPLITTER_TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(SPLITTER_TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(SPLITTER_TEST_OBJECTS) -o $(SPLITTER_TEST_TARGET) $(LDFLAGS) $(LIBS)

$(TOKENIZER_TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(TOKENIZER_TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(TOKENIZER_TEST_OBJECTS) -o $(TOKENIZER_TEST_TARGET) $(LDFLAGS) $(LIBS)

# Unit test target - compiles with tests enabled
$(UNIT_TEST_TARGET): CXXFLAGS += -DNVS_ENABLE_INLINE_TESTS
$(UNIT_TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(UNIT_TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(UNIT_TEST_OBJECTS) -o $(UNIT_TEST_TARGET) $(LDFLAGS) $(LIBS)

# Pattern rules for different source directories
$(OBJ_DIR)/%.o: %.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: test/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: ../deps/simdjson/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

# Special rules for unit test objects with test prefix to avoid conflicts
$(OBJ_DIR)/test_%.o: %.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -DNVS_ENABLE_INLINE_TESTS -c $< -o $@

$(OBJ_DIR)/test_simdjson.o: ../deps/simdjson/simdjson.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -DNVS_ENABLE_INLINE_TESTS -c $< -o $@

clean:
	rm -rf $(OBJ_DIR) $(BIN_DIR)

# Test targets
test: $(TEST_TARGET)
	./$(TEST_TARGET) test-bundle

# Run unit tests with doctest
unit-tests: $(UNIT_TEST_TARGET)
	@echo "Running unit tests with doctest..."
	./$(UNIT_TEST_TARGET)

# Build tests but don't run (for CI)
build-tests: $(UNIT_TEST_TARGET)
	@echo "Tests built successfully"

test-bundle: $(BUNDLE_TEST_TARGET)
	@echo "Usage: make test-bundle BUNDLE_DIR=<path-to-bundle>"
	@echo "Example: make test-bundle BUNDLE_DIR=./test-bundle"
	@if [ -n "$(BUNDLE_DIR)" ]; then \
		./$(BUNDLE_TEST_TARGET) $(BUNDLE_DIR); \
	else \
		echo "No BUNDLE_DIR specified. Use BUNDLE_DIR=<path> to test a bundle."; \
	fi

test-splitter: $(SPLITTER_TEST_TARGET)
	./$(SPLITTER_TEST_TARGET)

test-tokenizer: $(TOKENIZER_TEST_TARGET)
	@echo "Running tokenizer unit tests..."
	./$(TOKENIZER_TEST_TARGET)

# Force rebuild (useful for CI)
rebuild: clean all

run: test

debug: $(TEST_TARGET)
	lldb ./$(TEST_TARGET)

# Sanitizer builds
test-asan: CXXFLAGS += -fsanitize=address -fno-sanitize-recover=all
test-asan: LDFLAGS += -fsanitize=address
test-asan: clean $(STRESS_TARGET)
	./$(STRESS_TARGET)

test-tsan: CXXFLAGS += -fsanitize=thread -fno-sanitize-recover=all
test-tsan: LDFLAGS += -fsanitize=thread
test-tsan: clean $(STRESS_TARGET)
	./$(STRESS_TARGET)

# Run stress tests with sanitizers by default
# Use SANITIZER=none to disable, SANITIZER=thread for TSan
SANITIZER ?= address

ifeq ($(SANITIZER),address)
stress: test-asan
else ifeq ($(SANITIZER),thread)
stress: test-tsan
else
stress: $(STRESS_TARGET)
	./$(STRESS_TARGET)
endif

.PHONY: all clean run debug test test-asan test-tsan stress