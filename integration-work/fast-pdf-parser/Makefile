# Fast PDF Parser Makefile

CXX = clang++
CXXFLAGS = -std=c++17 -O3 -Wall -Wextra -pthread \
           -I/opt/homebrew/include \
           -I/opt/homebrew/Cellar/mupdf-tools/1.26.3/include \
           -Iinclude

LDFLAGS = -L/opt/homebrew/lib \
          -L/opt/homebrew/Cellar/mupdf-tools/1.26.3/lib \
          -lmupdf -lmupdf-third -lz -pthread

# Directories
SRCDIR = src
OBJDIR = obj
BINDIR = bin
TESTDIR = tests
BENCHDIR = benchmarks
EXAMPLEDIR = examples

# Create directories
$(shell mkdir -p $(OBJDIR) $(BINDIR))

# Source files
SRCS = $(SRCDIR)/fast_pdf_parser.cpp \
       $(SRCDIR)/thread_pool.cpp \
       $(SRCDIR)/text_extractor.cpp \
       $(SRCDIR)/hierarchical_chunker.cpp \
       $(SRCDIR)/cl100k_base_data.cpp

# Object files
OBJS = $(patsubst $(SRCDIR)/%.cpp,$(OBJDIR)/%.o,$(SRCS))

# CLI specific objects
CLI_OBJS = $(OBJDIR)/chunk_pdf_cli.o

# Test objects (compiled with ENABLE_TESTS)
TEST_OBJS = $(OBJDIR)/test_runner.o \
            $(OBJDIR)/thread_pool_test.o \
            $(OBJDIR)/hierarchical_chunker_test.o

# Executables
TARGETS = $(BINDIR)/chunk-pdf-cli \
          $(BINDIR)/perf-test \
          $(BINDIR)/token-test \
          $(BINDIR)/benchmark-passes \
          $(BINDIR)/tokenizer-example

# Library
LIBRARY = $(BINDIR)/libhierarchicalchunker.a

# Default target
all: $(TARGETS) $(LIBRARY)

# Build object files
$(OBJDIR)/%.o: $(SRCDIR)/%.cpp
	@mkdir -p $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Build test object files
$(OBJDIR)/%_test.o: $(SRCDIR)/%.cpp
	@mkdir -p $(OBJDIR)
	$(CXX) $(CXXFLAGS) -DENABLE_TESTS -c $< -o $@

# Build test runner object
$(OBJDIR)/test_runner.o: $(SRCDIR)/test_runner.cpp
	@mkdir -p $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Build objects from other directories
$(OBJDIR)/%.o: $(TESTDIR)/%.cpp
	@mkdir -p $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(OBJDIR)/%.o: $(BENCHDIR)/%.cpp
	@mkdir -p $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

$(OBJDIR)/%.o: $(EXAMPLEDIR)/%.cpp
	@mkdir -p $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

# Main CLI program
$(BINDIR)/chunk-pdf-cli: $(OBJS) $(CLI_OBJS)
	@mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LDFLAGS)

# Test programs
$(BINDIR)/perf-test: $(OBJDIR)/fast_pdf_parser.o $(OBJDIR)/thread_pool.o $(OBJDIR)/text_extractor.o $(OBJDIR)/perf_test.o
	@mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LDFLAGS)

$(BINDIR)/token-test: $(OBJDIR)/token_test.o
	@mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LDFLAGS)

$(BINDIR)/benchmark-passes: $(OBJDIR)/benchmark_passes.o
	@mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LDFLAGS)

$(BINDIR)/tokenizer-example: $(OBJDIR)/tokenizer_example.o
	@mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LDFLAGS)

# Library target
$(LIBRARY): $(OBJS)
	@mkdir -p $(BINDIR)
	ar rcs $@ $^

# Test runner target
$(BINDIR)/test-runner: $(OBJDIR)/test_runner.o $(OBJDIR)/thread_pool_test.o \
                       $(OBJDIR)/hierarchical_chunker_test.o $(OBJDIR)/cl100k_base_data.o \
                       $(OBJDIR)/fast_pdf_parser.o $(OBJDIR)/text_extractor.o
	@mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LDFLAGS)

# Clean
clean:
	rm -rf $(OBJDIR) $(BINDIR) out/
	rm -f *.cmake *.sh
	rm -f cl100k_base.tiktoken
	rm -f test-runner chunk-pdf-cli perf-test token-test benchmark-passes tokenizer-example

# Run test
test: $(BINDIR)/chunk-pdf-cli
	$(BINDIR)/chunk-pdf-cli --input n3797.pdf --max-chunk-size 512 --min-chunk-size 150 --overlap 50 --page-limit 10

# Run unit tests
unit-test: $(BINDIR)/test-runner
	$(BINDIR)/test-runner

# Install (copy to system location)
install: $(BINDIR)/chunk-pdf-cli
	cp $(BINDIR)/chunk-pdf-cli /usr/local/bin/

.PHONY: all clean test unit-test install