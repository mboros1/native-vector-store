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

// Dynamic import for OpenAI (ES module)
async function loadOpenAI() {
  const { default: OpenAI } = await import('openai');
  return new OpenAI({
    apiKey: process.env.OPENAI_API_KEY
  });
}

async function generateEmbedding(openai, text, model = 'text-embedding-3-small') {
  try {
    const response = await openai.embeddings.create({
      model: model,
      input: text,
    });
    return response.data[0].embedding;
  } catch (error) {
    console.error('Error generating embedding:', error.message);
    throw error;
  }
}

async function processFile(openai, inputFile, outputFile, fileIndex) {
  console.log(`\n📄 Processing file ${fileIndex}: ${path.basename(inputFile)}`);
  
  // Read the JSON file
  const content = fs.readFileSync(inputFile, 'utf8');
  let documents;
  
  try {
    documents = JSON.parse(content);
  } catch (error) {
    console.error(`❌ Error parsing JSON from ${inputFile}:`, error.message);
    return 0;
  }
  
  if (!Array.isArray(documents)) {
    console.error(`❌ Expected array of documents in ${inputFile}`);
    return 0;
  }
  
  console.log(`   Found ${documents.length} documents`);
  
  let processedCount = 0;
  
  // Process each document
  for (let i = 0; i < documents.length; i++) {
    const doc = documents[i];
    
    if (!doc.text) {
      console.warn(`   ⚠️  Document ${i} has no text field, skipping`);
      continue;
    }
    
    try {
      // Generate embedding for the text
      const embedding = await generateEmbedding(openai, doc.text);
      
      // Add embedding to document with global unique ID
      doc.id = `doc_${fileIndex}_${i}`;
      doc.metadata = {
        embedding: embedding,
        embedding_model: 'text-embedding-3-small',
        embedding_dim: embedding.length,
        original_meta: doc.meta  // Preserve original metadata
      };
      
      processedCount++;
      
      // Progress indicator
      if ((i + 1) % 10 === 0 || i === documents.length - 1) {
        console.log(`   ✅ Processed ${i + 1}/${documents.length} documents`);
      }
      
      // Add small delay to avoid rate limiting
      if (i < documents.length - 1) {
        await new Promise(resolve => setTimeout(resolve, 50)); // Reduced delay for batch processing
      }
    } catch (error) {
      console.error(`   ❌ Failed to process document ${i}:`, error.message);
      if (error.message.includes('rate limit')) {
        console.log('   ⏸️  Rate limit hit, waiting 60 seconds...');
        await new Promise(resolve => setTimeout(resolve, 60000));
        i--; // Retry this document
      }
    }
  }
  
  // Write the updated documents to output file
  fs.writeFileSync(outputFile, JSON.stringify(documents, null, 2));
  console.log(`   💾 Saved ${processedCount} documents to ${path.basename(outputFile)}`);
  
  return processedCount;
}

async function processDirectory(inputDir, outputDir) {
  console.log('🚀 Batch Embedding Generator');
  console.log('============================\n');
  
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
    .sort(); // Sort for consistent processing order
  
  if (files.length === 0) {
    console.error('❌ No JSON files found in input directory');
    return;
  }
  
  console.log(`📊 Found ${files.length} JSON files to process\n`);
  
  let totalDocs = 0;
  let totalProcessed = 0;
  let fileIndex = 0;
  
  // Process each file
  for (const file of files) {
    const inputFile = path.join(inputDir, file);
    const outputFile = path.join(outputDir, file);
    
    const processed = await processFile(openai, inputFile, outputFile, fileIndex);
    totalProcessed += processed;
    fileIndex++;
    
    // Progress update
    console.log(`📈 Progress: ${fileIndex}/${files.length} files completed\n`);
  }
  
  // Summary
  console.log('✅ Processing Complete!');
  console.log('======================');
  console.log(`   Files processed: ${files.length}`);
  console.log(`   Documents with embeddings: ${totalProcessed}`);
  console.log(`   Output directory: ${outputDir}`);
}

// Main function
async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 1) {
    console.log('Usage: node add_embeddings_dir.js <input-dir> [output-dir]');
    console.log('Example: node add_embeddings_dir.js out/ out_embedded/');
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
    await processDirectory(inputDir, outputDir);
  } catch (error) {
    console.error('❌ Fatal error:', error);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}