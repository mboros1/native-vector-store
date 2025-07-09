#!/usr/bin/env node

require('dotenv').config();

const fs = require('fs');
const path = require('path');
const { createReadStream } = require('fs');
const { pipeline } = require('stream/promises');

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

async function addEmbeddingsToFile(inputFile, outputFile) {
  console.log(`📄 Processing file: ${inputFile}`);
  
  // Load OpenAI
  const openai = await loadOpenAI();
  
  // Read the JSON file
  const content = fs.readFileSync(inputFile, 'utf8');
  let documents;
  
  try {
    documents = JSON.parse(content);
  } catch (error) {
    console.error(`❌ Error parsing JSON from ${inputFile}:`, error.message);
    return;
  }
  
  if (!Array.isArray(documents)) {
    console.error(`❌ Expected array of documents in ${inputFile}`);
    return;
  }
  
  console.log(`📊 Found ${documents.length} documents to process`);
  
  // Process each document
  for (let i = 0; i < documents.length; i++) {
    const doc = documents[i];
    
    if (!doc.text) {
      console.warn(`⚠️  Document ${i} has no text field, skipping`);
      continue;
    }
    
    console.log(`🔄 Processing document ${i + 1}/${documents.length}...`);
    
    try {
      // Generate embedding for the text
      const embedding = await generateEmbedding(openai, doc.text);
      
      // Add embedding to document
      doc.id = `doc_${i}`;  // Add ID if not present
      doc.metadata = {
        embedding: embedding,
        embedding_model: 'text-embedding-3-small',
        embedding_dim: embedding.length,
        original_meta: doc.meta  // Preserve original metadata
      };
      
      console.log(`✅ Added embedding (dim: ${embedding.length}) to document ${i + 1}`);
      
      // Add small delay to avoid rate limiting
      if (i < documents.length - 1) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    } catch (error) {
      console.error(`❌ Failed to process document ${i}:`, error.message);
      // Continue with next document
    }
  }
  
  // Write the updated documents to output file
  fs.writeFileSync(outputFile, JSON.stringify(documents, null, 2));
  console.log(`\n✅ Successfully wrote ${documents.length} documents with embeddings to ${outputFile}`);
}

// Main function
async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 1) {
    console.log('Usage: node add_embeddings_single.js <input-file> [output-file]');
    console.log('Example: node add_embeddings_single.js out/README.json out/README_embedded.json');
    console.log('\nIf output-file is not specified, it will default to <input>_embedded.json');
    process.exit(1);
  }
  
  const inputFile = args[0];
  let outputFile = args[1];
  
  if (!outputFile) {
    const parsed = path.parse(inputFile);
    outputFile = path.join(parsed.dir, `${parsed.name}_embedded${parsed.ext}`);
  }
  
  if (!fs.existsSync(inputFile)) {
    console.error(`❌ Input file not found: ${inputFile}`);
    process.exit(1);
  }
  
  try {
    await addEmbeddingsToFile(inputFile, outputFile);
  } catch (error) {
    console.error('❌ Fatal error:', error);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}