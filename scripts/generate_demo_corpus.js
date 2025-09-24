#!/usr/bin/env node
/*
Generates a directory of JSON documents (arrays) suitable for embedding with embed_dir.js
and packing with nvs-packer. Documents are grouped into obvious topical clusters so queries
produce intuitive results.

Usage:
  node scripts/generate_demo_corpus.js [out_dir] [docs_per_topic]

Example:
  node scripts/generate_demo_corpus.js out_corpus 25
*/

import fs from 'fs/promises';
import path from 'path';

const OUT_DIR = process.argv[2] || path.join(process.cwd(), 'out_corpus');
const PER_TOPIC = parseInt(process.argv[3] || '25', 10);

const topics = [
  {
    name: 'physics',
    keywords: ['quantum', 'particle', 'wave', 'electron', 'photon', 'field', 'spin', 'energy'],
    patterns: [
      (kw, name, i) => `${kw[0]} ${kw[1]} are discussed here. We also mention ${kw[2]} and ${kw[3]} in this paragraph about ${name}.`,
      (kw, name, i) => `In modern ${name}, ${kw[0]} ${kw[4]} interactions reveal ${kw[5]} dynamics and ${kw[6]} states.`,
      (kw, name, i) => `An introduction to ${kw[0]} ${kw[5]} theory with focus on ${kw[1]} and ${kw[2]}.`,
    ],
  },
  {
    name: 'cooking',
    keywords: ['recipe', 'cook', 'bake', 'ingredients', 'oven', 'simmer', 'spice', 'kitchen'],
    patterns: [
      (kw, name, i) => `${kw[0]}: we ${kw[1]} and ${kw[2]} using fresh ${kw[3]} in the ${kw[7]}.`,
      (kw, name, i) => `How to ${kw[1]} with ${kw[6]} and ${kw[3]} then ${kw[5]} before using the ${kw[4]}.`,
      (kw, name, i) => `Beginner ${name} tips: ${kw[3]}, ${kw[6]}, and mastering your ${kw[4]}.`,
    ],
  },
  {
    name: 'finance',
    keywords: ['market', 'stock', 'investment', 'portfolio', 'risk', 'returns', 'capital', 'trading'],
    patterns: [
      (kw, name, i) => `${kw[2]} strategies for ${kw[3]} balance ${kw[4]} and expected ${kw[5]} in the ${kw[0]}.`,
      (kw, name, i) => `Principles of ${name}: ${kw[1]} ${kw[7]} and prudent ${kw[6]} allocation.`,
      (kw, name, i) => `A primer on ${kw[3]} diversification to mitigate ${kw[4]} in volatile ${kw[0]}.`,
    ],
  },
  {
    name: 'software',
    keywords: ['src', 'memory', 'safety', 'concurrency', 'compiler', 'borrow', 'ownership', 'performance'],
    patterns: [
      (kw, name, i) => `${kw[0]} ${kw[6]} model enables ${kw[2]} and ${kw[3]} with strong ${kw[1]} guarantees.`,
      (kw, name, i) => `The ${kw[4]} enforces ${kw[5]} rules to improve ${kw[7]} and ${kw[2]}.`,
      (kw, name, i) => `Modern ${name} emphasizes ${kw[2]} and ${kw[3]} without sacrificing ${kw[7]}.`,
    ],
  },
];

function seededShuffle(arr, seed) {
  // Simple LCG-based shuffle for determinism
  function rand() { seed = (seed * 1664525 + 1013904223) % 4294967296; return seed / 4294967296; }
  const a = arr.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

async function ensureDir(dir) { await fs.mkdir(dir, { recursive: true }); }

async function generate() {
  await ensureDir(OUT_DIR);
  console.log(`📁 Writing corpus to ${OUT_DIR} (${PER_TOPIC} docs/topic)`);

  for (const topic of topics) {
    const docs = [];
    for (let i = 0; i < PER_TOPIC; i++) {
      const kws = seededShuffle(topic.keywords, 42 + i);
      const pat = topic.patterns[i % topic.patterns.length];
      const text = pat(kws, topic.name, i);
      docs.push({
        id: `${topic.name}-${String(i).padStart(3, '0')}`,
        text,
        metadata: {}
      });
    }
    const file = path.join(OUT_DIR, `${topic.name}.json`);
    await fs.writeFile(file, JSON.stringify(docs, null, 2));
    console.log(`  ✅ ${path.basename(file)} (${docs.length} docs)`);
  }

  console.log('\nNext steps:');
  console.log(`  1) Add embeddings:  OPENAI_API_KEY=... npm run embed -- ${OUT_DIR}`);
  console.log('  2) Pack bundle (Rust): cargo run -p nvs-packer --', OUT_DIR, '-o out_bundle');
  console.log('  3) Generate query embeddings: node scripts/generate_queries.js out_queries.json');
  console.log('  4) Validate (Rust): cargo run --example validate_corpus --manifest-path src/crates/nvs-core/Cargo.toml -- --bundle out_bundle --queries out_queries.json --k 5');
}

generate().catch((e) => { console.error(e); process.exit(1); });
