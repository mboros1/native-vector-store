#!/usr/bin/env node
// ESM benchmark runner for nvs-packer
// Runs the packer multiple times on a given input dir, parses timings, and reports averages.

import fs from 'fs/promises';
import path from 'path';
import { spawn } from 'child_process';
import pidusage from 'pidusage';
import si from 'systeminformation';

function parseDuration(s) {
  // Accept formats like: "1.012587375s", "24.237291ms", "145.417µs"
  const m = String(s).trim().match(/^([0-9]*\.?[0-9]+)\s*(s|ms|µs|us)$/);
  if (!m) return NaN;
  const v = parseFloat(m[1]);
  const unit = m[2];
  if (unit === 's') return v * 1000;
  if (unit === 'ms') return v;
  if (unit === 'µs' || unit === 'us') return v / 1000;
  return NaN;
}

function parseTimesFromOutput(out) {
  // Find the line starting with "  Time: read ..."
  const lines = out.split(/\r?\n/);
  const timeLine = lines.find(l => l.trim().startsWith('Time: read')) || '';
  // Example: Time: read 1.012587375s  vectors 19.130334ms  bm25 2.410641083s  meta 1.904908417s  manifest 127.583µs  checksums 121.902209ms
  const parts = timeLine.replace('Time:', '').trim().split(/\s+/);
  // parts like: ["read", "1.01s", "vectors", "19ms", "bm25", "2.41s", "meta", "1.90s", "manifest", "127µs", "checksums", "121ms"]
  const obj = {};
  for (let i = 0; i + 1 < parts.length; i += 2) {
    const key = parts[i];
    const val = parseDuration(parts[i + 1]);
    if (!Number.isNaN(val)) obj[key] = val;
  }
  return obj;
}

function parseBundleSize(out) {
  const m = out.match(/Bundle size:\s+([0-9.]+)\s+MB/);
  return m ? parseFloat(m[1]) : NaN;
}

async function runOnce({ bin, input, outDir, compress, level, blockSize, usePipeline=false, threads=0, parallelStages=false, fastLoader=false, mmapThreshold=0, bm25Buckets=0 }) {
  await fs.rm(outDir, { recursive: true, force: true }).catch(() => {});
  await fs.mkdir(outDir, { recursive: true });
  const args = [];
  if (compress) args.push(`--compress=${compress}`);
  if (level != null) args.push(`--zstd-level=${level}`);
  if (blockSize) args.push(`--block-size=${blockSize}`);
  if (usePipeline) args.push('--use-pipeline');
  if (threads && Number.isFinite(threads) && threads > 0) args.push(`--threads=${threads}`);
  if (parallelStages) args.push('--parallel-stages');
  if (fastLoader) args.push('--fast-loader');
  if (mmapThreshold) args.push(`--mmap-threshold=${mmapThreshold}`);
  if (bm25Buckets) args.push(`--bm25-buckets=${bm25Buckets}`);
  args.push(input, '-o', outDir);

  const t0 = process.hrtime.bigint();
  let stdout = '';
  let stderr = '';
  const child = spawn(bin, args, { stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', d => { stdout += d.toString(); });
  child.stderr.on('data', d => { stderr += d.toString(); });

  // Real-time monitoring (best effort)
  const cpuSamples = [];
  const rssSamples = [];
  const diskSamples = []; // { readMBs, writeMBs }
  let lastDisk = null;
  // macOS fallback: stream iostat if systeminformation lacks rBytes/wBytes
  let ioProc = null;
  let ioStarted = false;
  if (process.platform === 'darwin') {
    try {
      // Use extended stats to get read/write split if available
      ioProc = spawn('iostat', ['-d', '-w', '1', '-x']);
      let buf = '';
      let headerFields = [];
      let headerParsed = false;
      ioProc.stdout.on('data', (d) => {
        buf += d.toString();
        const lines = buf.split(/\r?\n/);
        buf = lines.pop() || '';
        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed) continue;
          const parts = trimmed.split(/\s+/);
          // Parse header with r/s, w/s, kr/s, kw/s if present
          if (!headerParsed && (trimmed.includes('r/s') || trimmed.includes('kr/s'))) {
            headerFields = parts;
            headerParsed = true;
            continue;
          }
          // Skip summary lines that don't start with a disk name
          if (!headerParsed) continue;
          // Heuristically: first column is device name (e.g., disk0)
          if (!/^disk/.test(parts[0])) continue;
          const fieldIndex = (name) => headerFields.findIndex(h => h.toLowerCase() === name);
          let readMBs = 0, writeMBs = 0;
          const krIdx = fieldIndex('kr/s');
          const kwIdx = fieldIndex('kw/s');
          const mbsIdx = fieldIndex('mb/s');
          if (krIdx > 0 && kwIdx > 0 && parts.length > Math.max(krIdx, kwIdx)) {
            // Values in KB/s -> convert to MB/s
            const kr = Number(parts[krIdx]);
            const kw = Number(parts[kwIdx]);
            if (!Number.isNaN(kr)) readMBs += kr / 1024;
            if (!Number.isNaN(kw)) writeMBs += kw / 1024;
          } else if (mbsIdx > 0 && parts.length > mbsIdx) {
            const mbs = Number(parts[mbsIdx]);
            if (!Number.isNaN(mbs)) { writeMBs += mbs; }
          }
          if (readMBs || writeMBs) {
            diskSamples.push({ readMBs, writeMBs });
          }
        }
      });
      ioProc.on('error', () => { /* ignore */ });
      ioStarted = true;
    } catch {
      ioProc = null;
    }
  }
  const intervalMs = 1000;

  async function sampleOnce() {
    try {
      const stat = await pidusage(child.pid);
      // pidusage cpu is % (0..100*cores). We'll keep as % of one core for simplicity.
      cpuSamples.push(stat.cpu);
      rssSamples.push(stat.memory / (1024*1024));
    } catch {}
    try {
      const io = await si.disksIO();
      const t = Date.now();
      let readMBs = NaN, writeMBs = NaN;
      if (typeof io.rBytes === 'number' && typeof io.wBytes === 'number' && io.rBytes > 0 && io.wBytes > 0) {
        if (lastDisk) {
          const dt = (t - lastDisk.t) / 1000;
          const dRead = Math.max(0, io.rBytes - lastDisk.rBytes);
          const dWrite = Math.max(0, io.wBytes - lastDisk.wBytes);
          readMBs = dRead / (1024*1024) / (dt || 1);
          writeMBs = dWrite / (1024*1024) / (dt || 1);
          diskSamples.push({ readMBs, writeMBs });
        }
        lastDisk = { rBytes: io.rBytes, wBytes: io.wBytes, t };
      }
      // Live line from either SI or iostat fallback
      const latest = diskSamples.at(-1) || { readMBs: 0, writeMBs: 0 };
      process.stdout.write(`    [live] CPU:${(cpuSamples.at(-1)||0).toFixed(1)}%  RSS:${(rssSamples.at(-1)||0).toFixed(1)}MB  Disk r:${(latest.readMBs||0).toFixed(1)}MB/s w:${(latest.writeMBs||0).toFixed(1)}MB/s\r`);
    } catch {}
  }

  const timer = setInterval(sampleOnce, intervalMs);
  // Prime disk baseline
  await sampleOnce();

  await new Promise((resolve, reject) => {
    child.on('close', code => {
      clearInterval(timer);
      if (ioProc && ioStarted) { try { ioProc.kill(); } catch {} }
      process.stdout.write('\n');
      if (code === 0) resolve();
      else reject(new Error(`packer exit ${code}: ${stderr}`));
    });
  });
  const t1 = process.hrtime.bigint();
  const wallMs = Number(t1 - t0) / 1e6;
  const times = parseTimesFromOutput(stdout);
  const sizeMb = parseBundleSize(stdout);
  // Aggregate monitor stats
  const avg = a => a.length ? a.reduce((x,y)=>x+y,0)/a.length : 0;
  const max = a => a.length ? Math.max(...a) : 0;
  const cpuAvg = avg(cpuSamples);
  const cpuMax = max(cpuSamples);
  const rssMax = max(rssSamples);
  const readAvg = avg(diskSamples.map(d=>d.readMBs));
  const writeAvg = avg(diskSamples.map(d=>d.writeMBs));
  const readMax = max(diskSamples.map(d=>d.readMBs));
  const writeMax = max(diskSamples.map(d=>d.writeMBs));

  return { wallMs, times, sizeMb, raw: stdout, monitor: { cpuAvg, cpuMax, rssMax, readAvg, writeAvg, readMax, writeMax } };
}

function avg(arr) { return arr.reduce((a,b)=>a+b,0) / (arr.length || 1); }

async function main() {
  // Defaults
  const cwd = process.cwd();
  const bin = path.join(cwd, 'src', 'target', 'release', 'nvs-packer');
  const input = process.argv.find(a => a.startsWith('--input='))?.split('=')[1] || path.join(cwd, 'samples', 'embedded_docs');
  const outDir = process.argv.find(a => a.startsWith('--out='))?.split('=')[1] || path.join(cwd, 'nvs-bundle');
  const runs = parseInt(process.argv.find(a => a.startsWith('--runs='))?.split('=')[1] || '3', 10);
  const compress = (process.argv.find(a => a.startsWith('--compress='))?.split('=')[1]) || 'zstd';
  const level = parseInt(process.argv.find(a => a.startsWith('--zstd-level='))?.split('=')[1] || '3', 10);
  const blockSize = parseInt(process.argv.find(a => a.startsWith('--block-size='))?.split('=')[1] || '131072', 10);
  const compare = !!process.argv.find(a => a === '--compare-pipeline' || a === '--compare');
  const threads = parseInt(process.argv.find(a => a.startsWith('--threads='))?.split('=')[1] || '0', 10);
  const parallelStages = !!process.argv.find(a => a === '--parallel-stages');
  const fastLoader = !!process.argv.find(a => a === '--fast-loader');
  const mmapThreshold = parseInt(process.argv.find(a => a.startsWith('--mmap-threshold='))?.split('=')[1] || '0', 10);
  const bm25Buckets = parseInt(process.argv.find(a => a.startsWith('--bm25-buckets='))?.split('=')[1] || '0', 10);

  // Sanity check binary
  try { await fs.access(bin); } catch { console.error(`Missing packer binary at ${bin}. Build with: cargo build -p nvs-packer --release`); process.exit(1); }
  // Sanity check input
  try { await fs.access(input); } catch { console.error(`Input not found: ${input}`); process.exit(1); }

  console.log(`Benchmarking: ${bin}`);
  console.log(`  Input: ${input}`);
  console.log(`  Output dir: ${outDir}`);
  console.log(`  Runs: ${runs}  Compress: ${compress}  Level: ${level}  BlockSize: ${blockSize}`);
  if (compare) {
    console.log(`  Compare: baseline vs pipeline (threads=${threads||'auto'}, parallelStages=${parallelStages})`);
  }
  async function runScenario(label, opts) {
    const results = [];
    for (let i = 0; i < runs; i++) {
      console.log(`\n[${label}] Run ${i+1}/${runs}...`);
      const r = await runOnce({ bin, input, outDir: `${opts.outDir}_${label}`, compress, level, blockSize, ...opts });
  const t = r.times;
  console.log(`  Wall: ${r.wallMs.toFixed(1)} ms  Size: ${isNaN(r.sizeMb)?'n/a':r.sizeMb.toFixed(2)+' MB'}`);
  const bm25Extra = (t['bm25_tokenize']!=null || t['bm25_local']!=null || t['bm25_merge']!=null)
    ? `  bm25_tokenize=${(t['bm25_tokenize']||0).toFixed(2)}  bm25_local=${(t['bm25_local']||0).toFixed(2)}  bm25_merge=${(t['bm25_merge']||0).toFixed(2)}`
    : '';
  console.log(`  Stages (ms): read=${t.read?.toFixed(2)}  vectors=${t.vectors?.toFixed(2)}  bm25=${t.bm25?.toFixed(2)}${bm25Extra}  meta=${t.meta?.toFixed(2)}  manifest=${t.manifest?.toFixed(2)}  checksums=${t.checksums?.toFixed(2)}`);
      if (r.monitor) {
        const m = r.monitor;
        console.log(`  CPU avg=${m.cpuAvg.toFixed(1)}% max=${m.cpuMax.toFixed(1)}%  RSS max=${m.rssMax.toFixed(1)} MB`);
        console.log(`  Disk r avg=${m.readAvg.toFixed(1)}MB/s max=${m.readMax.toFixed(1)}MB/s  w avg=${m.writeAvg.toFixed(1)}MB/s max=${m.writeMax.toFixed(1)}MB/s`);
      }
      results.push(r);
    }
    return results;
  }

  const baseline = await runScenario('baseline', { outDir, fastLoader, mmapThreshold, bm25Buckets });
  let piped = null;
  if (compare) {
    piped = await runScenario('pipeline', { outDir, usePipeline: true, threads, parallelStages });
  }

  function summarize(label, results) {
    const walls = results.map(r => r.wallMs);
    const reads = results.map(r => r.times.read || 0);
    const vectors = results.map(r => r.times.vectors || 0);
    const bm25 = results.map(r => r.times.bm25 || 0);
    const meta = results.map(r => r.times.meta || 0);
    const manif = results.map(r => r.times.manifest || 0);
    const chks = results.map(r => r.times.checksums || 0);
    const sizes = results.map(r => r.sizeMb).filter(v => !isNaN(v));
    const cpuAvg = results.map(r => r.monitor?.cpuAvg || 0);
    const cpuMax = results.map(r => r.monitor?.cpuMax || 0);
    const rssMax = results.map(r => r.monitor?.rssMax || 0);
    const rAvg = results.map(r => r.monitor?.readAvg || 0);
    const rMax = results.map(r => r.monitor?.readMax || 0);
    const wAvg = results.map(r => r.monitor?.writeAvg || 0);
    const wMax = results.map(r => r.monitor?.writeMax || 0);
    console.log(`\n[${label}] Averages over ${runs} runs (ms):`);
    console.log(`  Wall: ${avg(walls).toFixed(1)}`);
    console.log(`  read=${avg(reads).toFixed(2)}  vectors=${avg(vectors).toFixed(2)}  bm25=${avg(bm25).toFixed(2)}  meta=${avg(meta).toFixed(2)}  manifest=${avg(manif).toFixed(2)}  checksums=${avg(chks).toFixed(2)}`);
    if (sizes.length) console.log(`  Size: ${avg(sizes).toFixed(2)} MB`);
    console.log(`  CPU avg=${avg(cpuAvg).toFixed(1)}% max=${avg(cpuMax).toFixed(1)}%  RSS max=${avg(rssMax).toFixed(1)} MB`);
    console.log(`  Disk r avg=${avg(rAvg).toFixed(1)}MB/s max=${avg(rMax).toFixed(1)}MB/s  w avg=${avg(wAvg).toFixed(1)}MB/s max=${avg(wMax).toFixed(1)}MB/s`);
    return { wall: avg(walls), meta: avg(meta), bm25: avg(bm25) };
  }

  const baseSummary = summarize('baseline', baseline);
  if (piped) {
    const pipeSummary = summarize('pipeline', piped);
    const delta = (a,b) => (b - a);
    console.log(`\nComparison (pipeline - baseline):`);
    console.log(`  Wall delta: ${delta(baseSummary.wall, pipeSummary.wall).toFixed(1)} ms`);
    console.log(`  BM25 delta: ${delta(baseSummary.bm25, pipeSummary.bm25).toFixed(2)} ms  Meta delta: ${delta(baseSummary.meta, pipeSummary.meta).toFixed(2)} ms`);
  }
}

main().catch(err => { console.error(err); process.exit(1); });
