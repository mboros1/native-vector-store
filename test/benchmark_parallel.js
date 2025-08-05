const { VectorStore } = require('../index');
const path = require('path');
const fs = require('fs');

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
    
    // Create vector store with detected dimensions
    const store = new VectorStore(embeddingDim);
    
    // Benchmark loading
    console.log('📚 Loading documents from files...');
    const startTime = Date.now();
    
    store.loadDir(dataDir);
    
    const loadTime = Date.now() - startTime;
    const totalDocs = store.size();
    
    console.log(`✅ Loaded ${totalDocs} documents in ${loadTime}ms`);
    console.log(`   Average: ${(loadTime / totalDocs).toFixed(2)}ms per document`);
    console.log(`   Files processed: ${files.length}`);
    console.log(`   Documents per file: ${Math.round(totalDocs / files.length)}\n`);
    
    // Benchmark search
    console.log('🔍 Testing search performance...');
    const query = new Float32Array(embeddingDim).fill(0.5);
    
    const searchStart = Date.now();
    const results = store.search(query, 10);
    const searchTime = Date.now() - searchStart;
    
    console.log(`✅ Search completed in ${searchTime}ms`);
    console.log(`   Results found: ${results.length}`);
    if (results.length > 0) {
        console.log(`   Top result: ${results[0].id} (score: ${results[0].score.toFixed(4)})\n`);
    }
    
    // Performance summary
    console.log('📊 Performance Summary:');
    console.log('====================');
    console.log(`   Total documents: ${totalDocs}`);
    console.log(`   Load time: ${loadTime}ms`);
    console.log(`   Search time: ${searchTime}ms`);
    console.log(`   Load rate: ${Math.round(totalDocs / (loadTime / 1000))} docs/sec`);
    
    // Compare with target
    const targetLoadTime = totalDocs * 10; // 10ms per 1000 docs for 100k target
    console.log(`\n🎯 Performance vs Target:`);
    console.log(`   Load: ${loadTime}ms (target: <${targetLoadTime}ms) ${loadTime < targetLoadTime ? '✅' : '❌'}`);
    console.log(`   Search: ${searchTime}ms (target: <10ms) ${searchTime < 10 ? '✅' : '❌'}`);
}

runBenchmark().catch(console.error);