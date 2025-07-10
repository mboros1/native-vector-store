# Detect OS
UNAME_S := $(shell uname -s)

# Compiler
ifeq ($(UNAME_S),Darwin)
    CXX = clang++
else
    CXX = g++
endif

# Base flags
CXXFLAGS = -std=c++17 -g -O0 -fno-omit-frame-pointer -DDEBUG

# OS-specific configuration
ifeq ($(UNAME_S),Darwin)
    # macOS configuration
    INCLUDES = -I. -I/opt/homebrew/opt/libomp/include -I/opt/homebrew/include
    LDFLAGS = -L/opt/homebrew/opt/libomp/lib -L/opt/homebrew/lib
    LIBS = -lomp -lsimdjson
    # Add flags for better debugging with lldb
    CXXFLAGS += -glldb -gdwarf-4
    # OpenMP flags for macOS
    CXXFLAGS += -Xpreprocessor -fopenmp
else
    # Linux configuration
    INCLUDES = -I. -I/usr/include
    LDFLAGS = -L/usr/lib
    LIBS = -lgomp -lsimdjson
    # OpenMP flags for Linux
    CXXFLAGS += -fopenmp
endif

TARGET = test_vector_store
STRESS_TARGET = test_stress
SOURCES = test_main.cpp
STRESS_SOURCES = test_stress.cpp
OBJECTS = $(SOURCES:.cpp=.o)
STRESS_OBJECTS = $(STRESS_SOURCES:.cpp=.o)

all: $(TARGET) $(STRESS_TARGET)

$(TARGET): $(OBJECTS)
	$(CXX) $(CXXFLAGS) $(OBJECTS) -o $(TARGET) $(LDFLAGS) $(LIBS)

$(STRESS_TARGET): $(STRESS_OBJECTS)
	$(CXX) $(CXXFLAGS) $(STRESS_OBJECTS) -o $(STRESS_TARGET) $(LDFLAGS) $(LIBS)

%.o: %.cpp
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

clean:
	rm -f $(OBJECTS) $(STRESS_OBJECTS) $(TARGET) $(STRESS_TARGET)

# Force rebuild (useful for CI)
rebuild: clean all

run: $(TARGET)
	./$(TARGET)

debug: $(TARGET)
	lldb ./$(TARGET)

# Sanitizer builds
test-asan: CXXFLAGS += -fsanitize=address -fno-sanitize-recover=all
test-asan: LDFLAGS += -fsanitize=address
test-asan: clean $(STRESS_TARGET)
	./$(STRESS_TARGET)

test-tsan: CXXFLAGS += -fsanitize=thread -fno-sanitize-recover=all
test-tsan: LDFLAGS += -fsanitize=thread
test-tsan: clean $(STRESS_TARGET)
	./$(STRESS_TARGET)

# Run stress tests
stress: $(STRESS_TARGET)
	./$(STRESS_TARGET)

.PHONY: all clean run debug test-asan test-tsan stress