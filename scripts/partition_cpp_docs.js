#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

function partitionDocuments(inputDir, outputDir, docsPerFile = 100) {
  console.log(`📂 Partitioning documents from ${inputDir} to ${outputDir}`);
  console.log(`   Target: ${docsPerFile} documents per file\n`);

  // Create output directory
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  // Process each JSON file in input directory
  const files = fs.readdirSync(inputDir).filter(f => f.endsWith('.json'));
  let totalDocs = 0;
  let totalFiles = 0;

  for (const file of files) {
    console.log(`📄 Processing ${file}...`);
    const filePath = path.join(inputDir, file);
    const content = fs.readFileSync(filePath, 'utf8');
    const data = JSON.parse(content);

    if (Array.isArray(data)) {
      // Split array into chunks
      for (let i = 0; i < data.length; i += docsPerFile) {
        const chunk = data.slice(i, i + docsPerFile);
        const outputFile = path.join(outputDir, `${path.basename(file, '.json')}_${String(totalFiles).padStart(4, '0')}.json`);
        
        fs.writeFileSync(outputFile, JSON.stringify(chunk, null, 2));
        totalFiles++;
        totalDocs += chunk.length;
        
        if (totalFiles % 10 === 0) {
          console.log(`   Created ${totalFiles} files (${totalDocs} docs)...`);
        }
      }
    } else {
      // Single document - just copy
      const outputFile = path.join(outputDir, `${path.basename(file, '.json')}_${String(totalFiles).padStart(4, '0')}.json`);
      fs.writeFileSync(outputFile, JSON.stringify([data], null, 2));
      totalFiles++;
      totalDocs++;
    }
  }

  console.log(`\n✅ Partitioning complete!`);
  console.log(`   Total documents: ${totalDocs}`);
  console.log(`   Total files created: ${totalFiles}`);
  console.log(`   Average docs per file: ${(totalDocs / totalFiles).toFixed(1)}`);
}

// Run if called directly
if (require.main === module) {
  const args = process.argv.slice(2);
  
  if (args.length < 2) {
    console.error('Usage: node partition_cpp_docs.js <input_dir> <output_dir> [docs_per_file]');
    console.error('Example: node partition_cpp_docs.js cpp_std_embedded cpp_std_partitioned 100');
    process.exit(1);
  }

  const inputDir = args[0];
  const outputDir = args[1];
  const docsPerFile = args[2] ? parseInt(args[2]) : 100;

  if (!fs.existsSync(inputDir)) {
    console.error(`Error: Input directory '${inputDir}' does not exist`);
    process.exit(1);
  }

  partitionDocuments(inputDir, outputDir, docsPerFile);
}

module.exports = { partitionDocuments };