#!/usr/bin/env node
// Picks a random PDF and its chunk JSON, runs the LLM judge, and writes a TXT report.
// Usage:
//   OPENAI_API_KEY=... node scripts/llm_judge_pick.mjs \
//     [--pdf-dir samples/pdf] [--chunk-dir .nvs-work/chunks] \
//     [--out-dir .nvs-work/judge] [--model gpt-4o-mini]

import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import process from 'node:process';

function parseArgs(argv) {
  const out = {
    pdfDir: 'samples/pdf',
    chunkDir: 'src/.nvs-work/chunks',
    outDir: 'src/.nvs-work/judge',
    model: 'gpt-5-mini',
  };
  for (let i = 2; i < argv.length; i++) {
    const k = argv[i];
    const v = argv[i + 1];
    if (k === '--pdf-dir') { out.pdfDir = v; i++; }
    else if (k === '--chunk-dir') { out.chunkDir = v; i++; }
    else if (k === '--out-dir') { out.outDir = v; i++; }
    else if (k === '--model') { out.model = v; i++; }
  }
  return out;
}

function listPdfs(dir) {
  if (!fs.existsSync(dir)) return [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files = [];
  for (const e of entries) {
    if (!e.isFile()) continue;
    const p = path.join(dir, e.name);
    if (p.toLowerCase().endsWith('.pdf')) files.push(p);
  }
  return files;
}

function findChunkForPdf(chunkDir, pdfPath) {
  const stem = path.basename(pdfPath, path.extname(pdfPath));
  const preferred = path.join(chunkDir, `${stem}_chunks.json`);
  if (fs.existsSync(preferred)) return preferred;
  // fallback: search for any json that includes stem
  if (!fs.existsSync(chunkDir)) return null;
  const cand = fs.readdirSync(chunkDir)
    .filter(n => n.toLowerCase().endsWith('.json'))
    .map(n => path.join(chunkDir, n))
    .find(p => path.basename(p).toLowerCase().includes(stem.toLowerCase()));
  return cand || null;
}

function runJudge(pdf, chunks, model, outJson) {
  return new Promise((resolve, reject) => {
    const args = [
      'scripts/llm_judge_openai.mjs',
      '--pdf', pdf,
      '--chunks', chunks,
      '--model', model,
      '--out', outJson,
    ];
    const proc = spawn(process.execPath, args, { stdio: 'inherit' });
    proc.on('exit', code => {
      if (code === 0) resolve(); else reject(new Error(`judge exited ${code}`));
    });
  });
}

async function main() {
  const args = parseArgs(process.argv);
  const { pdfDir, chunkDir, outDir, model } = args;
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    console.error('OPENAI_API_KEY not set.');
    process.exit(2);
  }
  const pdfs = listPdfs(pdfDir);
  if (pdfs.length === 0) {
    console.error(`no PDFs found in ${pdfDir}`);
    process.exit(2);
  }
  // random pick
  const idx = Math.floor(Math.random() * pdfs.length);
  const pdf = pdfs[idx];
  const chunk = findChunkForPdf(chunkDir, pdf);
  if (!chunk) {
    console.error(`no chunk found for ${pdf} under ${chunkDir}`);
    process.exit(2);
  }
  fs.mkdirSync(outDir, { recursive: true });
  const base = path.basename(pdf, path.extname(pdf));
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const outJson = path.join(outDir, `${base}.${ts}.judge.json`);
  const outTxt = path.join(outDir, `${base}.${ts}.judge.txt`);

  await runJudge(pdf, chunk, model, outJson);
  let payload = {};
  try { payload = JSON.parse(fs.readFileSync(outJson, 'utf8')); }
  catch (e) { payload = { error: String(e) }; }

  // Try to derive a human-readable report
  const tryParseFromRaw = (raw) => {
    if (!raw) return null;
    // Remove code fences and parse JSON if possible
    const fence = /```+[a-zA-Z0-9_-]*\n([\s\S]*?)```/m;
    const m = raw.match(fence);
    let s = m && m[1] ? m[1] : raw;
    const idxStart = s.indexOf('{');
    const idxEnd = s.lastIndexOf('}');
    if (idxStart >= 0 && idxEnd > idxStart) {
      const j = s.slice(idxStart, idxEnd + 1);
      try { return JSON.parse(j); } catch (_) {}
    }
    return null;
  };

  if (payload && payload.raw) {
    const parsed = tryParseFromRaw(payload.raw);
    if (parsed) payload = parsed;
  }

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
      lines.push(payload.summary.trim());
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
    lines.push(`JSON saved: ${outJson}`);
  } else {
    lines.push('LLM Output (raw):');
    lines.push(JSON.stringify(payload, null, 2));
  }

  fs.writeFileSync(outTxt, lines.join('\n'));
  console.error(`Wrote ${outTxt}`);
}

main().catch(err => {
  console.error('judge:pick failed:', err);
  process.exit(1);
});
