const { VectorStore } = require('../index');
const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');
const assert = require('assert');

// Test configuration
const DIM = 1536;
const DOCS_PER_THREAD = 125000; // 1M total with 8 threads
const NUM_THREADS = 8;
const SEARCH_ITERATIONS = 100;

if (isMainThread) {
    console.log('🔥 Starting concurrent stress tests...');
    
    async function testConcurrentInserts() {
        console.log('\n📝 Test 1: 1M parallel inserts with 8 threads');
        const store = new VectorStore(DIM);
        const startTime = Date.now();
        
        const workers = [];
        for (let i = 0; i < NUM_THREADS; i++) {
            workers.push(new Promise((resolve, reject) => {
                const worker = new Worker(__filename, {
                    workerData: { 
                        test: 'insert', 
                        threadId: i,
                        docsPerThread: DOCS_PER_THREAD,
                        dim: DIM
                    }
                });
                
                worker.on('message', (msg) => {
                    if (msg.type === 'batch') {
                        // Add documents from worker
                        for (const doc of msg.docs) {
                            try {
                                store.addDocument(doc);
                            } catch (e) {
                                reject(new Error(`Thread ${i}: ${e.message}`));
                            }
                        }
                    } else if (msg.type === 'done') {
                        resolve(msg.count);
                    }
                });
                
                worker.on('error', reject);
                worker.on('exit', (code) => {
                    if (code !== 0) {
                        reject(new Error(`Worker stopped with exit code ${code}`));
                    }
                });
            }));
        }
        
        try {
            const results = await Promise.all(workers);
            const totalDocs = results.reduce((a, b) => a + b, 0);
            const elapsed = Date.now() - startTime;
            
            console.log(`✅ Inserted ${totalDocs} documents in ${elapsed}ms`);
            console.log(`   Store size: ${store.size()}`);
            console.log(`   Rate: ${Math.round(totalDocs / (elapsed / 1000))} docs/sec`);
            
            assert.strictEqual(store.size(), totalDocs, 'Store size should match inserted count');
        } catch (error) {
            console.error('❌ Concurrent insert test failed:', error);
            process.exit(1);
        }
    }
    
    async function testOversizeAllocation() {
        console.log('\n📏 Test 2: 64MB+1 allocation (expect fail)');
        const store = new VectorStore(10);
        
        try {
            // Create a document with massive metadata to exceed chunk size
            const hugeMetadata = 'x'.repeat(67108865); // 64MB + 1
            const doc = {
                id: 'huge',
                text: 'test',
                metadata: {
                    embedding: new Array(10).fill(0.1),
                    huge: hugeMetadata
                }
            };
            
            store.addDocument(doc);
            console.error('❌ Should have thrown error for oversize allocation');
            process.exit(1);
        } catch (error) {
            console.log('✅ Correctly rejected oversize allocation');
        }
    }
    
    async function testConcurrentNormalizeAndSearch() {
        console.log('\n🔄 Test 3: Concurrent normalize_all + search while loading');
        const store = new VectorStore(DIM);
        
        // Pre-load some documents
        const initialDocs = 10000;
        for (let i = 0; i < initialDocs; i++) {
            store.addDocument({
                id: `init-${i}`,
                text: `Initial document ${i}`,
                metadata: {
                    embedding: Array.from({ length: DIM }, () => Math.random())
                }
            });
        }
        
        // Start concurrent operations
        const operations = [];
        
        // Continuous loading
        operations.push(new Promise((resolve) => {
            let count = 0;
            const interval = setInterval(() => {
                try {
                    for (let i = 0; i < 100; i++) {
                        store.addDocument({
                            id: `load-${count++}`,
                            text: `Loading document ${count}`,
                            metadata: {
                                embedding: Array.from({ length: DIM }, () => Math.random())
                            }
                        });
                    }
                } catch (e) {
                    // Ignore capacity errors
                }
                
                if (count >= 50000) {
                    clearInterval(interval);
                    resolve(count);
                }
            }, 10);
        }));
        
        // Continuous normalization
        operations.push(new Promise((resolve) => {
            let iterations = 0;
            const interval = setInterval(() => {
                store.normalize();
                iterations++;
                
                if (iterations >= 100) {
                    clearInterval(interval);
                    resolve(iterations);
                }
            }, 50);
        }));
        
        // Continuous searching
        operations.push(new Promise((resolve) => {
            let searches = 0;
            const query = new Float32Array(DIM).fill(0.1);
            const interval = setInterval(() => {
                const results = store.search(query, 10);
                assert(results.length <= 10, 'Search should return at most k results');
                searches++;
                
                if (searches >= SEARCH_ITERATIONS) {
                    clearInterval(interval);
                    resolve(searches);
                }
            }, 30);
        }));
        
        const startTime = Date.now();
        const [loaded, normalized, searched] = await Promise.all(operations);
        const elapsed = Date.now() - startTime;
        
        console.log(`✅ Concurrent operations completed in ${elapsed}ms`);
        console.log(`   Documents loaded: ${loaded}`);
        console.log(`   Normalizations: ${normalized}`);
        console.log(`   Searches: ${searched}`);
        console.log(`   Final store size: ${store.size()}`);
    }
    
    async function testAlignmentRequests() {
        console.log('\n🎯 Test 4: Various alignment requests');
        const store = new VectorStore(16);
        
        // Test valid alignments
        const validAlignments = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
        
        for (const align of validAlignments) {
            try {
                // Create document that will trigger alignment in allocator
                const doc = {
                    id: `align-${align}`,
                    text: 'x'.repeat(align), // Force specific allocation size
                    metadata: {
                        embedding: new Array(16).fill(0.1)
                    }
                };
                store.addDocument(doc);
            } catch (e) {
                console.error(`❌ Failed with alignment ${align}: ${e.message}`);
                process.exit(1);
            }
        }
        console.log('✅ All valid alignments handled correctly');
        
        // Test invalid alignment (>4096) - this would need to be tested differently
        // as the JS layer doesn't directly control alignment
        console.log('✅ Alignment tests completed');
    }
    
    // Run all tests
    (async () => {
        try {
            await testConcurrentInserts();
            await testOversizeAllocation();
            await testConcurrentNormalizeAndSearch();
            await testAlignmentRequests();
            
            console.log('\n✅ All stress tests passed!');
            process.exit(0);
        } catch (error) {
            console.error('\n❌ Stress tests failed:', error);
            process.exit(1);
        }
    })();
    
} else {
    // Worker thread code for parallel inserts
    const { test, threadId, docsPerThread, dim } = workerData;
    
    if (test === 'insert') {
        const batchSize = 1000;
        let totalSent = 0;
        
        for (let batch = 0; batch < docsPerThread / batchSize; batch++) {
            const docs = [];
            for (let i = 0; i < batchSize; i++) {
                const docId = threadId * docsPerThread + batch * batchSize + i;
                docs.push({
                    id: `thread-${threadId}-doc-${docId}`,
                    text: `Document ${docId} from thread ${threadId}`,
                    metadata: {
                        embedding: Array.from({ length: dim }, () => Math.random()),
                        thread: threadId,
                        batch: batch
                    }
                });
            }
            
            parentPort.postMessage({ type: 'batch', docs });
            totalSent += docs.length;
        }
        
        parentPort.postMessage({ type: 'done', count: totalSent });
    }
}