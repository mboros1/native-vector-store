#!/usr/bin/env node
// Consolidated LLM-as-judge script.
// Modes:
// 1) Single file: --pdf FILE.pdf [--chunks FILE.json]
//    If --chunks omitted, tries to find in --chunk-dir.
// 2) Random pick: if --pdf not provided, picks from --pdf-dir and matches chunk in --chunk-dir.
// Writes JSON and TXT reports to --out or --out-dir.
//
// Usage examples:
//   OPENAI_API_KEY=... npm run judge -- --pdf samples/pdf/Doc.pdf --chunks .nvs-work/chunks/Doc_chunks.json
//   OPENAI_API_KEY=... npm run judge -- --pdf-dir samples/pdf --chunk-dir .nvs-work/chunks --out-dir .nvs-work/judge

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import OpenAI from 'openai';

function parseArgs(argv) {
  const out = {
    pdfDir: 'samples/pdf',
    chunkDir: '.nvs-work/chunks',
    outDir: '.nvs-work/judge',
    model: 'gpt-5-mini',
  };
  for (let i = 2; i < argv.length; i++) {
    const k = argv[i];
    const v = argv[i + 1];
    if (k === '--pdf') { out.pdf = v; i++; }
    else if (k === '--chunks') { out.chunks = v; i++; }
    else if (k === '--pdf-dir') { out.pdfDir = v; i++; }
    else if (k === '--chunk-dir') { out.chunkDir = v; i++; }
    else if (k === '--out') { out.out = v; i++; }
    else if (k === '--out-dir') { out.outDir = v; i++; }
    else if (k === '--model') { out.model = v; i++; }
    else if (k === '--pages') { out.pages = v; i++; }
    else if (k === '--seed') { out.seed = v; i++; }
  }
  return out;
}

function listPdfs(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true })
    .filter(e => e.isFile() && e.name.toLowerCase().endsWith('.pdf'))
    .map(e => path.join(dir, e.name));
}

function findChunkForPdf(chunkDir, pdfPath) {
  const stem = path.basename(pdfPath, path.extname(pdfPath));
  const preferred = path.join(chunkDir, `${stem}_chunks.json`);
  if (fs.existsSync(preferred)) return preferred;
  if (!fs.existsSync(chunkDir)) return null;
  const cand = fs.readdirSync(chunkDir)
    .filter(n => n.toLowerCase().endsWith('.json'))
    .map(n => path.join(chunkDir, n))
    .find(p => path.basename(p).toLowerCase().includes(stem.toLowerCase()));
  return cand || null;
}

function pickSamples(chunks, pagesCsv, maxPages = 5) {
  // Exclude doc-level meta records that can span multiple pages and confuse evaluation
  const items = chunks.filter(c => {
    const schema = (c.meta && c.meta.schema_name) || (c.metadata && c.metadata.schema_name) || c.schema_name;
    return !(schema && /docmeta/i.test(schema));
  });
  let pages = [];
  if (pagesCsv) {
    pages = pagesCsv.split(',').map(s => parseInt(s.trim(), 10)).filter(n => Number.isFinite(n));
  } else {
    const present = new Set();
    for (const c of items) {
      const sp = (c.meta?.start_page ?? c.metadata?.start_page ?? c.start_page);
      if (typeof sp === 'number') present.add(sp);
    }
    pages = Array.from(present).sort((a,b) => a-b).slice(0, maxPages);
  }
  const selected = [];
  for (const p of pages) {
    const texts = items
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

function parseModelJson(text) {
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
    return JSON.parse(cleaned);
  } catch (e) {
    return { raw: text };
  }
}

function buildTxtReport(pdf, chunk, model, payload, jsonPath) {
  const lines = [];
  lines.push(`PDF:   ${pdf}`);
  lines.push(`Chunk: ${chunk}`);
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
    if (jsonPath) lines.push(`JSON saved: ${jsonPath}`);
  } else {
    lines.push('LLM Output (raw):');
    lines.push(JSON.stringify(payload, null, 2));
  }
  return lines.join('\n');
}

async function judgeOnce(pdfPath, chunkPath, model, outJson, outTxt, pagesCsv) {
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) throw new Error('OPENAI_API_KEY not set');
  const chunks = JSON.parse(fs.readFileSync(chunkPath, 'utf8'));
  const selected = pickSamples(chunks, pagesCsv || null, 5);
  const prompt = buildPrompt(path.basename(pdfPath), selected);
  const client = new OpenAI({ apiKey });
  const file = await client.files.create({ file: fs.createReadStream(pdfPath), purpose: 'assistants' });
  const resp = await client.responses.create({
    model,
    input: [
      { role: 'user', content: [ { type: 'input_text', text: prompt }, { type: 'input_file', file_id: file.id } ] }
    ],
  });
  let text = '';
  for (const item of resp.output || []) {
    if (item.type === 'message') {
      for (const p of item.content || []) {
        if (p.type === 'output_text') text += p.text;
      }
    }
  }
  const jsonOut = parseModelJson(text || JSON.stringify(resp));
  if (outJson) fs.writeFileSync(outJson, JSON.stringify(jsonOut, null, 2));
  if (outTxt) fs.writeFileSync(outTxt, buildTxtReport(pdfPath, chunkPath, model, jsonOut, outJson));
  return { json: jsonOut, outJson, outTxt };
}

async function main() {
  const args = parseArgs(process.argv);
  const model = args.model || 'gpt-5-mini';
  let pdfPath = args.pdf ? path.resolve(args.pdf) : null;
  let chunkPath = args.chunks ? path.resolve(args.chunks) : null;
  const outDir = args.outDir ? path.resolve(args.outDir) : path.resolve('.nvs-work/judge');

  if (!pdfPath) {
    // random pick
    const pdfs = listPdfs(args.pdfDir);
    if (pdfs.length === 0) {
      console.error(`no PDFs found in ${args.pdfDir}`);
      process.exit(2);
    }
    const seed = args.seed ? Number(args.seed) : Date.now();
    const rng = () => { const x = Math.sin(seed) * 10000; return x - Math.floor(x); };
    const idx = Math.floor(rng() * pdfs.length);
    pdfPath = path.resolve(pdfs[idx]);
  }
  if (!chunkPath) {
    const found = findChunkForPdf(args.chunkDir, pdfPath);
    if (!found) {
      console.error(`no chunk found for ${pdfPath} under ${args.chunkDir}`);
      process.exit(2);
    }
    chunkPath = path.resolve(found);
  }
  fs.mkdirSync(outDir, { recursive: true });
  const base = path.basename(pdfPath, path.extname(pdfPath));
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const outJson = args.out ? path.resolve(args.out) : path.join(outDir, `${base}.${ts}.judge.json`);
  const outTxt = outJson.replace(/\.json$/i, '.txt');
  const { outJson: oj, outTxt: ot } = await judgeOnce(pdfPath, chunkPath, model, outJson, outTxt, args.pages);
  console.error(`Wrote ${oj}`);
  console.error(`Wrote ${ot}`);
}

main().catch(err => {
  console.error('judge failed:', err);
  process.exit(1);
});
