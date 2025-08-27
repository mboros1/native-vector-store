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
    INCLUDES = -I. -I../deps/simdjson -I../deps/atomic_queue -I../deps -I../deps/xxhash -I/opt/homebrew/opt/libomp/include
    LDFLAGS = -L/opt/homebrew/opt/libomp/lib
    LIBS = -lomp
    # Add flags for better debugging with lldb
    CXXFLAGS += -glldb -gdwarf-4
    # OpenMP flags for macOS
    CXXFLAGS += -Xpreprocessor -fopenmp
else
    # Linux configuration
    INCLUDES = -I. -I../deps/simdjson -I../deps/atomic_queue -I../deps -I../deps/xxhash
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

# Source files
PACK_SOURCES = nvs_pack.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
BUNDLE_TEST_SOURCES = test/test_bundle.cpp ../deps/simdjson/simdjson.cpp
TEST_SOURCES = test/test_vector_store_v2.cpp vector_store_v2.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
STRESS_SOURCES = test/test_stress_v2.cpp vector_store_v2.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
COMP_SOURCES = test/test_comprehensive_v2.cpp vector_store_v2.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
# Object files
PACK_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(PACK_SOURCES)))
BUNDLE_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(BUNDLE_TEST_SOURCES)))
TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(TEST_SOURCES)))
STRESS_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(STRESS_SOURCES)))
COMP_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(COMP_SOURCES)))

all: $(BIN_DIR) $(OBJ_DIR) $(PACK_TARGET) $(BUNDLE_TEST_TARGET) $(TEST_TARGET) $(STRESS_TARGET) $(COMP_TARGET)

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

# Pattern rules for different source directories
$(OBJ_DIR)/%.o: %.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: test/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: ../deps/simdjson/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

clean:
	rm -rf $(OBJ_DIR) $(BIN_DIR)

# Test targets
test: $(TEST_TARGET)
	./$(TEST_TARGET) test-bundle

test-bundle: $(BUNDLE_TEST_TARGET)
	@echo "Usage: make test-bundle BUNDLE_DIR=<path-to-bundle>"
	@echo "Example: make test-bundle BUNDLE_DIR=./test-bundle"
	@if [ -n "$(BUNDLE_DIR)" ]; then \
		./$(BUNDLE_TEST_TARGET) $(BUNDLE_DIR); \
	else \
		echo "No BUNDLE_DIR specified. Use BUNDLE_DIR=<path> to test a bundle."; \
	fi

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