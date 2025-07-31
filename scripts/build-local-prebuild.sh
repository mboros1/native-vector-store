#!/bin/bash
# Build prebuild for current platform (for testing)

echo "Building prebuild for current platform..."

# Clean any existing prebuilds
rm -rf prebuilds/

# Build for current architecture
npx prebuildify --napi --strip

# List what was built
echo "Built prebuilds:"
find prebuilds -name "*.node" -type f

echo "Done! These files will be included when you run 'npm pack' or 'npm publish'"