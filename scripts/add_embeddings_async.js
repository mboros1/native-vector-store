#!/usr/bin/env node

require('dotenv').config();

const fs = require('fs');
const path = require('path');

// Check if OpenAI API key is set
if (!process.env.OPENAI_API_KEY) {
  console.error('❌ Error: OPENAI_API_KEY environment variable is not set');
  console.error('Please set it in .env file or with: export OPENAI_API_KEY=your-api-key');
  process.exit(1);
}

// Configuration
const BATCH_SIZE = 5; // Number of files to process concurrently
const EMBEDDINGS_PER_REQUEST = 20; // Number of embeddings to generate per API call
const RATE_LIMIT_DELAY = 100; // Delay between API calls in ms

// Dynamic import for OpenAI (ES module)
async function loadOpenAI() {
  const { default: OpenAI } = await import('openai');
  return new OpenAI({
    apiKey: process.env.OPENAI_API_KEY
  });
}

async function generateEmbeddings(openai, texts, model = 'text-embedding-3-small') {
  try {
    const response = await openai.embeddings.create({
      model: model,
      input: texts,
    });
    return response.data.map(d => d.embedding);
  } catch (error) {
    console.error('Error generating embeddings:', error.message);
    throw error;
  }
}

async function processFileAsync(openai, inputFile, outputFile, fileIndex) {
  const startTime = Date.now();
  console.log(`📄 [File ${fileIndex}] Processing: ${path.basename(inputFile)}`);
  
  // Read the JSON file
  const content = fs.readFileSync(inputFile, 'utf8');
  let documents;
  
  try {
    documents = JSON.parse(content);
  } catch (error) {
    console.error(`❌ [File ${fileIndex}] Error parsing JSON:`, error.message);
    return { processed: 0, total: 0, time: 0 };
  }
  
  if (!Array.isArray(documents)) {
    console.error(`❌ [File ${fileIndex}] Expected array of documents`);
    return { processed: 0, total: 0, time: 0 };
  }
  
  console.log(`   [File ${fileIndex}] Found ${documents.length} documents`);
  
  let processedCount = 0;
  
  // Process documents in batches
  for (let i = 0; i < documents.length; i += EMBEDDINGS_PER_REQUEST) {
    const batch = documents.slice(i, Math.min(i + EMBEDDINGS_PER_REQUEST, documents.length));
    const texts = batch.map(doc => doc.text).filter(text => text);
    
    if (texts.length === 0) continue;
    
    try {
      // Generate embeddings for the batch
      const embeddings = await generateEmbeddings(openai, texts);
      
      // Assign embeddings to documents
      let embIndex = 0;
      for (let j = 0; j < batch.length; j++) {
        if (batch[j].text) {
          const docIndex = i + j;
          documents[docIndex].id = `doc_${fileIndex}_${docIndex}`;
          documents[docIndex].metadata = {
            embedding: embeddings[embIndex],
            embedding_model: 'text-embedding-3-small',
            embedding_dim: embeddings[embIndex].length,
            original_meta: documents[docIndex].meta
          };
          embIndex++;
          processedCount++;
        }
      }
      
      // Rate limiting delay
      if (i + EMBEDDINGS_PER_REQUEST < documents.length) {
        await new Promise(resolve => setTimeout(resolve, RATE_LIMIT_DELAY));
      }
      
    } catch (error) {
      console.error(`   ❌ [File ${fileIndex}] Error processing batch starting at ${i}:`, error.message);
      if (error.message.includes('rate limit')) {
        console.log(`   ⏸️  [File ${fileIndex}] Rate limit hit, waiting 60 seconds...`);
        await new Promise(resolve => setTimeout(resolve, 60000));
        i -= EMBEDDINGS_PER_REQUEST; // Retry this batch
      }
    }
  }
  
  // Write the updated documents to output file
  fs.writeFileSync(outputFile, JSON.stringify(documents, null, 2));
  
  const elapsedTime = ((Date.now() - startTime) / 1000).toFixed(1);
  console.log(`✅ [File ${fileIndex}] Completed: ${processedCount}/${documents.length} docs in ${elapsedTime}s`);
  
  return { processed: processedCount, total: documents.length, time: elapsedTime };
}

async function processDirectoryAsync(inputDir, outputDir) {
  console.log('🚀 Async Batch Embedding Generator');
  console.log('==================================');
  console.log(`📊 Batch size: ${BATCH_SIZE} files concurrently`);
  console.log(`📊 Embeddings per API call: ${EMBEDDINGS_PER_REQUEST}\n`);
  
  // Create output directory if it doesn't exist
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
    console.log(`📁 Created output directory: ${outputDir}\n`);
  }
  
  // Load OpenAI
  const openai = await loadOpenAI();
  
  // Get all JSON files
  const files = fs.readdirSync(inputDir)
    .filter(f => f.endsWith('.json'))
    .sort();
  
  if (files.length === 0) {
    console.error('❌ No JSON files found in input directory');
    return;
  }
  
  console.log(`📊 Found ${files.length} JSON files to process\n`);
  
  const startTime = Date.now();
  let totalProcessed = 0;
  let totalDocs = 0;
  
  // Process files in batches
  for (let i = 0; i < files.length; i += BATCH_SIZE) {
    const batch = files.slice(i, Math.min(i + BATCH_SIZE, files.length));
    
    console.log(`\n🔄 Processing batch ${Math.floor(i / BATCH_SIZE) + 1}/${Math.ceil(files.length / BATCH_SIZE)}`);
    console.log(`   Files: ${batch.join(', ')}\n`);
    
    // Process batch files concurrently
    const promises = batch.map((file, index) => {
      const inputFile = path.join(inputDir, file);
      const outputFile = path.join(outputDir, file);
      const fileIndex = i + index;
      return processFileAsync(openai, inputFile, outputFile, fileIndex);
    });
    
    const results = await Promise.all(promises);
    
    // Aggregate results
    results.forEach(result => {
      totalProcessed += result.processed;
      totalDocs += result.total;
    });
    
    console.log(`\n📈 Batch complete: ${i + batch.length}/${files.length} files processed`);
  }
  
  const totalTime = ((Date.now() - startTime) / 1000).toFixed(1);
  
  // Summary
  console.log('\n✅ Processing Complete!');
  console.log('======================');
  console.log(`   Files processed: ${files.length}`);
  console.log(`   Documents with embeddings: ${totalProcessed}/${totalDocs}`);
  console.log(`   Total time: ${totalTime}s`);
  console.log(`   Average: ${(totalTime / files.length).toFixed(1)}s per file`);
  console.log(`   Output directory: ${outputDir}`);
}

// Main function
async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 1) {
    console.log('Usage: node add_embeddings_async.js <input-dir> [output-dir]');
    console.log('Example: node add_embeddings_async.js out/ out_embedded/');
    console.log('\nIf output-dir is not specified, it will default to <input-dir>_embedded/');
    process.exit(1);
  }
  
  const inputDir = args[0];
  let outputDir = args[1];
  
  if (!outputDir) {
    outputDir = inputDir + '_embedded';
  }
  
  if (!fs.existsSync(inputDir)) {
    console.error(`❌ Input directory not found: ${inputDir}`);
    process.exit(1);
  }
  
  try {
    await processDirectoryAsync(inputDir, outputDir);
  } catch (error) {
    console.error('❌ Fatal error:', error);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}