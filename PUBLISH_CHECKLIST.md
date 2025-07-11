# NPM Publishing Checklist

## Pre-publish Steps

1. **Update package.json**
   - [ ] Set correct version number (follow semver)
   - [ ] Update author information
   - [ ] Verify license is correct

2. **Test Everything**
   ```bash
   npm run clean
   npm run build
   npm test
   ```

3. **Check what will be published**
   ```bash
   npm pack --dry-run
   ```
   This shows all files that will be included in the package.

4. **Verify native dependencies**
   - Ensure simdjson is documented as a system requirement
   - OpenMP requirements are clear

## Publishing

1. **Login to npm** (if not already)
   ```bash
   npm login
   ```

2. **Publish to npm**
   ```bash
   npm publish
   ```

   For first release or if the package name is taken:
   ```bash
   npm publish --access public
   ```

## Post-publish

1. **Verify installation works**
   ```bash
   # In a different directory
   npm install native-vector-store
   ```

2. **Test the published package**
   ```javascript
   const { VectorStore } = require('native-vector-store');
   const store = new VectorStore(1536);
   console.log('Success!');
   ```

## Important Notes

- The package requires system dependencies (simdjson, OpenMP)
- Users need to have build tools installed for compilation
- Consider using prebuildify for prebuilt binaries in the future

## System Requirements Documentation

Make sure README clearly states:
- C++17 compatible compiler required
- simdjson library must be installed
- OpenMP support required
- Node.js 14+ required