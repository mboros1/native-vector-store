#!/usr/bin/env node
/*
Generates a queries.json with real OpenAI embeddings for obvious topical queries
to validate vector/BM25/hybrid results against a bundled corpus.

Usage:
  OPENAI_API_KEY=... node scripts/generate_queries.js [out_file]

Default out_file: out_queries.json
*/

import fs from 'fs/promises';
import path from 'path';
import OpenAI from 'openai';

const OUT = process.argv[2] || path.join(process.cwd(), 'out_queries.json');
const MODEL = process.env.EMBED_MODEL || 'text-embedding-3-small';

const QUERIES = [
  { topic: 'physics',  query: 'quantum fields and photons in particle physics' },
  { topic: 'cooking',  query: 'baking with fresh ingredients in the kitchen' },
  { topic: 'finance',  query: 'portfolio diversification and managing risk in stock markets' },
  { topic: 'software', query: 'Rust ownership and memory safety for concurrency' },
];

async function main() {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    console.error('❌ OPENAI_API_KEY is required');
    process.exit(1);
  }
  const openai = new OpenAI({ apiKey });

  const out = [];
  for (const q of QUERIES) {
    process.stdout.write(`Embedding: ${q.query} ... `);
    const resp = await openai.embeddings.create({
      model: MODEL,
      input: q.query,
      encoding_format: 'float',
    });
    const embedding = resp.data[0].embedding;
    out.push({ topic: q.topic, query: q.query, embedding });
    console.log(`dim=${embedding.length}`);
  }

  await fs.writeFile(OUT, JSON.stringify(out, null, 2));
  console.log(`\n✅ Wrote ${out.length} queries with embeddings to ${OUT}`);
}

main().catch((e) => { console.error(e); process.exit(1); });
