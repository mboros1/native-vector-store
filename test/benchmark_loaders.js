#!/usr/bin/env node

const { VectorStore } = require('../');
const path = require('path');
const fs = require('fs');

// Helper to drop caches (requires sudo on Linux/macOS)
function dropCaches() {
  if (process.platform === 'linux') {
    try {
      require('child_process').execSync('sudo sync && sudo sh -c "echo 3 > /proc/sys/vm/drop_caches"');
    } catch (e) {
      // Silently fail
    }
  } else if (process.platform === 'darwin') {
    try {
      require('child_process').execSync('sudo purge');
    } catch (e) {
      // Silently fail
    }
  }
}

async function benchmarkLoader(name, loaderFn, docsPath, dropCache = false) {
  console.log(`\n📊 Benchmarking ${name}`);
  
  const runs = 20;
  const times = [];
  
  for (let i = 0; i < runs; i++) {
    if (dropCache) {
      dropCaches();
    }
    
    const store = new VectorStore(1536);
    const start = process.hrtime.bigint();
    
    loaderFn(store, docsPath);
    
    const elapsed = Number(process.hrtime.bigint() - start) / 1e6; // Convert to ms
    times.push(elapsed);
    
    // Only log every 10th run to avoid spam
    if ((i + 1) % 10 === 0) {
      console.log(`   Run ${i + 1}: ${elapsed.toFixed(2)}ms (${store.size()} docs loaded)`);
    }
    
    // Force garbage collection if available
    if (global.gc) {
      global.gc();
    }
  }
  
  // Calculate statistics
  const avg = times.reduce((a, b) => a + b, 0) / times.length;
  const min = Math.min(...times);
  const max = Math.max(...times);
  
  // Calculate standard deviation
  const variance = times.reduce((sum, time) => sum + Math.pow(time - avg, 2), 0) / times.length;
  const stdDev = Math.sqrt(variance);
  
  // Calculate median
  const sorted = [...times].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  
  console.log(`   📈 Results (${runs} runs):`);
  console.log(`      Min: ${min.toFixed(2)}ms`);
  console.log(`      Max: ${max.toFixed(2)}ms`);
  console.log(`      Avg: ${avg.toFixed(2)}ms`);
  console.log(`      Median: ${median.toFixed(2)}ms`);
  console.log(`      Std Dev: ${stdDev.toFixed(2)}ms`);
  
  return { name, avg, min, max, median, stdDev };
}

async function main() {
  console.log('🔥 Vector Store Loader Benchmark\n');
  
  // Test directories
  const testDirs = [
    {
      path: path.join(__dirname, '../out_embedded'),
      name: 'Orchestrator docs (465 files, ~45MB total)'
    },
    {
      path: path.join(__dirname, '../cpp_std_embedded'),
      name: 'C++ standard (2 files, ~340MB total)'
    },
    {
      path: path.join(__dirname, '../cpp_std_partitioned'),
      name: 'C++ standard partitioned (66 files, ~340MB total, 100 docs/file)'
    }
  ];
  
  for (const testDir of testDirs) {
    if (!fs.existsSync(testDir.path)) {
      console.log(`\n⚠️  Skipping ${testDir.name}: directory not found`);
      continue;
    }
    
    console.log(`\n\n🗂️  Testing with: ${testDir.name}`);
    console.log(`   Path: ${testDir.path}`);
    
    const results = [];
    
    // Test standard loader
    results.push(await benchmarkLoader(
      'Standard loader (with optimizations)',
      (store, path) => store.loadDir(path),
      testDir.path,
      false // Don't drop cache for first comparison
    ));
    
    // Test with cold cache
    console.log('\n--- With cold cache ---');
    results.push(await benchmarkLoader(
      'Standard loader (cold cache)',
      (store, path) => store.loadDir(path),
      testDir.path,
      true
    ));
    
    // Test memory-mapped loader
    results.push(await benchmarkLoader(
      'Memory-mapped loader (cold cache)',
      (store, path) => store.loadDirMMap(path),
      testDir.path,
      true
    ));
    
    // Test adaptive loader
    results.push(await benchmarkLoader(
      'Adaptive loader (cold cache)',
      (store, path) => store.loadDirAdaptive(path),
      testDir.path,
      true
    ));
    
    // Summary
    console.log('\n📊 Summary:');
    for (const result of results) {
      console.log(`   ${result.name}: ${result.avg.toFixed(2)}ms avg`);
    }
    
    // Calculate speedup
    if (results.length >= 2) {
      const baseline = results[1].avg; // Cold cache standard
      for (let i = 2; i < results.length; i++) {
        const speedup = baseline / results[i].avg;
        console.log(`   → ${results[i].name} is ${speedup.toFixed(2)}x faster`);
      }
    }
  }
  
  console.log('\n✅ Benchmark complete!\n');
}

if (require.main === module) {
  main().catch(console.error);
}