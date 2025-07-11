#!/usr/bin/env node

const fs = require('fs').promises;
const path = require('path');
const { OpenAI } = require('openai');

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

async function processDocumentBatch(documents, startIdx, batchSize, basename) {
  const results = [];
  const endIdx = Math.min(startIdx + batchSize, documents.length);
  
  const promises = [];
  for (let i = startIdx; i < endIdx; i++) {
    const doc = documents[i];
    
    const processDoc = async () => {
      // Get the text to embed
      let textToEmbed = doc.text || doc.content || '';
      if (doc.meta && doc.meta.title) {
        textToEmbed = `${doc.meta.title}\n${textToEmbed}`;
      }
      
      if (!textToEmbed) {
        return null;
      }
      
      // Get embedding
      const embedding = await getEmbedding(textToEmbed);
      if (!embedding) {
        return null;
      }
      
      // Create document with required structure
      return {
        id: doc.id || `doc_${basename}_${i}`,
        text: textToEmbed.substring(0, 1000), // Limit text length for storage
        metadata: {
          embedding: embedding,
          ...doc.meta,
          original_length: textToEmbed.length
        }
      };
    };
    
    promises.push(processDoc());
  }
  
  const batchResults = await Promise.all(promises);
  return batchResults.filter(doc => doc !== null);
}

async function processJsonFile(inputPath, outputPath) {
  console.log(`\n📄 Processing: ${inputPath}`);
  
  try {
    // Read the input file
    const content = await fs.readFile(inputPath, 'utf8');
    const documents = JSON.parse(content);
    
    if (!Array.isArray(documents)) {
      console.error('❌ Input file must contain a JSON array of documents');
      return false;
    }
    
    console.log(`   Found ${documents.length} documents to process`);
    
    // Process documents in concurrent batches
    const BATCH_SIZE = 20; // Process 20 documents concurrently
    const basename = path.basename(inputPath, '.json');
    const processedDocs = [];
    
    const startTime = Date.now();
    
    for (let i = 0; i < documents.length; i += BATCH_SIZE) {
      const batchNum = Math.floor(i / BATCH_SIZE) + 1;
      const totalBatches = Math.ceil(documents.length / BATCH_SIZE);
      
      console.log(`   Processing batch ${batchNum}/${totalBatches} (documents ${i + 1}-${Math.min(i + BATCH_SIZE, documents.length)})...`);
      
      const batchResults = await processDocumentBatch(documents, i, BATCH_SIZE, basename);
      processedDocs.push(...batchResults);
      
      // Progress update
      const elapsed = Math.round((Date.now() - startTime) / 1000);
      const docsPerSec = processedDocs.length / elapsed;
      const remaining = Math.round((documents.length - processedDocs.length) / docsPerSec);
      
      console.log(`   Processed ${processedDocs.length}/${documents.length} documents (${Math.round(docsPerSec)} docs/sec, ~${remaining}s remaining)`);
      
      // Small delay between batches to avoid rate limits
      if (i + BATCH_SIZE < documents.length) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }
    }
    
    // Write the output file
    await fs.writeFile(outputPath, JSON.stringify(processedDocs, null, 2));
    
    const totalTime = Math.round((Date.now() - startTime) / 1000);
    console.log(`✅ Processed ${processedDocs.length} documents in ${totalTime}s`);
    console.log(`   Output saved to: ${outputPath}`);
    
    return true;
  } catch (error) {
    console.error(`❌ Error processing file: ${error.message}`);
    return false;
  }
}

async function processDirectory(inputDir, outputDir) {
  try {
    // Create output directory if it doesn't exist
    await fs.mkdir(outputDir, { recursive: true });
    
    // Get all JSON files in input directory
    const files = await fs.readdir(inputDir);
    const jsonFiles = files.filter(f => f.endsWith('.json'));
    
    if (jsonFiles.length === 0) {
      console.log('❌ No JSON files found in input directory');
      return;
    }
    
    console.log(`🚀 Processing ${jsonFiles.length} files from ${inputDir}`);
    console.log(`   Output directory: ${outputDir}\n`);
    
    let successCount = 0;
    for (const file of jsonFiles) {
      const inputPath = path.join(inputDir, file);
      const outputPath = path.join(outputDir, file);
      
      const success = await processJsonFile(inputPath, outputPath);
      if (success) successCount++;
    }
    
    console.log(`\n✨ Processing complete!`);
    console.log(`   Successfully processed: ${successCount}/${jsonFiles.length} files`);
    
  } catch (error) {
    console.error(`❌ Error: ${error.message}`);
    process.exit(1);
  }
}

// Main function
async function main() {
  const args = process.argv.slice(2);
  
  if (args.length < 2) {
    console.log('Usage: node generate_embeddings.js <input_dir> <output_dir>');
    console.log('Example: node generate_embeddings.js ./raw_docs ./embedded_docs');
    process.exit(1);
  }
  
  if (!process.env.OPENAI_API_KEY) {
    console.error('❌ Error: OPENAI_API_KEY environment variable not set');
    console.error('   Please set your OpenAI API key: export OPENAI_API_KEY=your-key-here');
    process.exit(1);
  }
  
  const [inputDir, outputDir] = args;
  await processDirectory(inputDir, outputDir);
}

// Run if called directly
if (require.main === module) {
  main();
}

module.exports = { processJsonFile, processDirectory };