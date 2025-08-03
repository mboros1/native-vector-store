#!/usr/bin/env node

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

console.log('🔍 Checking package contents...\n');

// Generate docs if not already present
if (!fs.existsSync('docs')) {
  console.log('📚 Generating documentation...');
  execSync('npm run docs', { stdio: 'inherit' });
}

// Get package contents
console.log('\n📦 Package contents (npm pack --dry-run):');
console.log('=' .repeat(60));
const packOutput = execSync('npm pack --dry-run', { encoding: 'utf8' });
console.log(packOutput);

// Count documentation files
console.log('\n📄 Documentation statistics:');
console.log('=' .repeat(60));

const countFiles = (dir, pattern) => {
  let count = 0;
  const files = fs.readdirSync(dir, { withFileTypes: true });
  
  for (const file of files) {
    const fullPath = path.join(dir, file.name);
    if (file.isDirectory()) {
      count += countFiles(fullPath, pattern);
    } else if (pattern.test(file.name)) {
      count++;
    }
  }
  return count;
};

if (fs.existsSync('docs')) {
  const htmlFiles = countFiles('docs', /\.html$/);
  const cssFiles = countFiles('docs', /\.css$/);
  const jsFiles = countFiles('docs', /\.js$/);
  const totalFiles = countFiles('docs', /.*/);
  
  console.log(`HTML files: ${htmlFiles}`);
  console.log(`CSS files: ${cssFiles}`);
  console.log(`JavaScript files: ${jsFiles}`);
  console.log(`Total documentation files: ${totalFiles}`);
  
  // Get size
  const getDirSize = (dir) => {
    let size = 0;
    const files = fs.readdirSync(dir, { withFileTypes: true });
    
    for (const file of files) {
      const fullPath = path.join(dir, file.name);
      if (file.isDirectory()) {
        size += getDirSize(fullPath);
      } else {
        size += fs.statSync(fullPath).size;
      }
    }
    return size;
  };
  
  const docsSize = getDirSize('docs');
  console.log(`Documentation size: ${(docsSize / 1024 / 1024).toFixed(2)} MB`);
}

// List key documentation files
console.log('\n📑 Key documentation files:');
console.log('=' .repeat(60));
const keyDocs = [
  'docs/index.html',
  'docs/VectorStore.html',
  'docs/global.html',
  'README.md',
  'USAGE.md',
  'PERFORMANCE.md',
  'CLAUDE.md'
];

keyDocs.forEach(file => {
  if (fs.existsSync(file)) {
    const stats = fs.statSync(file);
    console.log(`✓ ${file} (${(stats.size / 1024).toFixed(1)} KB)`);
  } else {
    console.log(`✗ ${file} (not found)`);
  }
});

// Show what users will see
console.log('\n👁️  What users will see after installation:');
console.log('=' .repeat(60));
console.log('After running: npm install native-vector-store\n');
console.log('Documentation will be available at:');
console.log('  node_modules/native-vector-store/docs/index.html');
console.log('\nMarkdown documentation:');
console.log('  node_modules/native-vector-store/README.md');
console.log('  node_modules/native-vector-store/USAGE.md');
console.log('  node_modules/native-vector-store/PERFORMANCE.md');
console.log('\nAPI documentation:');
console.log('  node_modules/native-vector-store/docs/VectorStore.html');
console.log('  node_modules/native-vector-store/docs/global.html (type definitions)');