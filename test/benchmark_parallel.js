const { VectorStore } = require('../index');
const path = require('path');
const fs = require('fs');
const { execSync } = require('child_process');

async function runBenchmark() {
    console.log('🚀 Native Vector Store Parallel Loading Benchmark');
    console.log('==============================================\n');
    
    // Try different possible data directories
    const possibleDirs = [
        path.join(__dirname, '..', 'test_data'),      // generate_test_data.js output
        path.join(__dirname, 'benchmark_data')        // create_benchmark_data.js output
    ];
    
    let dataDir = null;
    let files = [];
    
    for (const dir of possibleDirs) {
        if (fs.existsSync(dir)) {
            const dirFiles = fs.readdirSync(dir).filter(f => f.endsWith('.json'));
            if (dirFiles.length > 0) {
                dataDir = dir;
                files = dirFiles;
                break;
            }
        }
    }
    
    // Check if benchmark data exists
    if (!dataDir || files.length === 0) {
        console.log('❌ Benchmark data not found. Please run one of:');
        console.log('   node test/generate_test_data.js (for large dataset)');
        console.log('   node test/create_benchmark_data.js (for smaller dataset)');
        process.exit(1);
    }
    
    console.log(`📁 Found ${files.length} JSON files in ${path.relative(process.cwd(), dataDir)}\n`);
    
    // Auto-detect embedding dimensions from first document
    const firstFilePath = path.join(dataDir, files[0]);
    const firstFileData = JSON.parse(fs.readFileSync(firstFilePath, 'utf-8'));
    const firstDoc = Array.isArray(firstFileData) ? firstFileData[0] : firstFileData;
    const embeddingDim = firstDoc.metadata.embedding.length;
    
    console.log(`🔍 Auto-detected embedding dimensions: ${embeddingDim}`);
    
    // Count total documents
    let totalDocs = 0;
    let totalSize = 0;
    for (const file of files) {
        const filePath = path.join(dataDir, file);
        const stat = fs.statSync(filePath);
        totalSize += stat.size;
        
        // Sample first few files to estimate doc count
        if (totalDocs === 0 || files.indexOf(file) < 5) {
            const data = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
            totalDocs += Array.isArray(data) ? data.length : 1;
        }
    }
    
    // Estimate if we didn't count all
    if (files.length > 5) {
        totalDocs = Math.round(totalDocs * files.length / 5);
    }
    
    console.log(`📊 Dataset Statistics:`);
    console.log(`   Total files: ${files.length}`);
    console.log(`   Total size: ${(totalSize / 1024 / 1024).toFixed(2)} MB`);
    console.log(`   Estimated documents: ${totalDocs.toLocaleString()}\n`);
    
    // Build nvs-pack if needed
    const nvsPackPath = path.join(__dirname, '..', 'src', 'bin', 'nvs-pack');
    if (!fs.existsSync(nvsPackPath)) {
        console.log('⚙️  Building nvs-pack...');
        execSync('make -C src all', { stdio: 'inherit', cwd: path.join(__dirname, '..') });
    }
    
    // Create bundle
    const bundleDir = path.join(__dirname, 'benchmark_bundle');
    console.log('📦 Creating vector store bundle...');
    const startBundle = Date.now();
    
    // Remove old bundle directory if it exists
    if (fs.existsSync(bundleDir)) {
        fs.rmSync(bundleDir, { recursive: true, force: true });
    }
    
    try {
        execSync(`${nvsPackPath} --out ${bundleDir} ${dataDir}`, { stdio: 'pipe' });
    } catch (error) {
        console.error('Failed to create bundle:', error.message);
        process.exit(1);
    }
    
    const bundleTime = Date.now() - startBundle;
    console.log(`✅ Bundle created in ${bundleTime}ms\n`);
    
    // Load the bundle
    console.log('📥 Loading bundle into vector store...');
    const startLoad = Date.now();
    const store = new VectorStore(bundleDir);
    const loadTime = Date.now() - startLoad;
    
    console.log(`✅ Loaded ${store.size()} documents in ${loadTime}ms`);
    console.log(`   Throughput: ${(store.size() / (bundleTime / 1000)).toFixed(0)} documents/second`);
    console.log(`   Dimensions: ${store.dimensions()}\n`);
    
    // Run search benchmarks
    console.log('🔍 Running search benchmarks...\n');
    
    // Generate random query
    const query = new Float32Array(embeddingDim);
    for (let i = 0; i < embeddingDim; i++) {
        query[i] = Math.random() - 0.5;
    }
    // Normalize
    let sum = 0;
    for (let i = 0; i < embeddingDim; i++) {
        sum += query[i] * query[i];
    }
    const norm = Math.sqrt(sum);
    for (let i = 0; i < embeddingDim; i++) {
        query[i] /= norm;
    }
    
    // Warm-up
    console.log('🔥 Warming up...');
    for (let i = 0; i < 10; i++) {
        store.search(query, 10);
    }
    
    // Benchmark different k values
    const kValues = [1, 5, 10, 20, 50, 100];
    const iterations = 100;
    
    console.log('📈 Search Performance (averaged over 100 iterations):');
    console.log('┌────────┬──────────┬──────────┬──────────┐');
    console.log('│   k    │   Mean   │   Min    │   Max    │');
    console.log('├────────┼──────────┼──────────┼──────────┤');
    
    for (const k of kValues) {
        const times = [];
        
        for (let i = 0; i < iterations; i++) {
            const start = process.hrtime.bigint();
            const results = store.search(query, k);
            const end = process.hrtime.bigint();
            times.push(Number(end - start) / 1000000); // Convert to ms
        }
        
        const mean = times.reduce((a, b) => a + b, 0) / times.length;
        const min = Math.min(...times);
        const max = Math.max(...times);
        
        console.log(`│  ${k.toString().padEnd(5)} │ ${mean.toFixed(2).padStart(6)} ms │ ${min.toFixed(2).padStart(6)} ms │ ${max.toFixed(2).padStart(6)} ms │`);
    }
    console.log('└────────┴──────────┴──────────┴──────────┘\n');
    
    // Test BM25 search
    console.log('📝 Testing BM25 text search...');
    const textQuery = 'machine learning neural network';
    const startBM25 = process.hrtime.bigint();
    const bm25Results = store.searchBM25(textQuery, 10);
    const bm25Time = Number(process.hrtime.bigint() - startBM25) / 1000000;
    console.log(`✅ BM25 search completed in ${bm25Time.toFixed(2)}ms, found ${bm25Results.length} results\n`);
    
    // Test hybrid search
    console.log('🔀 Testing hybrid search...');
    const startHybrid = process.hrtime.bigint();
    const hybridResults = store.searchHybrid(query, textQuery, 10);
    const hybridTime = Number(process.hrtime.bigint() - startHybrid) / 1000000;
    console.log(`✅ Hybrid search completed in ${hybridTime.toFixed(2)}ms, found ${hybridResults.length} results\n`);
    
    // Summary
    console.log('📊 Summary:');
    console.log(`   Bundle creation: ${bundleTime}ms`);
    // Get total bundle size by summing all files in the bundle directory
    let bundleSize = 0;
    const bundleFiles = fs.readdirSync(bundleDir);
    for (const file of bundleFiles) {
        bundleSize += fs.statSync(path.join(bundleDir, file)).size;
    }
    console.log(`   Bundle size: ${(bundleSize / 1024 / 1024).toFixed(2)} MB`);
    console.log(`   Documents: ${store.size().toLocaleString()}`);
    console.log(`   Throughput: ${(store.size() / (bundleTime / 1000)).toFixed(0)} docs/sec`);
    console.log(`   Vector search (k=10): ${times[2].toFixed(2)}ms average`);
    console.log(`   BM25 search: ${bm25Time.toFixed(2)}ms`);
    console.log(`   Hybrid search: ${hybridTime.toFixed(2)}ms`);
    
    // Clean up
    store.close();
    fs.rmSync(bundleDir, { recursive: true, force: true });
    
    console.log('\n✅ Benchmark complete!');
}

// Run if executed directly
if (require.main === module) {
    runBenchmark().catch(console.error);
}

module.exports = { runBenchmark };