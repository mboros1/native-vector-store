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
UNIT_TEST_TARGET = $(BIN_DIR)/unit_tests
TOKENIZER_TEST_TARGET = $(BIN_DIR)/test_tokenizer

# Source files
PACK_SOURCES = nvs_pack.cpp document_loader.cpp simple_tokenizer.cpp ../deps/simdjson/simdjson.cpp
# Unit test sources - includes production code with tests enabled
# Use doctest main from test/unit_test.cpp
UNIT_TEST_SOURCES = test/unit_test.cpp nvs_pack.cpp document_loader.cpp simple_tokenizer.cpp vector_store_v2.cpp ../deps/simdjson/simdjson.cpp
TOKENIZER_TEST_SOURCES = test/test_tokenizer.cpp simple_tokenizer.cpp
# Object files
PACK_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(PACK_SOURCES)))
TOKENIZER_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/%.o,$(notdir $(TOKENIZER_TEST_SOURCES)))
# Unit test objects need special naming to avoid conflicts
UNIT_TEST_OBJECTS = $(patsubst %.cpp,$(OBJ_DIR)/unit_%.o,$(notdir $(UNIT_TEST_SOURCES)))

all: $(BIN_DIR) $(OBJ_DIR) $(PACK_TARGET)

$(BIN_DIR):
	mkdir -p $(BIN_DIR)

$(OBJ_DIR):
	mkdir -p $(OBJ_DIR)

$(PACK_TARGET): $(BIN_DIR) $(OBJ_DIR) $(PACK_OBJECTS)
	$(CXX) $(CXXFLAGS) $(PACK_OBJECTS) -o $(PACK_TARGET) $(LDFLAGS) $(LIBS)


$(TOKENIZER_TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(TOKENIZER_TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(TOKENIZER_TEST_OBJECTS) -o $(TOKENIZER_TEST_TARGET) $(LDFLAGS) $(LIBS)


# Unit test target - compiles with tests enabled
# Extra test flags (e.g., -DNVS_TEST_V2_BUNDLE) can be passed via EXTRA_TEST_FLAGS
$(UNIT_TEST_TARGET): CXXFLAGS += -DNVS_ENABLE_INLINE_TESTS $(EXTRA_TEST_FLAGS)
$(UNIT_TEST_TARGET): $(BIN_DIR) $(OBJ_DIR) $(UNIT_TEST_OBJECTS)
	$(CXX) $(CXXFLAGS) $(UNIT_TEST_OBJECTS) -o $(UNIT_TEST_TARGET) $(LDFLAGS) $(LIBS)

# Pattern rules for different source directories
$(OBJ_DIR)/%.o: %.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@


# Doctest test files (no inline production tests by default)
$(OBJ_DIR)/%.o: test/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

$(OBJ_DIR)/%.o: ../deps/simdjson/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

# Special rules for unit test objects with unique prefix to enable inline tests
$(OBJ_DIR)/unit_%.o: %.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -DNVS_ENABLE_INLINE_TESTS $(EXTRA_TEST_FLAGS) -c $< -o $@

$(OBJ_DIR)/unit_%.o: test/%.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -DNVS_ENABLE_INLINE_TESTS $(EXTRA_TEST_FLAGS) -c $< -o $@

$(OBJ_DIR)/unit_simdjson.o: ../deps/simdjson/simdjson.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -DNVS_ENABLE_INLINE_TESTS $(EXTRA_TEST_FLAGS) -c $< -o $@

clean:
	rm -rf $(OBJ_DIR) $(BIN_DIR)

# Run unit tests with doctest
unit-tests: $(UNIT_TEST_TARGET)
	@echo "Running unit tests with doctest..."
	./$(UNIT_TEST_TARGET)

# Build tests but don't run (for CI)
build-tests: $(UNIT_TEST_TARGET)
	@echo "Tests built successfully"

test-tokenizer: $(TOKENIZER_TEST_TARGET)
	@echo "Running tokenizer unit tests..."
	./$(TOKENIZER_TEST_TARGET)

# Build tokenizer doctests with inline tests enabled
$(OBJ_DIR)/test_simple_tokenizer.o: simple_tokenizer.cpp | $(OBJ_DIR)
	$(CXX) $(CXXFLAGS) $(INCLUDES) -DNVS_ENABLE_INLINE_TESTS -c $< -o $@

# Override tokenizer objects to include inline tests
tokenizer-with-inline: CXXFLAGS += -DNVS_ENABLE_INLINE_TESTS
tokenizer-with-inline: $(BIN_DIR) $(OBJ_DIR) $(OBJ_DIR)/test_simple_tokenizer.o $(OBJ_DIR)/test_tokenizer.o
	$(CXX) $(CXXFLAGS) $(OBJ_DIR)/test_simple_tokenizer.o $(OBJ_DIR)/test_tokenizer.o -o $(TOKENIZER_TEST_TARGET) $(LDFLAGS) $(LIBS)

rebuild: clean all

.PHONY: unit-tests-bundle
unit-tests-bundle: EXTRA_TEST_FLAGS += -DNVS_TEST_V2_BUNDLE
unit-tests-bundle: unit-tests

.PHONY: unit-tests-pack
unit-tests-pack: EXTRA_TEST_FLAGS += -DNVS_TEST_PACK_BUNDLE
unit-tests-pack: unit-tests
