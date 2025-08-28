#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

function printUsage() {
    console.log('Usage: view_bundle.js [bundle_dir] [file_type]');
    console.log('');
    console.log('View contents of Native Vector Store bundle files in human-readable format');
    console.log('');
    console.log('Arguments:');
    console.log('  bundle_dir  Path to bundle directory (default: current directory)');
    console.log('  file_type   Type of file to view:');
    console.log('              manifest   - Bundle manifest (JSON)');
    console.log('              terms      - Term dictionary');
    console.log('              lexicon    - Lexicon entries');
    console.log('              postings   - Posting lists');
    console.log('              meta       - Document metadata blocks');
    console.log('              meta_idx   - Metadata index');
    console.log('              doclen     - Document lengths');
    console.log('              vectors    - Vector dimensions (summary only)');
    console.log('');
    console.log('Examples:');
    console.log('  view_bundle.js samples/pmc_bundle_v2 manifest');
    console.log('  view_bundle.js . meta | less');
    console.log('  view_bundle.js samples/pmc_bundle_v2 terms | head -50');
}

function readManifest(bundleDir) {
    const manifestPath = path.join(bundleDir, 'manifest.json');
    if (!fs.existsSync(manifestPath)) {
        throw new Error(`No manifest.json found in ${bundleDir}`);
    }
    return JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
}

function viewManifest(bundleDir) {
    const manifest = readManifest(bundleDir);
    console.log(JSON.stringify(manifest, null, 2));
}

function viewTerms(bundleDir) {
    const termsPath = path.join(bundleDir, 'terms.dict');
    if (!fs.existsSync(termsPath)) {
        throw new Error('terms.dict not found');
    }
    
    const buffer = fs.readFileSync(termsPath);
    let offset = 0;
    let termId = 0;
    
    console.log('Term Dictionary');
    console.log('===============');
    console.log('ID\tTerm');
    console.log('---\t----');
    
    while (offset < buffer.length) {
        const nullIdx = buffer.indexOf(0, offset);
        if (nullIdx === -1) break;
        
        const term = buffer.toString('utf8', offset, nullIdx);
        console.log(`${termId}\t${term}`);
        
        termId++;
        offset = nullIdx + 1;
    }
    
    console.log(`\nTotal terms: ${termId}`);
}

function viewLexicon(bundleDir) {
    const lexPath = path.join(bundleDir, 'lexicon.bin');
    if (!fs.existsSync(lexPath)) {
        throw new Error('lexicon.bin not found');
    }
    
    const buffer = fs.readFileSync(lexPath);
    const numEntries = buffer.readUInt32LE(0);
    
    console.log('Lexicon Entries');
    console.log('===============');
    console.log(`Total entries: ${numEntries}`);
    console.log('');
    console.log('Term ID\tDoc Freq\tPostings Offset\tPostings Size');
    console.log('-------\t--------\t--------------\t-------------');
    
    let offset = 4;
    for (let i = 0; i < numEntries && i < 1000; i++) {  // Show first 1000
        const termId = buffer.readUInt32LE(offset);
        const docFreq = buffer.readUInt32LE(offset + 4);
        const postingsOffset = buffer.readBigUInt64LE(offset + 8);
        const postingsSize = buffer.readUInt32LE(offset + 16);
        
        console.log(`${termId}\t${docFreq}\t${postingsOffset}\t${postingsSize}`);
        offset += 20;
    }
    
    if (numEntries > 1000) {
        console.log(`... (${numEntries - 1000} more entries)`);
    }
}

function viewPostings(bundleDir) {
    const postPath = path.join(bundleDir, 'postings.bin');
    if (!fs.existsSync(postPath)) {
        throw new Error('postings.bin not found');
    }
    
    const buffer = fs.readFileSync(postPath);
    const fileSize = buffer.length;
    
    console.log('Postings File');
    console.log('=============');
    console.log(`File size: ${fileSize.toLocaleString()} bytes`);
    console.log('');
    console.log('Note: Postings are stored as compressed arrays of (doc_id, term_freq) pairs.');
    console.log('Use lexicon.bin to find specific posting list offsets.');
    console.log('');
    
    // Show a sample posting list (the first one)
    const lexPath = path.join(bundleDir, 'lexicon.bin');
    if (fs.existsSync(lexPath)) {
        const lexBuffer = fs.readFileSync(lexPath);
        const numEntries = lexBuffer.readUInt32LE(0);
        
        if (numEntries > 0) {
            // Read first lexicon entry
            const firstTermId = lexBuffer.readUInt32LE(4);
            const firstDocFreq = lexBuffer.readUInt32LE(8);
            const firstOffset = Number(lexBuffer.readBigUInt64LE(12));
            const firstSize = lexBuffer.readUInt32LE(20);
            
            console.log(`Sample: First posting list (term_id=${firstTermId}, doc_freq=${firstDocFreq})`);
            console.log('Doc ID\tTerm Freq');
            console.log('------\t---------');
            
            let offset = firstOffset;
            for (let i = 0; i < firstDocFreq && offset < firstOffset + firstSize; i++) {
                const docId = buffer.readUInt32LE(offset);
                const termFreq = buffer.readUInt32LE(offset + 4);
                console.log(`${docId}\t${termFreq}`);
                offset += 8;
                
                if (i >= 10) {
                    console.log(`... (${firstDocFreq - i - 1} more documents)`);
                    break;
                }
            }
        }
    }
}

function viewMetadata(bundleDir) {
    const manifest = readManifest(bundleDir);
    const metaPath = path.join(bundleDir, manifest.files.meta.path);
    
    if (!fs.existsSync(metaPath)) {
        throw new Error(`${manifest.files.meta.path} not found`);
    }
    
    const buffer = fs.readFileSync(metaPath);
    const blockSize = manifest.files.meta.block_size || 131072;
    
    // Read the block count first
    const blockCount = buffer.readUInt32LE(0);
    
    console.log('Metadata Blocks');
    console.log('===============');
    console.log(`Block size: ${blockSize.toLocaleString()} bytes`);
    console.log(`Doc aligned: ${manifest.files.meta.doc_aligned || false}`);
    console.log(`Total file size: ${buffer.length.toLocaleString()} bytes`);
    console.log(`Total blocks: ${blockCount}`);
    console.log('');
    
    // Read all block headers first
    const headers = [];
    let headerOffset = 4; // Start after block count
    for (let i = 0; i < blockCount; i++) {
        headers.push({
            blockId: buffer.readUInt32LE(headerOffset),
            uncompressedSize: buffer.readUInt32LE(headerOffset + 4),
            docCount: buffer.readUInt32LE(headerOffset + 8),
            padding: buffer.readUInt32LE(headerOffset + 12)
        });
        headerOffset += 16;
    }
    
    // Now read block data
    let dataOffset = 4 + (blockCount * 16); // Start after headers
    
    for (let blockIdx = 0; blockIdx < Math.min(3, blockCount); blockIdx++) {
        const header = headers[blockIdx];
        
        console.log(`\nBlock ${blockIdx}:`);
        console.log(`  Block ID: ${header.blockId}`);
        console.log(`  Uncompressed size: ${header.uncompressedSize.toLocaleString()} bytes`);
        console.log(`  Document count: ${header.docCount}`);
        console.log(`  Documents:`);
        
        let blockOffset = 0;
        for (let i = 0; i < Math.min(5, header.docCount); i++) {  // Show first 5 docs per block
            // Read DocHeader
            const docId = buffer.readBigUInt64LE(dataOffset + blockOffset);
            const timestamp = buffer.readBigUInt64LE(dataOffset + blockOffset + 8);
            const idLen = buffer.readUInt32LE(dataOffset + blockOffset + 16);
            const textLen = buffer.readUInt32LE(dataOffset + blockOffset + 20);
            const sourceLen = buffer.readUInt32LE(dataOffset + blockOffset + 24);
            
            blockOffset += 32; // DocHeader size
            
            // Read strings
            const id = buffer.toString('utf8', dataOffset + blockOffset, dataOffset + blockOffset + idLen);
            blockOffset += idLen;
            
            const textPreview = buffer.toString('utf8', dataOffset + blockOffset, dataOffset + blockOffset + Math.min(100, textLen));
            blockOffset += textLen;
            
            const source = buffer.toString('utf8', dataOffset + blockOffset, dataOffset + blockOffset + sourceLen);
            blockOffset += sourceLen;
            
            console.log(`    Doc ${docId}:`);
            console.log(`      ID: ${id}`);
            console.log(`      Timestamp: ${new Date(Number(timestamp) * 1000).toISOString()}`);
            console.log(`      Text size: ${textLen.toLocaleString()} bytes`);
            console.log(`      Text preview: ${textPreview}${textLen > 100 ? '...' : ''}`);
            console.log(`      Source: ${source}`);
        }
        
        if (header.docCount > 5) {
            console.log(`    ... (${header.docCount - 5} more documents in this block)`);
        }
        
        // Move to next block (blocks are padded to blockSize)
        dataOffset += blockSize;
    }
    
    if (blockCount > 3) {
        console.log(`\n... (${blockCount - 3} more blocks)`);
    }
}

function viewMetaIndex(bundleDir) {
    const manifest = readManifest(bundleDir);
    const idxPath = path.join(bundleDir, 'meta.idx');
    
    if (!fs.existsSync(idxPath)) {
        throw new Error('meta.idx not found');
    }
    
    const buffer = fs.readFileSync(idxPath);
    const numEntries = buffer.length / 16; // Each entry is 16 bytes
    
    console.log('Metadata Index');
    console.log('==============');
    console.log(`Total documents: ${numEntries}`);
    console.log('');
    
    // Parse schema if available
    const schema = manifest.files.meta_idx?.schema || 'u32 block_id, u32 offset, u32 doc_size';
    console.log(`Schema: ${schema}`);
    console.log('');
    
    console.log('Doc ID\tBlock ID\tOffset\t\tDoc Size');
    console.log('------\t--------\t------\t\t--------');
    
    for (let i = 0; i < Math.min(100, numEntries); i++) {
        const offset = i * 16;
        const blockId = buffer.readUInt32LE(offset);
        const blockOffset = buffer.readUInt32LE(offset + 4);
        const docSize = buffer.readUInt32LE(offset + 8);
        // padding at offset + 12
        
        console.log(`${i}\t${blockId}\t\t${blockOffset}\t\t${docSize}`);
    }
    
    if (numEntries > 100) {
        console.log(`... (${numEntries - 100} more entries)`);
    }
    
    // Show some statistics
    console.log('\nStatistics:');
    let totalSize = 0;
    let minSize = Infinity;
    let maxSize = 0;
    const blockCounts = new Map();
    
    for (let i = 0; i < numEntries; i++) {
        const offset = i * 16;
        const blockId = buffer.readUInt32LE(offset);
        const docSize = buffer.readUInt32LE(offset + 8);
        
        totalSize += docSize;
        minSize = Math.min(minSize, docSize);
        maxSize = Math.max(maxSize, docSize);
        
        blockCounts.set(blockId, (blockCounts.get(blockId) || 0) + 1);
    }
    
    console.log(`  Average doc size: ${Math.round(totalSize / numEntries).toLocaleString()} bytes`);
    console.log(`  Min doc size: ${minSize.toLocaleString()} bytes`);
    console.log(`  Max doc size: ${maxSize.toLocaleString()} bytes`);
    console.log(`  Total blocks used: ${blockCounts.size}`);
    console.log(`  Avg docs per block: ${Math.round(numEntries / blockCounts.size)}`);
}

function viewDocLen(bundleDir) {
    const doclenPath = path.join(bundleDir, 'doclen.u32');
    if (!fs.existsSync(doclenPath)) {
        throw new Error('doclen.u32 not found');
    }
    
    const buffer = fs.readFileSync(doclenPath);
    const numDocs = buffer.length / 4;
    
    console.log('Document Lengths');
    console.log('================');
    console.log(`Total documents: ${numDocs}`);
    console.log('');
    console.log('Doc ID\tLength');
    console.log('------\t------');
    
    // Show first 100
    for (let i = 0; i < Math.min(100, numDocs); i++) {
        const length = buffer.readUInt32LE(i * 4);
        console.log(`${i}\t${length}`);
    }
    
    if (numDocs > 100) {
        console.log(`... (${numDocs - 100} more documents)`);
    }
    
    // Calculate statistics
    let total = 0;
    let min = Infinity;
    let max = 0;
    
    for (let i = 0; i < numDocs; i++) {
        const length = buffer.readUInt32LE(i * 4);
        total += length;
        min = Math.min(min, length);
        max = Math.max(max, length);
    }
    
    console.log('\nStatistics:');
    console.log(`  Average length: ${Math.round(total / numDocs)}`);
    console.log(`  Min length: ${min}`);
    console.log(`  Max length: ${max}`);
    console.log(`  Total tokens: ${total.toLocaleString()}`);
}

function viewVectors(bundleDir) {
    const manifest = readManifest(bundleDir);
    const vectorsPath = path.join(bundleDir, 'vectors.f32');
    
    if (!fs.existsSync(vectorsPath)) {
        throw new Error('vectors.f32 not found');
    }
    
    const stats = fs.statSync(vectorsPath);
    const numDocs = manifest.num_docs;
    const dim = manifest.dim;
    const expectedSize = numDocs * dim * 4;
    
    console.log('Vector Embeddings');
    console.log('=================');
    console.log(`Number of documents: ${numDocs.toLocaleString()}`);
    console.log(`Embedding dimensions: ${dim}`);
    console.log(`Data type: float32`);
    console.log(`File size: ${stats.size.toLocaleString()} bytes`);
    console.log(`Expected size: ${expectedSize.toLocaleString()} bytes`);
    console.log(`Size per vector: ${(dim * 4).toLocaleString()} bytes`);
    console.log('');
    
    if (stats.size !== expectedSize) {
        console.log('WARNING: File size does not match expected size!');
    }
    
    // Show first few vectors (just dimensions, not all values)
    const buffer = fs.readFileSync(vectorsPath);
    console.log('Sample vectors (showing first and last 5 dimensions):');
    console.log('Doc ID\tFirst 5 dims\t\t\t\t\tLast 5 dims');
    console.log('------\t------------\t\t\t\t\t-----------');
    
    for (let i = 0; i < Math.min(10, numDocs); i++) {
        const offset = i * dim * 4;
        const first5 = [];
        const last5 = [];
        
        for (let j = 0; j < 5; j++) {
            first5.push(buffer.readFloatLE(offset + j * 4).toFixed(3));
        }
        
        for (let j = dim - 5; j < dim; j++) {
            last5.push(buffer.readFloatLE(offset + j * 4).toFixed(3));
        }
        
        console.log(`${i}\t[${first5.join(', ')}...]\t[...${last5.join(', ')}]`);
    }
    
    if (numDocs > 10) {
        console.log(`... (${numDocs - 10} more vectors)`);
    }
}

// Main CLI
function main() {
    const args = process.argv.slice(2);
    
    if (args.includes('-h') || args.includes('--help')) {
        printUsage();
        process.exit(0);
    }
    
    const bundleDir = args[0] || '.';
    const fileType = args[1];
    
    if (!fileType) {
        printUsage();
        process.exit(1);
    }
    
    if (!fs.existsSync(bundleDir)) {
        console.error(`Error: Directory ${bundleDir} does not exist`);
        process.exit(1);
    }
    
    try {
        switch (fileType.toLowerCase()) {
            case 'manifest':
                viewManifest(bundleDir);
                break;
            case 'terms':
                viewTerms(bundleDir);
                break;
            case 'lexicon':
                viewLexicon(bundleDir);
                break;
            case 'postings':
                viewPostings(bundleDir);
                break;
            case 'meta':
                viewMetadata(bundleDir);
                break;
            case 'meta_idx':
                viewMetaIndex(bundleDir);
                break;
            case 'doclen':
                viewDocLen(bundleDir);
                break;
            case 'vectors':
                viewVectors(bundleDir);
                break;
            default:
                console.error(`Error: Unknown file type '${fileType}'`);
                console.error('');
                printUsage();
                process.exit(1);
        }
    } catch (err) {
        console.error(`Error: ${err.message}`);
        process.exit(1);
    }
}

main();