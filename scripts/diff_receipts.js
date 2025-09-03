#!/usr/bin/env node
// ESM script to diff receipts.txt between two bundles
// Usage: node scripts/diff_receipts.js --a <bundleA> --b <bundleB> [--limit N]

import fs from 'fs/promises';
import path from 'path';

function usage() {
  console.log('Usage: node scripts/diff_receipts.js --a <bundleA> --b <bundleB> [--limit N]');
  process.exit(1);
}

function normKey(p) {
  // Normalize common leading prefixes and separators
  let s = p.replace(/^\.{1,2}\//, '');
  s = s.replace(/\\/g, '/');
  return s;
}

async function readReceipts(dir) {
  const file = path.join(dir, 'receipts.txt');
  const text = await fs.readFile(file, 'utf8');
  const map = new Map();
  let total = 0;
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    const [rawKey, rawVal] = line.split(/\t/);
    if (!rawKey || rawVal == null) continue;
    const key = normKey(rawKey.trim());
    const val = Number(rawVal.trim());
    if (!Number.isFinite(val)) continue;
    map.set(key, val);
    total += val;
  }
  return { map, total };
}

function diffMaps(a, b) {
  const onlyA = [];
  const onlyB = [];
  const changed = [];
  for (const [k, va] of a.map.entries()) {
    if (!b.map.has(k)) { onlyA.push(k); continue; }
    const vb = b.map.get(k);
    if (va !== vb) changed.push([k, va, vb]);
  }
  for (const [k] of b.map.entries()) {
    if (!a.map.has(k)) onlyB.push(k);
  }
  onlyA.sort();
  onlyB.sort();
  changed.sort((x,y)=> x[0].localeCompare(y[0]));
  return { onlyA, onlyB, changed };
}

async function main() {
  const args = process.argv.slice(2);
  const getArg = (name) => {
    const i = args.findIndex(a => a === name || a.startsWith(name+'='));
    if (i === -1) return undefined;
    const v = args[i].includes('=') ? args[i].split('=')[1] : args[i+1];
    return v;
  };
  const dirA = getArg('--a');
  const dirB = getArg('--b');
  const limitStr = getArg('--limit');
  const limit = limitStr ? parseInt(limitStr, 10) : 50;
  if (!dirA || !dirB) usage();

  const [A, B] = await Promise.all([readReceipts(dirA), readReceipts(dirB)]);
  const d = diffMaps(A, B);

  const sum = (iter, m) => iter.reduce((acc, k) => acc + (m.get(k) || 0), 0);

  console.log('Receipts Diff');
  console.log('============');
  console.log(`A: ${dirA}`);
  console.log(`B: ${dirB}`);
  console.log('');
  console.log(`Totals: files A=${A.map.size} B=${B.map.size}  docs A=${A.total} B=${B.total}`);
  console.log('');

  console.log(`Only in A (${d.onlyA.length}):`);
  for (const k of d.onlyA.slice(0, limit)) {
    console.log(`  ${k}\t${A.map.get(k)}`);
  }
  if (d.onlyA.length > limit) console.log(`  ... (${d.onlyA.length - limit} more)`);
  console.log('');

  console.log(`Only in B (${d.onlyB.length}):`);
  for (const k of d.onlyB.slice(0, limit)) {
    console.log(`  ${k}\t${B.map.get(k)}`);
  }
  if (d.onlyB.length > limit) console.log(`  ... (${d.onlyB.length - limit} more)`);
  console.log('');

  console.log(`Changed counts (${d.changed.length}):`);
  for (const [k, va, vb] of d.changed.slice(0, limit)) {
    console.log(`  ${k}\tA:${va}\tB:${vb}\tΔ:${vb - va}`);
  }
  if (d.changed.length > limit) console.log(`  ... (${d.changed.length - limit} more)`);

  console.log('');
  // Summaries for diffs
  const extraA = sum(d.onlyA, A.map);
  const extraB = sum(d.onlyB, B.map);
  const deltaChanged = d.changed.reduce((acc, [k, va, vb]) => acc + (vb - va), 0);
  console.log(`Summary:`);
  console.log(`  Docs only in A: ${extraA}`);
  console.log(`  Docs only in B: ${extraB}`);
  console.log(`  Docs delta over changed: ${deltaChanged}`);
}

main().catch(err => { console.error(err); process.exit(1); });

