#!/bin/bash

# Build prebuilds for current platform
echo "Building prebuilds for native-vector-store..."

# Clean previous builds
rm -rf prebuilds/

# Build for current platform and architecture
echo "Building for current platform..."
npx prebuildify --napi --strip

# For local testing on macOS, build both architectures if on Apple Silicon
if [[ "$OSTYPE" == "darwin"* ]] && [[ "$(uname -m)" == "arm64" ]]; then
    echo "Building universal binary for macOS..."
    npx prebuildify --napi --strip --arch x64
    npx prebuildify --napi --strip --arch arm64
fi

echo "Prebuilds created:"
find prebuilds -type f -name "*.node" | sort

echo "Done!"