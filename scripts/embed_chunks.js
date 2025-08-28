#!/usr/bin/env node

const fs = require('fs').promises;
const path = require('path');
const { OpenAI } = require('openai');
require('dotenv').config();

// Initialize OpenAI client
const openai = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY
});

async function getEmbedding(text) {
  try {
    const response = await openai.embeddings.create({
      model: "text-embedding-3-small",
      input: text,
    });
    return response.data[0].embedding;
  } catch (error) {
    console.error(`Error getting embedding: ${error.message}`);
    return null;
  }
}

async function processChunkFile(chunkFilePath, outputDir) {
  const basename = path.basename(chunkFilePath, '_chunks.json');
  
  try {
    // Read the chunk file
    const content = await fs.readFile(chunkFilePath, 'utf8');
    const chunks = JSON.parse(content);
    
    if (!Array.isArray(chunks)) {
      console.error(`❌ ${basename}: not an array`);
      return { success: false, documents: 0 };
    }
    
    console.log(`📄 Processing ${basename}: ${chunks.length} chunks`);
    
    // Process each chunk
    const documents = [];
    let processedCount = 0;
    
    // Process chunks in batches to avoid rate limits
    const BATCH_SIZE = 10;
    
    for (let i = 0; i < chunks.length; i += BATCH_SIZE) {
      const batch = chunks.slice(i, Math.min(i + BATCH_SIZE, chunks.length));
      
      const batchPromises = batch.map(async (chunk, batchIdx) => {
        const chunkIndex = i + batchIdx;
        
        if (!chunk.text) {
          return null;
        }
        
        // Get embedding for the chunk text
        const embedding = await getEmbedding(chunk.text);
        if (!embedding) {
          console.error(`   ⚠️  Failed to get embedding for chunk ${chunkIndex}`);
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
      
      const batchResults = await Promise.all(batchPromises);
      const validDocs = batchResults.filter(doc => doc !== null);
      documents.push(...validDocs);
      processedCount += validDocs.length;
      
      // Progress update
      console.log(`   Processed ${processedCount}/${chunks.length} chunks...`);
      
      // Small delay between batches to avoid rate limits
      if (i + BATCH_SIZE < chunks.length) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    }
    
    // Write the output file
    const outputPath = path.join(outputDir, `${basename}.json`);
    await fs.writeFile(outputPath, JSON.stringify(documents, null, 2));
    
    console.log(`✅ ${basename}: ${documents.length} documents created`);
    
    return { success: true, documents: documents.length };
    
  } catch (error) {
    console.error(`❌ Error processing ${basename}: ${error.message}`);
    return { success: false, documents: 0 };
  }
}

async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 2) {
    console.log('Usage: node embed_chunks.js <input_chunks_dir> <output_dir>');
    console.log('Example: node embed_chunks.js samples/json samples/embedded_docs');
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
  
  console.log(`🚀 Processing ${chunkFiles.length} chunk files`);
  console.log(`   Input: ${inputDir}`);
  console.log(`   Output: ${outputDir}`);
  console.log(`   Model: text-embedding-3-small (1536 dimensions)\n`);
  
  const startTime = Date.now();
  let totalDocuments = 0;
  let successCount = 0;
  
  // Process files sequentially to avoid overwhelming the API
  for (const file of chunkFiles) {
    const inputPath = path.join(inputDir, file);
    const result = await processChunkFile(inputPath, outputDir);
    
    if (result.success) {
      successCount++;
      totalDocuments += result.documents;
    }
  }
  
  const totalTime = Math.round((Date.now() - startTime) / 1000);
  
  console.log(`\n✨ Processing complete!`);
  console.log(`   Successfully processed: ${successCount}/${chunkFiles.length} files`);
  console.log(`   Total documents created: ${totalDocuments}`);
  console.log(`   Time taken: ${totalTime}s`);
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