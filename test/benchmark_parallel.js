const { VectorStore } = require('../index');
const path = require('path');
const fs = require('fs');

async function runBenchmark() {
    console.log('🚀 Native Vector Store Parallel Loading Benchmark');
    console.log('==============================================\n');
    
    const dataDir = path.join(__dirname, 'benchmark_data');
    
    // Check if benchmark data exists
    if (!fs.existsSync(dataDir)) {
        console.log('❌ Benchmark data not found. Please run: node test/create_benchmark_data.js');
        process.exit(1);
    }
    
    // Count JSON files
    const files = fs.readdirSync(dataDir).filter(f => f.endsWith('.json'));
    console.log(`📁 Found ${files.length} JSON files in benchmark directory\n`);
    
    // Create vector store
    const store = new VectorStore(20);
    
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
    const query = new Float32Array(20).fill(0.5);
    
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