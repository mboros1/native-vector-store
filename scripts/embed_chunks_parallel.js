#!/usr/bin/env node

const fs = require('fs').promises;
const path = require('path');
const { OpenAI } = require('openai');
require('dotenv').config();

// Configuration
const MAX_CONCURRENT_FILES = 10;  // Process 10 files at once
const MAX_CONCURRENT_EMBEDDINGS = 50;  // Up to 50 embedding API calls at once
const RATE_LIMIT_DELAY = 50;  // Small delay between batches (ms)

// Initialize OpenAI client
const openai = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY
});

// Semaphore for controlling concurrent API calls
class Semaphore {
  constructor(maxConcurrent) {
    this.maxConcurrent = maxConcurrent;
    this.current = 0;
    this.queue = [];
  }

  async acquire() {
    if (this.current >= this.maxConcurrent) {
      await new Promise(resolve => this.queue.push(resolve));
    }
    this.current++;
  }

  release() {
    this.current--;
    if (this.queue.length > 0) {
      const resolve = this.queue.shift();
      resolve();
    }
  }
}

const embeddingSemaphore = new Semaphore(MAX_CONCURRENT_EMBEDDINGS);

async function getEmbeddingWithRetry(text, retries = 3) {
  await embeddingSemaphore.acquire();
  
  try {
    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const response = await openai.embeddings.create({
          model: "text-embedding-3-small",
          input: text,
        });
        return response.data[0].embedding;
      } catch (error) {
        if (attempt === retries) {
          console.error(`   ⚠️  Failed after ${retries} attempts: ${error.message}`);
          return null;
        }
        // Exponential backoff
        await new Promise(resolve => setTimeout(resolve, Math.pow(2, attempt) * 1000));
      }
    }
  } finally {
    embeddingSemaphore.release();
  }
}

async function processChunkFile(chunkFilePath, outputDir, fileIndex, totalFiles) {
  const basename = path.basename(chunkFilePath, '_chunks.json');
  const startTime = Date.now();
  
  try {
    // Read the chunk file
    const content = await fs.readFile(chunkFilePath, 'utf8');
    const chunks = JSON.parse(content);
    
    if (!Array.isArray(chunks)) {
      console.error(`[${fileIndex}/${totalFiles}] ❌ ${basename}: not an array`);
      return { success: false, documents: 0 };
    }
    
    console.log(`[${fileIndex}/${totalFiles}] 📄 ${basename}: ${chunks.length} chunks`);
    
    // Process all chunks in parallel (controlled by semaphore)
    const documentPromises = chunks.map(async (chunk, chunkIndex) => {
      if (!chunk.text) {
        return null;
      }
      
      // Get embedding for the chunk text
      const embedding = await getEmbeddingWithRetry(chunk.text);
      if (!embedding) {
        return null;
      }
      
      // Create document with the nvs-pack expected format
      return {
        id: `${basename}_chunk_${chunkIndex}`,
        text: chunk.text,
        metadata: {
          embedding: embedding,
          source_file: basename,
          chunk_index: chunkIndex,
          original_meta: chunk.meta || {}
        }
      };
    });
    
    // Wait for all embeddings to complete
    const documents = (await Promise.all(documentPromises)).filter(doc => doc !== null);
    
    // Write the output file
    const outputPath = path.join(outputDir, `${basename}.json`);
    await fs.writeFile(outputPath, JSON.stringify(documents, null, 2));
    
    const processingTime = ((Date.now() - startTime) / 1000).toFixed(1);
    console.log(`[${fileIndex}/${totalFiles}] ✅ ${basename}: ${documents.length}/${chunks.length} docs in ${processingTime}s`);
    
    return { success: true, documents: documents.length, chunks: chunks.length };
    
  } catch (error) {
    console.error(`[${fileIndex}/${totalFiles}] ❌ ${basename}: ${error.message}`);
    return { success: false, documents: 0, chunks: 0 };
  }
}

async function processFilesBatch(files, inputDir, outputDir, startIndex, totalFiles) {
  const promises = files.map(async (file, index) => {
    const inputPath = path.join(inputDir, file);
    const result = await processChunkFile(inputPath, outputDir, startIndex + index, totalFiles);
    // Small delay between file starts to avoid thundering herd
    if (index > 0) {
      await new Promise(resolve => setTimeout(resolve, RATE_LIMIT_DELAY * index));
    }
    return result;
  });
  
  return await Promise.all(promises);
}

async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 2) {
    console.log('Usage: node embed_chunks_parallel.js <input_chunks_dir> <output_dir>');
    console.log('Example: node embed_chunks_parallel.js samples/json samples/embedded_docs');
    process.exit(1);
  }
  
  if (!process.env.OPENAI_API_KEY) {
    console.error('❌ Error: OPENAI_API_KEY environment variable not set');
    console.error('   Please set it in your .env file');
    process.exit(1);
  }
  
  const [inputDir, outputDir] = args;
  
  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });
  
  // Get all chunk files
  const files = await fs.readdir(inputDir);
  const chunkFiles = files.filter(f => f.endsWith('_chunks.json'));
  
  if (chunkFiles.length === 0) {
    console.log('❌ No chunk files found in input directory');
    process.exit(1);
  }
  
  console.log(`🚀 Processing ${chunkFiles.length} chunk files with parallel processing`);
  console.log(`   Input: ${inputDir}`);
  console.log(`   Output: ${outputDir}`);
  console.log(`   Model: text-embedding-3-small (1536 dimensions)`);
  console.log(`   Parallelism: ${MAX_CONCURRENT_FILES} files, ${MAX_CONCURRENT_EMBEDDINGS} embeddings\n`);
  
  const startTime = Date.now();
  let totalDocuments = 0;
  let totalChunks = 0;
  let successCount = 0;
  
  // Process files in batches
  for (let i = 0; i < chunkFiles.length; i += MAX_CONCURRENT_FILES) {
    const batch = chunkFiles.slice(i, Math.min(i + MAX_CONCURRENT_FILES, chunkFiles.length));
    const results = await processFilesBatch(batch, inputDir, outputDir, i + 1, chunkFiles.length);
    
    // Update statistics
    for (const result of results) {
      if (result.success) {
        successCount++;
        totalDocuments += result.documents;
        totalChunks += result.chunks;
      }
    }
    
    // Progress update
    const progress = Math.min(i + MAX_CONCURRENT_FILES, chunkFiles.length);
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    const rate = (totalDocuments / elapsed).toFixed(1);
    console.log(`\n⏱️  Progress: ${progress}/${chunkFiles.length} files | ${totalDocuments} docs | ${rate} docs/sec\n`);
  }
  
  const totalTime = Math.round((Date.now() - startTime) / 1000);
  const docsPerSecond = (totalDocuments / totalTime).toFixed(1);
  
  console.log(`\n✨ Processing complete!`);
  console.log(`   Successfully processed: ${successCount}/${chunkFiles.length} files`);
  console.log(`   Total chunks processed: ${totalChunks}`);
  console.log(`   Total documents created: ${totalDocuments}`);
  console.log(`   Success rate: ${((totalDocuments / totalChunks) * 100).toFixed(1)}%`);
  console.log(`   Time taken: ${totalTime}s`);
  console.log(`   Processing rate: ${docsPerSecond} docs/sec`);
  console.log(`   Output directory: ${outputDir}`);
  
  if (successCount > 0) {
    console.log(`\n📦 Next step: Create bundle with nvs-pack`);
    console.log(`   ./src/bin/nvs-pack --dim 1536 --model text-embedding-3-small ${outputDir}`);
  }
}

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}

module.exports = { processChunkFile };