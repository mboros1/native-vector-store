const { HierarchicalChunker, chunkPdf } = require('../');
const path = require('path');
const fs = require('fs');

console.log('Testing fast-pdf-parser Node.js bindings...');

// Test 1: Create chunker with options
console.log('\n1. Testing HierarchicalChunker constructor...');
try {
    const chunker = new HierarchicalChunker({
        maxTokens: 512,
        minTokens: 150,
        overlapTokens: 0,
        threadCount: 4
    });
    console.log('✓ Created HierarchicalChunker instance');
    
    // Test getOptions
    const options = chunker.getOptions();
    console.log('✓ Options:', options);
    
    // Test setOptions
    chunker.setOptions({ maxTokens: 600 });
    const newOptions = chunker.getOptions();
    console.log('✓ Updated maxTokens:', newOptions.maxTokens);
} catch (err) {
    console.error('✗ Error:', err.message);
}

// Test 2: Chunk a PDF file
console.log('\n2. Testing PDF chunking...');
const testPdf = path.join(__dirname, '..', 'n3797.pdf');

if (fs.existsSync(testPdf)) {
    try {
        const chunker = new HierarchicalChunker();
        console.log(`Chunking ${testPdf} (first 10 pages)...`);
        
        const result = chunker.chunkFile(testPdf, 10);
        
        console.log('✓ Chunking complete!');
        console.log(`  Total pages: ${result.totalPages}`);
        console.log(`  Total chunks: ${result.totalChunks}`);
        console.log(`  Processing time: ${result.processingTimeMs}ms`);
        console.log(`  Pages/second: ${(result.totalPages * 1000 / result.processingTimeMs).toFixed(1)}`);
        
        // Show first chunk
        if (result.chunks.length > 0) {
            const firstChunk = result.chunks[0];
            console.log('\n  First chunk:');
            console.log(`    Text preview: "${firstChunk.text.substring(0, 100)}..."`);
            console.log(`    Token count: ${firstChunk.tokenCount}`);
            console.log(`    Pages: ${firstChunk.startPage}-${firstChunk.endPage}`);
            console.log(`    Has heading: ${firstChunk.hasMajorHeading}`);
        }
        
        // Analyze token distribution
        console.log('\n  Token distribution:');
        const tokenCounts = result.chunks.map(c => c.tokenCount).sort((a, b) => a - b);
        console.log(`    Min: ${tokenCounts[0]}`);
        console.log(`    Max: ${tokenCounts[tokenCounts.length - 1]}`);
        console.log(`    Average: ${(tokenCounts.reduce((a, b) => a + b, 0) / tokenCounts.length).toFixed(0)}`);
        
    } catch (err) {
        console.error('✗ Error chunking PDF:', err.message);
    }
} else {
    console.log(`✗ Test PDF not found: ${testPdf}`);
}

// Test 3: Test convenience function
console.log('\n3. Testing convenience function...');
if (fs.existsSync(testPdf)) {
    try {
        console.log('Using chunkPdf() function...');
        const result = chunkPdf(testPdf, { 
            maxTokens: 400, 
            minTokens: 100,
            pageLimit: 5 
        });
        
        console.log('✓ Convenience function works!');
        console.log(`  Created ${result.totalChunks} chunks from ${result.totalPages} pages`);
        
    } catch (err) {
        console.error('✗ Error:', err.message);
    }
}

// Test 4: Error handling
console.log('\n4. Testing error handling...');
try {
    const chunker = new HierarchicalChunker();
    chunker.chunkFile('nonexistent.pdf');
    console.error('✗ Should have thrown an error');
} catch (err) {
    console.log('✓ Properly caught error:', err.message);
}

console.log('\nAll tests completed!');