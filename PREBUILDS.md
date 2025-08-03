# Prebuilt Binaries

This package includes prebuilt binaries for common platforms to make installation easier. Users don't need build tools or system dependencies when installing from npm if their platform is supported.

## Supported Platforms

Prebuilds are automatically created for:
- **Linux**: x64, arm64
- **macOS**: x64, arm64 (including Apple Silicon)
- **Windows**: x64

## How It Works

1. When users run `npm install native-vector-store`, the install script uses `node-gyp-build`
2. `node-gyp-build` checks if a prebuild exists for the current platform
3. If found, it uses the prebuild (fast, no compilation needed)
4. If not found, it falls back to building from source

## Building Prebuilds

### Locally (for current platform)
```bash
npm run prebuildify
```

### For all platforms (using GitHub Actions)
1. Push a tag starting with 'v' (e.g., v0.1.0)
2. GitHub Actions will automatically build for all platforms
3. Prebuilds will be attached to the GitHub release

### Manual trigger
You can also manually trigger the prebuild workflow from the Actions tab on GitHub.

## Including Prebuilds in npm Package

The prebuilds are automatically included when you run `npm publish`. The directory structure is:
```
native-vector-store/
├── prebuilds/
│   ├── linux-x64/
│   ├── linux-arm64/
│   ├── darwin-x64/
│   ├── darwin-arm64/
│   └── win32-x64/
└── ... other files
```

## Fallback Behavior

If a prebuild isn't available, users will need:
- C++17 compatible compiler
- simdjson library
- OpenMP support
- Python and build tools

## Testing Prebuilds

After building prebuilds:
```bash
# Test that it loads correctly
node -e "console.log(require('.'))"
```

## Troubleshooting

If prebuilds aren't working:
1. Check that `node-gyp-build` is in dependencies (not devDependencies)
2. Ensure prebuilds/ directory is not in .npmignore
3. Verify the binary names match node-gyp-build expectations