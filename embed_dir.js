// embed_dir.js
const fs = require('fs/promises');
const path = require('path');
const OpenAI = require('openai');

const MODEL = 'text-embedding-3-small';          // or -large
const DIR   = process.argv[2] || '.';            // directory of JSON docs
const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });

async function readJson(file) {
  const raw = await fs.readFile(file, 'utf8');
  return JSON.parse(raw);
}

function extractText(doc) {
  return doc.text ?? doc.content ?? doc.content?.text ?? '';
}

async function writeJson(file, data) {
  await fs.writeFile(file, JSON.stringify(data, null, 2));
}

async function embed(text) {
  const resp = await openai.embeddings.create({
    model: MODEL,
    input: text,
    encoding_format: 'float',
  });
  return resp.data[0].embedding;
}

async function processFile(file) {
  console.log(`Processing ${path.basename(file)}...`);
  
  const docs = await readJson(file);
  
  // Handle both single document and array of documents
  const docArray = Array.isArray(docs) ? docs : [docs];
  
  let processed = 0;
  
  for (const doc of docArray) {
    const text = extractText(doc);
    if (!text || text.trim().length === 0) {
      console.log(`  Skipping document ${processed + 1} - no text content`);
      continue;
    }
    
    // Check if already has embedding
    if (doc.metadata?.embedding) {
      console.log(`  Document ${processed + 1} already has embedding, skipping`);
      continue;
    }
    
    try {
      // Create metadata if it doesn't exist
      if (!doc.metadata) {
        doc.metadata = {};
      }
      
      // Add embedding to metadata
      doc.metadata.embedding = await embed(text);
      
      // Add document ID if it doesn't exist
      if (!doc.id) {
        doc.id = `${path.basename(file, '.json')}_${processed}`;
      }
      
      console.log(`  Added embedding to document ${processed + 1} (${doc.id})`);
      
      // Rate limiting to avoid hitting API limits
      await new Promise(resolve => setTimeout(resolve, 100));
      
    } catch (error) {
      console.error(`  Error processing document ${processed + 1}:`, error.message);
    }
    
    processed++;
  }
  
  // Save the updated documents
  await writeJson(file, docArray);
  console.log(`✅ Updated ${path.basename(file)} (${processed} documents processed)`);
}

async function main() {
  if (!process.env.OPENAI_API_KEY) {
    console.error('Error: OPENAI_API_KEY environment variable is required');
    process.exit(1);
  }
  
  console.log(`🚀 Starting embedding process for directory: ${DIR}`);
  console.log(`📏 Using model: ${MODEL}`);
  
  const files = (await fs.readdir(DIR))
    .filter(f => f.endsWith('.json'))
    .map(f => path.join(DIR, f));
  
  console.log(`📁 Found ${files.length} JSON files to process`);
  
  for (const file of files) {
    await processFile(file);
  }
  
  console.log('\n🎉 All files processed successfully!');
}

if (require.main === module) {
  main().catch(console.error);
}