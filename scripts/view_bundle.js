#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';

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
    if (!fs.existsSync(termsPath)) throw new Error('terms.dict not found');
    const buffer = fs.readFileSync(termsPath);
    let offset = 0, termId = 0;
    console.log('Term Dictionary (v2)');
    console.log('=====================');
    console.log('ID\tTerm');
    console.log('---\t----');
    while (offset + 4 <= buffer.length) {
        const len = buffer.readUInt32LE(offset); offset += 4;
        if (offset + len > buffer.length) break;
        const term = buffer.toString('utf8', offset, offset + len);
        offset += len;
        console.log(`${termId}\t${term}`);
        termId++;
    }
    console.log(`\nTotal terms: ${termId}`);
}

function viewLexicon(bundleDir) {
    const lexPath = path.join(bundleDir, 'lexicon.bin');
    if (!fs.existsSync(lexPath)) throw new Error('lexicon.bin not found');
    const buffer = fs.readFileSync(lexPath);
    if (buffer.length % 16 !== 0) console.warn('Warning: lexicon.bin size is not multiple of 16 bytes');
    const n = Math.floor(buffer.length / 16);
    console.log('Lexicon Entries (v2)');
    console.log('====================');
    console.log(`Total entries: ${n}`);
    console.log('');
    console.log('TermID\tDF\tOffset\tLength');
    console.log('-----\t--\t------\t------');
    let offset = 0;
    for (let i = 0; i < n && i < 1000; i++) {
        const postingsOffset = Number(buffer.readBigUInt64LE(offset));
        const length = buffer.readUInt32LE(offset + 8);
        const df = buffer.readUInt32LE(offset + 12);
        console.log(`${i}\t${df}\t${postingsOffset}\t${length}`);
        offset += 16;
    }
    if (n > 1000) console.log(`... (${n - 1000} more entries)`);
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
    console.log('Note: v2 postings are delta-encoded pairs (doc_delta, term_freq).');
    console.log('Use lexicon.bin to find offsets; sample below uses term_id=0.');
    console.log('');
    const lexPath = path.join(bundleDir, 'lexicon.bin');
    if (fs.existsSync(lexPath)) {
        const lex = fs.readFileSync(lexPath);
        const n = Math.floor(lex.length / 16);
        if (n > 0) {
            const sampleTermId = 0;
            const off = sampleTermId * 16;
            const postingsOffset = Number(lex.readBigUInt64LE(off));
            const length = lex.readUInt32LE(off + 8);
            const df = lex.readUInt32LE(off + 12);
            console.log(`Sample postings for term_id=${sampleTermId} (df=${df}, length=${length})`);
            console.log('Doc ID\tTerm Freq');
            console.log('------\t---------');
            let p = postingsOffset; let prev = 0;
            for (let i = 0; i < Math.min(length, 25); i++) {
                const delta = buffer.readUInt32LE(p);
                const tf = buffer.readUInt32LE(p + 4);
                const docId = prev + delta; prev = docId; p += 8;
                console.log(`${docId}\t${tf}`);
            }
            if (length > 25) console.log(`... (${length - 25} more)`);
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
    const blockCount = buffer.readUInt32LE(0);
    console.log('Metadata Blocks (v2)');
    console.log('====================');
    console.log(`Block size: ${blockSize.toLocaleString()} bytes`);
    console.log(`Total file size: ${buffer.length.toLocaleString()} bytes`);
    console.log(`Total blocks: ${blockCount}`);
    const headers = [];
    let headerOffset = 4;
    for (let i = 0; i < blockCount; i++) {
        headers.push({
            blockId: buffer.readUInt32LE(headerOffset),
            uncompressedSize: buffer.readUInt32LE(headerOffset + 4),
            docCount: buffer.readUInt32LE(headerOffset + 8),
            padding: buffer.readUInt32LE(headerOffset + 12)
        });
        headerOffset += 16;
    }
    let dataOffset = 4 + blockCount * 16;
    const inferredBlockSize = Math.floor((buffer.length - dataOffset) / blockCount);
    for (let blockIdx = 0; blockIdx < Math.min(3, blockCount); blockIdx++) {
        const header = headers[blockIdx];
        console.log(`\nBlock ${blockIdx} (id=${header.blockId}):`);
        console.log(`  Uncompressed size: ${header.uncompressedSize.toLocaleString()} bytes`);
        console.log(`  Document count: ${header.docCount}`);
        let pos = dataOffset + blockIdx * inferredBlockSize;
        let consumed = 0;
        for (let i = 0; i < Math.min(5, header.docCount); i++) {
            if (consumed + 4 > header.uncompressedSize) break;
            const idLen = buffer.readUInt32LE(pos); pos += 4; consumed += 4;
            const id = buffer.toString('utf8', pos, pos + idLen); pos += idLen; consumed += idLen;
            if (consumed + 4 > header.uncompressedSize) break;
            const textLen = buffer.readUInt32LE(pos); pos += 4; consumed += 4;
            const textPrev = buffer.toString('utf8', pos, pos + Math.min(100, textLen)); pos += textLen; consumed += textLen;
            if (consumed + 4 > header.uncompressedSize) break;
            const metaLen = buffer.readUInt32LE(pos); pos += 4; consumed += 4;
            const metaPrev = buffer.toString('utf8', pos, pos + Math.min(80, metaLen)); pos += metaLen; consumed += metaLen;
            console.log(`  Doc ${i}: id=${id} text(${textLen})='${textPrev}${textLen>100?'...':''}' meta(${metaLen})='${metaPrev}${metaLen>80?'...':''}'`);
        }
        if (header.docCount > 5) console.log(`  ... (${header.docCount - 5} more)`);
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
    const dtype = (manifest.embedding && manifest.embedding.dtype) || 'f32';
    const vectorsPath = path.join(bundleDir, `vectors.${dtype}`);
    if (!fs.existsSync(vectorsPath)) throw new Error(`vectors.${dtype} not found`);
    const stats = fs.statSync(vectorsPath);
    const numDocs = manifest.num_docs;
    const dim = manifest.dim;
    const elemSize = dtype === 'f16' ? 2 : 4;
    const rowSize = dim * elemSize;
    const alignedRowSize = Math.ceil(rowSize / 64) * 64;
    const expectedSize = numDocs * alignedRowSize;
    console.log('Vector Embeddings');
    console.log('=================');
    console.log(`Number of documents: ${numDocs.toLocaleString()}`);
    console.log(`Embedding dimensions: ${dim}`);
    console.log(`Data type: ${dtype}`);
    console.log(`Row size (aligned): ${alignedRowSize} bytes`);
    console.log(`Expected size: ${expectedSize.toLocaleString()} bytes`);
    console.log(`Actual size: ${stats.size.toLocaleString()} bytes`);
    if (stats.size !== expectedSize) console.log('WARNING: File size does not match expected aligned size!');
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
    const extra = args[2];
    
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
                viewPostings(bundleDir, extra ? parseInt(extra, 10) : 0);
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
