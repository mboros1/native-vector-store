#!/usr/bin/env node

const { VectorStore } = require('../');
const path = require('path');

console.log('Testing adaptive loader...\n');

const store = new VectorStore(1536);
const docsPath = path.join(__dirname, '../cpp_std_partitioned');

console.log(`Loading from ${docsPath} with adaptive strategy...`);
const start = Date.now();

// The adaptive loader will print stats to stderr
store.loadDirAdaptive(docsPath);

const elapsed = Date.now() - start;
console.log(`\nLoaded ${store.size()} documents in ${elapsed}ms`);
console.log('Finalized:', store.isFinalized());