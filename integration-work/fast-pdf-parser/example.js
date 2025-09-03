const { HierarchicalChunker } = require('./');
const path = require('path');

// Create a chunker with custom options
const chunker = new HierarchicalChunker({
    maxTokens: 512,      // Target chunk size
    minTokens: 150,      // Minimum chunk size
    overlapTokens: 0,    // No overlap between chunks
    threadCount: 4       // Use 4 threads
});

console.log('Chunker options:', chunker.getOptions());

// Chunk a PDF file
const pdfPath = path.join(__dirname, 'n3797.pdf');
console.log(`\nChunking ${pdfPath}...`);

const result = chunker.chunkFile(pdfPath, 20); // Process first 20 pages

console.log('\nResults:');
console.log(`  Pages processed: ${result.totalPages}`);
console.log(`  Chunks created: ${result.totalChunks}`);
console.log(`  Processing time: ${result.processingTimeMs}ms`);
console.log(`  Performance: ${(result.totalPages * 1000 / result.processingTimeMs).toFixed(1)} pages/second`);

// Show chunk statistics
if (result.chunks.length > 0) {
    const tokenCounts = result.chunks.map(c => c.tokenCount);
    const min = Math.min(...tokenCounts);
    const max = Math.max(...tokenCounts);
    const avg = tokenCounts.reduce((a, b) => a + b, 0) / tokenCounts.length;
    
    console.log('\nToken distribution:');
    console.log(`  Min: ${min} tokens`);
    console.log(`  Max: ${max} tokens`);
    console.log(`  Average: ${avg.toFixed(0)} tokens`);
    
    // Show first few chunks
    console.log('\nFirst 3 chunks:');
    result.chunks.slice(0, 3).forEach((chunk, i) => {
        console.log(`\nChunk ${i + 1}:`);
        console.log(`  Pages: ${chunk.startPage}-${chunk.endPage}`);
        console.log(`  Tokens: ${chunk.tokenCount}`);
        console.log(`  Has heading: ${chunk.hasMajorHeading}`);
        console.log(`  Text preview: "${chunk.text.substring(0, 100)}..."`);
    });
}