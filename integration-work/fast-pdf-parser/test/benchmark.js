const { HierarchicalChunker } = require('../');
const path = require('path');
const fs = require('fs');

console.log('Fast PDF Parser Benchmark\n');

// Test files
const testFiles = [
    'n3797.pdf',        // C++ standard document
    'test_pdfs/ml_paper_2024.pdf',
    'test_pdfs/scientific_review.pdf',
    'test_pdfs/technical_report.pdf',
    'test_pdfs/handbook_excerpt.pdf'
].map(f => path.join(__dirname, '..', f)).filter(fs.existsSync);

if (testFiles.length === 0) {
    console.error('No test PDF files found. Please add some PDFs to the project directory.');
    process.exit(1);
}

// Benchmark configurations
const configs = [
    { maxTokens: 256, name: 'Small chunks (256 tokens)' },
    { maxTokens: 512, name: 'Standard chunks (512 tokens)' },
    { maxTokens: 1024, name: 'Large chunks (1024 tokens)' }
];

// Thread count variations
const threadCounts = [1, 4, 8, 16];

async function runBenchmark() {
    console.log(`Found ${testFiles.length} test files\n`);
    
    for (const config of configs) {
        console.log(`\n=== ${config.name} ===`);
        console.log('Threads | File | Pages | Chunks | Time(ms) | Pages/sec');
        console.log('--------|------|-------|--------|----------|----------');
        
        for (const threads of threadCounts) {
            const chunker = new HierarchicalChunker({
                maxTokens: config.maxTokens,
                minTokens: Math.floor(config.maxTokens * 0.3),
                threadCount: threads
            });
            
            for (const file of testFiles) {
                const fileName = path.basename(file);
                const shortName = fileName.length > 20 ? 
                    fileName.substring(0, 17) + '...' : fileName;
                
                try {
                    const result = chunker.chunkFile(file, 100); // Limit to 100 pages for benchmark
                    const pagesPerSec = (result.totalPages * 1000 / result.processingTimeMs).toFixed(1);
                    
                    console.log(
                        `${threads.toString().padEnd(7)} | ` +
                        `${shortName.padEnd(20)} | ` +
                        `${result.totalPages.toString().padEnd(5)} | ` +
                        `${result.totalChunks.toString().padEnd(6)} | ` +
                        `${result.processingTimeMs.toString().padEnd(8)} | ` +
                        `${pagesPerSec}`
                    );
                } catch (err) {
                    console.error(`Error processing ${fileName}: ${err.message}`);
                }
            }
        }
    }
    
    // Token distribution analysis
    console.log('\n\n=== Token Distribution Analysis ===');
    const chunker = new HierarchicalChunker({ maxTokens: 512 });
    
    for (const file of testFiles.slice(0, 3)) { // Analyze first 3 files
        const fileName = path.basename(file);
        const result = chunker.chunkFile(file, 50); // First 50 pages
        
        if (result.chunks.length > 0) {
            const tokenCounts = result.chunks.map(c => c.tokenCount);
            const min = Math.min(...tokenCounts);
            const max = Math.max(...tokenCounts);
            const avg = tokenCounts.reduce((a, b) => a + b, 0) / tokenCounts.length;
            const variance = tokenCounts.reduce((sum, val) => sum + Math.pow(val - avg, 2), 0) / tokenCounts.length;
            const stdDev = Math.sqrt(variance);
            
            console.log(`\n${fileName}:`);
            console.log(`  Chunks: ${result.chunks.length}`);
            console.log(`  Min tokens: ${min}`);
            console.log(`  Max tokens: ${max}`);
            console.log(`  Avg tokens: ${avg.toFixed(1)}`);
            console.log(`  Std deviation: ${stdDev.toFixed(1)}`);
            console.log(`  Consistency: ${((1 - stdDev/avg) * 100).toFixed(1)}%`);
        }
    }
}

// Run the benchmark
runBenchmark().catch(console.error);