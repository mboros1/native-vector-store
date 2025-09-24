#!/usr/bin/env node
// LLM-as-judge for PDF extraction quality using OpenAI Responses API.
// Usage:
//   OPENAI_API_KEY=... node scripts/llm_judge_openai.mjs \
//     --pdf path/to/file.pdf \
//     --chunks path/to/chunks.json \
//     --model gpt-5-mini \
//     --out report.json

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import OpenAI from 'openai';

function parseArgs(argv) {
  const out = {};
  for (let i = 2; i < argv.length; i++) {
    const k = argv[i];
    const v = argv[i + 1];
    if (k === '--pdf') { out.pdf = v; i++; }
    else if (k === '--chunks') { out.chunks = v; i++; }
    else if (k === '--model') { out.model = v; i++; }
    else if (k === '--out') { out.out = v; i++; }
    else if (k === '--pages') { out.pages = v; i++; }
    else if (k === '--out-txt') { out.outTxt = v; i++; }
  }
  return out;
}

function pickSamples(chunks, pagesCsv, maxPages = 5) {
  // Select up to maxPages distinct pages present in chunks.
  let pages = [];
  if (pagesCsv) {
    pages = pagesCsv.split(',').map(s => parseInt(s.trim(), 10)).filter(n => Number.isFinite(n));
  } else {
    const present = new Set();
    for (const c of chunks) {
      const sp = (c.meta?.start_page ?? c.metadata?.start_page ?? c.start_page);
      if (typeof sp === 'number') present.add(sp);
    }
    pages = Array.from(present).sort((a,b) => a-b).slice(0, maxPages);
  }
  // Gather chunk texts for selected pages
  const selected = [];
  for (const p of pages) {
    const texts = chunks
      .filter(c => (c.meta?.start_page ?? c.metadata?.start_page ?? c.start_page) === p)
      .map(c => c.text).filter(Boolean);
    selected.push({ page: p, texts });
  }
  return selected;
}

function buildPrompt(pdfName, selected) {
  return `You are an expert judge assessing PDF text extraction quality for Retrieval-Augmented Generation (RAG).
We provide a PDF file (${pdfName}) and extracted text chunks grouped by page.
Rate the extraction on the following criteria for the provided pages, focusing on its suitability for RAG:

- completeness: does extracted text cover the page content?
- fidelity: preserves order, paragraphs, headings? avoids duplicates?
- noise: headers/footers/page numbers repeated? artifacts, broken hyphenations, ligatures?
- structure: section/heading boundaries and paragraph grouping sensible?
- tables/figures: captions/alt text reasonable, not interleaved in wrong places?

Output strict JSON only with this schema (the "overall" score is the RAG quality grade):
{
  "summary": string,
  "scores": {
    "completeness": 1..5,
    "fidelity": 1..5,
    "noise": 1..5,
    "structure": 1..5,
    "tables_figures": 1..5
  },
  "page_findings": [
    { "page": number, "notes": string }
  ],
  "overall": 1..5
}

Focus only on the provided pages and be concise in notes.

Here are the extracted texts by page (page number and list of chunks):
${selected.map(s => `Page ${s.page}:\n- ` + s.texts.slice(0, 30).map(t => t.replace(/\s+/g, ' ').slice(0, 300)).join('\n- ')).join('\n\n')}
`;
}

async function main() {
  const args = parseArgs(process.argv);
  if (!args.pdf || !args.chunks) {
    console.error('Usage: node scripts/llm_judge_openai.mjs --pdf FILE.pdf --chunks FILE.json [--model gpt-5-mini] [--pages 1,5,7] [--out report.json]');
    process.exit(2);
  }
  const model = args.model || 'gpt-5-mini';
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    console.error('OPENAI_API_KEY not set.');
    process.exit(2);
  }
  const pdfPath = path.resolve(args.pdf);
  const chunksPath = path.resolve(args.chunks);
  const chunks = JSON.parse(fs.readFileSync(chunksPath, 'utf8'));
  const selected = pickSamples(chunks, args.pages, 5);
  const prompt = buildPrompt(path.basename(pdfPath), selected);

  const client = new OpenAI({ apiKey });
  // Upload PDF file
  const file = await client.files.create({
    file: fs.createReadStream(pdfPath),
    purpose: 'assistants'
  });

  const resp = await client.responses.create({
    model,
    input: [
      {
        role: 'user',
        content: [
          { type: 'input_text', text: prompt },
          { type: 'input_file', file_id: file.id }
        ]
      }
    ],
  });

  // Extract text from response
  let text = '';
  for (const item of resp.output || []) {
    if (item.type === 'message') {
      for (const p of item.content || []) {
        if (p.type === 'output_text') text += p.text;
      }
    }
  }
  if (!text) text = JSON.stringify(resp, null, 2);

  // Try to parse JSON from model output (handle code fences gracefully)
  let jsonOut = null;
  const stripFences = (s) => {
    const fence = /```+[a-zA-Z0-9_-]*\n([\s\S]*?)```/m;
    const m = s.match(fence);
    if (m && m[1]) return m[1];
    return s;
  };
  const extractTailJson = (s) => {
    const idxStart = s.indexOf('{');
    const idxEnd = s.lastIndexOf('}');
    if (idxStart >= 0 && idxEnd > idxStart) return s.slice(idxStart, idxEnd + 1);
    return s;
  };
  try {
    let cleaned = stripFences(text);
    cleaned = extractTailJson(cleaned);
    jsonOut = JSON.parse(cleaned);
  } catch (e) {
    jsonOut = { raw: text };
  }

  if (args.out) {
    fs.writeFileSync(args.out, JSON.stringify(jsonOut, null, 2));
    console.error(`Wrote ${args.out}`);
  } else {
    console.log(JSON.stringify(jsonOut, null, 2));
  }

  // Optionally produce a human-readable TXT report alongside JSON
  const buildTxt = (payload) => {
    const lines = [];
    lines.push(`PDF:   ${pdfPath}`);
    lines.push(`Chunk: ${chunksPath}`);
    lines.push(`Model: ${model}`);
    lines.push(`Date:  ${new Date().toString()}`);
    lines.push('');
    const fmtScores = (scores) => {
      const order = [
        ['completeness', 'Completeness'],
        ['fidelity', 'Fidelity'],
        ['noise', 'Noise'],
        ['structure', 'Structure'],
        ['tables_figures', 'Tables/Figures'],
      ];
      return order
        .filter(([k]) => scores && typeof scores[k] !== 'undefined')
        .map(([k, label]) => `${label}: ${scores[k]}`)
        .join('\n');
    };
    if (payload && (payload.summary || payload.scores || payload.page_findings || payload.overall)) {
      if (payload.summary) {
        lines.push('Summary:');
        lines.push(String(payload.summary).trim());
        lines.push('');
      }
      if (payload.scores) {
        lines.push('Scores (1–5):');
        lines.push(fmtScores(payload.scores));
        lines.push('');
      }
      if (typeof payload.overall !== 'undefined') {
        lines.push(`Overall: ${payload.overall}`);
        lines.push('');
      }
      if (Array.isArray(payload.page_findings) && payload.page_findings.length) {
        lines.push('Page Findings:');
        for (const pf of payload.page_findings) {
          const page = typeof pf.page !== 'undefined' ? `Page ${pf.page}` : 'Page ?';
          const notes = (pf.notes || '').toString().trim();
          lines.push(`- ${page}: ${notes}`);
        }
        lines.push('');
      }
      if (args.out) lines.push(`JSON saved: ${args.out}`);
    } else {
      lines.push('LLM Output (raw):');
      lines.push(JSON.stringify(payload, null, 2));
    }
    return lines.join('\n');
  };
  // Write TXT if requested via --out-txt, or auto if --out provided (replace .json with .txt)
  const outTxtPath = args.outTxt || (args.out ? args.out.replace(/\.json$/i, '.txt') : null);
  if (outTxtPath) {
    fs.writeFileSync(outTxtPath, buildTxt(jsonOut));
    console.error(`Wrote ${outTxtPath}`);
  }
}

main().catch(err => {
  console.error('judge failed:', err);
  process.exit(1);
});
