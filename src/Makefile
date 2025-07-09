CXX = clang++
CXXFLAGS = -std=c++17 -g -O0 -fno-omit-frame-pointer -DDEBUG
INCLUDES = -I. -I/opt/homebrew/opt/libomp/include -I/opt/homebrew/include
LDFLAGS = -L/opt/homebrew/opt/libomp/lib -L/opt/homebrew/lib
LIBS = -lomp -lsimdjson

# Add flags for better debugging with lldb
CXXFLAGS += -glldb -gdwarf-4

# OpenMP flags for macOS
CXXFLAGS += -Xpreprocessor -fopenmp

TARGET = test_vector_store
SOURCES = test_main.cpp
OBJECTS = $(SOURCES:.cpp=.o)

all: $(TARGET)

$(TARGET): $(OBJECTS)
	$(CXX) $(CXXFLAGS) $(OBJECTS) -o $(TARGET) $(LDFLAGS) $(LIBS)

%.o: %.cpp
	$(CXX) $(CXXFLAGS) $(INCLUDES) -c $< -o $@

clean:
	rm -f $(OBJECTS) $(TARGET)

run: $(TARGET)
	./$(TARGET)

debug: $(TARGET)
	lldb ./$(TARGET)

.PHONY: all clean run debug