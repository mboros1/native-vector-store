# Usage Guide

This guide provides comprehensive examples and patterns for using native-vector-store in production environments.

## Table of Contents

- [Core Concepts](#core-concepts)
- [Document Format](#document-format)
- [Loading Strategies](#loading-strategies)
- [Search Patterns](#search-patterns)
- [Production Deployments](#production-deployments)
- [Error Handling](#error-handling)
- [Performance Optimization](#performance-optimization)
- [Migration Patterns](#migration-patterns)

## Core Concepts

### Immutable Design

The vector store follows an immutable, two-phase lifecycle:

1. **Loading Phase**: Add documents via `loadDir()` or `addDocument()`
2. **Serving Phase**: After `finalize()`, perform searches but no modifications

This design ensures:
- Predictable performance
- Thread-safe searches
- Simple deployment model
- No synchronization overhead

### File-Based Loading

`loadDir()` is the primary loading mechanism and **is your batch operation**:

```javascript
// This loads thousands of documents in parallel at 178k+ docs/sec
store.loadDir('./knowledge-base');

// Equivalent to (but much faster than):
for (const doc of documents) {
  store.addDocument(doc);
}
store.finalize();
```

## Document Format

### Single Document

```json
{
  "id": "doc-123",
  "text": "The quick brown fox jumps over the lazy dog.",
  "metadata": {
    "embedding": [0.1, 0.2, 0.3, ...],
    "category": "example",
    "source": "training-data",
    "timestamp": "2024-01-01T00:00:00Z"
  }
}
```

### Array of Documents

```json
[
  {
    "id": "doc-1",
    "text": "First document",
    "metadata": {
      "embedding": [...]
    }
  },
  {
    "id": "doc-2",
    "text": "Second document",
    "metadata": {
      "embedding": [...]
    }
  }
]
```

### Embedding Generation Example

```javascript
const OpenAI = require('openai');
const fs = require('fs').promises;

async function generateEmbeddings(texts) {
  const openai = new OpenAI();
  const response = await openai.embeddings.create({
    model: "text-embedding-3-small",
    input: texts,
  });
  
  return response.data.map(item => item.embedding);
}

async function prepareDocuments(inputFile, outputFile) {
  const content = await fs.readFile(inputFile, 'utf-8');
  const documents = JSON.parse(content);
  
  // Generate embeddings in batches
  const embeddings = await generateEmbeddings(
    documents.map(doc => doc.text)
  );
  
  // Add embeddings to documents
  documents.forEach((doc, i) => {
    doc.metadata = doc.metadata || {};
    doc.metadata.embedding = embeddings[i];
  });
  
  await fs.writeFile(outputFile, JSON.stringify(documents, null, 2));
}
```

## Loading Strategies

### Strategy 1: Organized Directories

Best for: Large knowledge bases with natural categories

```javascript
const store = new VectorStore(1536);

// Load entire knowledge base
store.loadDir('./knowledge-base');

// Directory structure:
// knowledge-base/
// ├── products/
// │   ├── electronics.json     (5000 docs)
// │   ├── clothing.json         (3000 docs)
// │   └── home-goods.json       (2000 docs)
// ├── support/
// │   ├── faqs.json            (500 docs)
// │   └── troubleshooting.json  (1500 docs)
// └── policies/
//     └── terms.json            (100 docs)
```

### Strategy 2: Chunked Large Files

Best for: Very large datasets

```javascript
// Split large dataset into chunks for optimal loading
const CHUNK_SIZE = 5000; // Documents per file

async function chunkDocuments(allDocs, outputDir) {
  for (let i = 0; i < allDocs.length; i += CHUNK_SIZE) {
    const chunk = allDocs.slice(i, i + CHUNK_SIZE);
    const filename = `${outputDir}/chunk-${i / CHUNK_SIZE}.json`;
    await fs.writeFile(filename, JSON.stringify(chunk));
  }
}

// Load chunked documents
const store = new VectorStore(1536);
store.loadDir('./chunked-docs');
```

### Strategy 3: Dynamic Loading

Best for: Development and testing

```javascript
async function loadFromMultipleSources(store) {
  // Load base knowledge
  store.loadDir('./base-knowledge');
  
  // Add dynamic content
  const apiDocs = await fetchFromAPI();
  apiDocs.forEach(doc => store.addDocument(doc));
  
  // Add user-specific content
  const userDocs = await loadUserDocuments(userId);
  userDocs.forEach(doc => store.addDocument(doc));
  
  // Finalize when all sources are loaded
  store.finalize();
}
```

## Search Patterns

### Basic Search

```javascript
const results = store.search(queryEmbedding, 10);

// Filter by score threshold
const relevantResults = results.filter(r => r.score > 0.7);
```

### Search with Context

```javascript
function searchWithContext(store, embedding, k = 10) {
  const results = store.search(embedding, k);
  
  return results.map(result => {
    const metadata = JSON.parse(result.metadata_json);
    return {
      ...result,
      category: metadata.category,
      source: metadata.source,
      context: extractContext(result.text, 200) // 200 char context
    };
  });
}

function extractContext(text, maxLength) {
  if (text.length <= maxLength) return text;
  return text.substring(0, maxLength) + '...';
}
```

### Multi-Query Search

```javascript
async function multiQuerySearch(store, queries, k = 5) {
  const allResults = new Map();
  
  for (const query of queries) {
    const embedding = await generateEmbedding(query);
    const results = store.search(embedding, k * 2); // Get more results
    
    // Aggregate scores
    results.forEach(result => {
      const existing = allResults.get(result.id);
      if (existing) {
        existing.score += result.score;
        existing.count += 1;
      } else {
        allResults.set(result.id, {
          ...result,
          count: 1
        });
      }
    });
  }
  
  // Sort by average score
  return Array.from(allResults.values())
    .map(r => ({ ...r, avgScore: r.score / r.count }))
    .sort((a, b) => b.avgScore - a.avgScore)
    .slice(0, k);
}
```

### Semantic Filtering

```javascript
function searchWithFilter(store, embedding, filter, k = 10) {
  // Get more results than needed
  const results = store.search(embedding, k * 3);
  
  // Apply semantic filter
  return results
    .filter(result => {
      const metadata = JSON.parse(result.metadata_json);
      return filter(metadata);
    })
    .slice(0, k);
}

// Usage
const productResults = searchWithFilter(
  store,
  queryEmbedding,
  (metadata) => metadata.category === 'products',
  10
);
```

## Production Deployments

### AWS Lambda

```javascript
// handler.js
let store;

async function initStore() {
  if (!store) {
    console.time('VectorStore initialization');
    store = new VectorStore(1536);
    
    // Load from bundled files or S3
    if (process.env.USE_S3) {
      await downloadFromS3('/tmp/knowledge');
      store.loadDir('/tmp/knowledge');
    } else {
      store.loadDir('./knowledge');
    }
    
    console.timeEnd('VectorStore initialization');
    console.log(`Loaded ${store.size()} documents`);
  }
  return store;
}

exports.handler = async (event) => {
  const store = await initStore();
  
  try {
    const { embedding, k = 10, threshold = 0.5 } = JSON.parse(event.body);
    const results = store.search(new Float32Array(embedding), k);
    
    return {
      statusCode: 200,
      body: JSON.stringify({
        results: results.filter(r => r.score > threshold),
        total: results.length
      })
    };
  } catch (error) {
    return {
      statusCode: 400,
      body: JSON.stringify({ error: error.message })
    };
  }
};
```

### Docker Container

```dockerfile
FROM node:18-alpine

# Install runtime dependencies
RUN apk add --no-cache libgomp

WORKDIR /app

# Copy and install dependencies
COPY package*.json ./
RUN npm ci --only=production

# Copy application and knowledge base
COPY . .
COPY knowledge-base ./knowledge-base

# Pre-initialize store during build
RUN node -e "
  const { VectorStore } = require('native-vector-store');
  const store = new VectorStore(1536);
  store.loadDir('./knowledge-base');
  console.log('Pre-loaded', store.size(), 'documents');
"

EXPOSE 3000
CMD ["node", "server.js"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: vector-search-api
spec:
  replicas: 3
  selector:
    matchLabels:
      app: vector-search
  template:
    metadata:
      labels:
        app: vector-search
    spec:
      initContainers:
      - name: knowledge-loader
        image: busybox
        command:
        - wget
        - "-O"
        - "/knowledge/data.tar.gz"
        - "https://storage.example.com/knowledge-base-v1.tar.gz"
        volumeMounts:
        - name: knowledge
          mountPath: /knowledge
      containers:
      - name: api
        image: myapp:latest
        env:
        - name: KNOWLEDGE_PATH
          value: /knowledge/data
        volumeMounts:
        - name: knowledge
          mountPath: /knowledge
          readOnly: true
        resources:
          requests:
            memory: "2Gi"
            cpu: "1"
          limits:
            memory: "4Gi"
            cpu: "2"
      volumes:
      - name: knowledge
        emptyDir: {}
```

## Error Handling

### Loading Errors

```javascript
class VectorStoreService {
  constructor(dimensions) {
    this.dimensions = dimensions;
    this.store = null;
  }
  
  async initialize(knowledgePath) {
    try {
      this.store = new VectorStore(this.dimensions);
      
      // Check if path exists
      if (!fs.existsSync(knowledgePath)) {
        throw new Error(`Knowledge path not found: ${knowledgePath}`);
      }
      
      // Load with error handling
      console.log(`Loading documents from ${knowledgePath}`);
      this.store.loadDir(knowledgePath);
      
      // Validate
      if (this.store.size() === 0) {
        throw new Error('No documents loaded');
      }
      
      console.log(`Successfully loaded ${this.store.size()} documents`);
      return true;
      
    } catch (error) {
      console.error('Failed to initialize vector store:', error);
      this.store = null;
      throw error;
    }
  }
  
  search(embedding, k, options = {}) {
    if (!this.store) {
      throw new Error('Vector store not initialized');
    }
    
    if (!embedding || embedding.length !== this.dimensions) {
      throw new Error(`Invalid embedding: expected ${this.dimensions} dimensions`);
    }
    
    try {
      const results = this.store.search(
        new Float32Array(embedding),
        k,
        options.normalizeQuery ?? true
      );
      
      return {
        success: true,
        results: results.filter(r => r.score > (options.threshold || 0)),
        total: results.length
      };
      
    } catch (error) {
      console.error('Search error:', error);
      return {
        success: false,
        error: error.message,
        results: []
      };
    }
  }
}
```

### Graceful Degradation

```javascript
// Fallback search service
class FallbackSearchService {
  constructor(primary, fallback) {
    this.primary = primary;
    this.fallback = fallback;
  }
  
  async search(embedding, k) {
    try {
      // Try primary store first
      return await this.primary.search(embedding, k);
    } catch (primaryError) {
      console.warn('Primary search failed, using fallback:', primaryError);
      
      try {
        // Fall back to secondary store
        return await this.fallback.search(embedding, k);
      } catch (fallbackError) {
        console.error('Both stores failed:', fallbackError);
        
        // Return empty results as last resort
        return {
          success: false,
          results: [],
          error: 'Search temporarily unavailable'
        };
      }
    }
  }
}
```

## Performance Optimization

### Memory Management

```javascript
// Monitor memory usage
function getMemoryStats() {
  const used = process.memoryUsage();
  return {
    rss: Math.round(used.rss / 1024 / 1024) + 'MB',
    heap: Math.round(used.heapUsed / 1024 / 1024) + 'MB',
    external: Math.round(used.external / 1024 / 1024) + 'MB'
  };
}

// Load with memory monitoring
console.log('Memory before loading:', getMemoryStats());
store.loadDir('./knowledge-base');
console.log('Memory after loading:', getMemoryStats());
```

### Query Optimization

```javascript
// Cache normalized queries
class OptimizedSearcher {
  constructor(store) {
    this.store = store;
    this.queryCache = new Map();
  }
  
  normalizeEmbedding(embedding) {
    // L2 normalization
    const arr = new Float32Array(embedding);
    let sum = 0;
    for (let i = 0; i < arr.length; i++) {
      sum += arr[i] * arr[i];
    }
    const norm = Math.sqrt(sum);
    for (let i = 0; i < arr.length; i++) {
      arr[i] /= norm;
    }
    return arr;
  }
  
  search(embedding, k, useCache = true) {
    const key = embedding.toString();
    
    if (useCache && this.queryCache.has(key)) {
      return this.queryCache.get(key);
    }
    
    // Pre-normalize for multiple searches
    const normalized = this.normalizeEmbedding(embedding);
    const results = this.store.search(normalized, k, false);
    
    if (useCache) {
      this.queryCache.set(key, results);
      // Limit cache size
      if (this.queryCache.size > 1000) {
        const firstKey = this.queryCache.keys().next().value;
        this.queryCache.delete(firstKey);
      }
    }
    
    return results;
  }
}
```

### Batch Processing

```javascript
// Process multiple queries efficiently
async function batchSearch(store, queries, k = 10) {
  const results = [];
  
  // Process in chunks to avoid blocking
  const BATCH_SIZE = 100;
  
  for (let i = 0; i < queries.length; i += BATCH_SIZE) {
    const batch = queries.slice(i, i + BATCH_SIZE);
    
    // Process batch in parallel
    const batchResults = await Promise.all(
      batch.map(async (query) => {
        const embedding = await generateEmbedding(query);
        return {
          query,
          results: store.search(embedding, k)
        };
      })
    );
    
    results.push(...batchResults);
    
    // Yield to event loop
    await new Promise(resolve => setImmediate(resolve));
  }
  
  return results;
}
```

## Migration Patterns

### Version Migration

```javascript
// Migrate between versions without downtime
class VectorStoreManager {
  constructor() {
    this.stores = new Map();
    this.activeVersion = null;
  }
  
  async loadVersion(version, path) {
    console.log(`Loading version ${version} from ${path}`);
    
    const store = new VectorStore(1536);
    store.loadDir(path);
    
    this.stores.set(version, {
      store,
      loadedAt: new Date(),
      documentCount: store.size()
    });
    
    console.log(`Version ${version} loaded with ${store.size()} documents`);
  }
  
  setActiveVersion(version) {
    if (!this.stores.has(version)) {
      throw new Error(`Version ${version} not loaded`);
    }
    
    const oldVersion = this.activeVersion;
    this.activeVersion = version;
    
    console.log(`Switched from version ${oldVersion} to ${version}`);
    
    // Clean up old versions after delay
    if (oldVersion && oldVersion !== version) {
      setTimeout(() => {
        this.stores.delete(oldVersion);
        console.log(`Cleaned up version ${oldVersion}`);
      }, 60000); // 1 minute delay
    }
  }
  
  search(embedding, k) {
    if (!this.activeVersion) {
      throw new Error('No active version');
    }
    
    return this.stores.get(this.activeVersion).store.search(embedding, k);
  }
}

// Usage
const manager = new VectorStoreManager();

// Load new version
await manager.loadVersion('v2', './knowledge-v2');

// Switch when ready
manager.setActiveVersion('v2');
```

### A/B Testing

```javascript
// Test different document sets
class ABTestStore {
  constructor(storeA, storeB, splitRatio = 0.5) {
    this.storeA = storeA;
    this.storeB = storeB;
    this.splitRatio = splitRatio;
    this.metrics = {
      a: { searches: 0, totalScore: 0 },
      b: { searches: 0, totalScore: 0 }
    };
  }
  
  search(embedding, k, userId) {
    // Consistent assignment based on user ID
    const useA = hashCode(userId) % 100 < this.splitRatio * 100;
    const store = useA ? this.storeA : this.storeB;
    const metrics = useA ? this.metrics.a : this.metrics.b;
    
    const results = store.search(embedding, k);
    
    // Track metrics
    metrics.searches++;
    metrics.totalScore += results.reduce((sum, r) => sum + r.score, 0);
    
    return {
      results,
      variant: useA ? 'A' : 'B'
    };
  }
  
  getMetrics() {
    return {
      a: {
        ...this.metrics.a,
        avgScore: this.metrics.a.totalScore / this.metrics.a.searches
      },
      b: {
        ...this.metrics.b,
        avgScore: this.metrics.b.totalScore / this.metrics.b.searches
      }
    };
  }
}

function hashCode(str) {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return Math.abs(hash);
}
```

## Advanced Patterns

### Hierarchical Search

```javascript
// Search with category-specific stores
class HierarchicalSearch {
  constructor() {
    this.stores = new Map();
    this.globalStore = null;
  }
  
  addCategoryStore(category, store) {
    this.stores.set(category, store);
  }
  
  setGlobalStore(store) {
    this.globalStore = store;
  }
  
  search(embedding, k, options = {}) {
    const results = [];
    
    // Search category-specific store if specified
    if (options.category && this.stores.has(options.category)) {
      const categoryResults = this.stores.get(options.category)
        .search(embedding, k);
      results.push(...categoryResults.map(r => ({
        ...r,
        source: options.category
      })));
    }
    
    // Search global store
    if (this.globalStore && !options.categoryOnly) {
      const globalResults = this.globalStore
        .search(embedding, k);
      results.push(...globalResults.map(r => ({
        ...r,
        source: 'global'
      })));
    }
    
    // Deduplicate and sort by score
    const seen = new Set();
    return results
      .filter(r => {
        if (seen.has(r.id)) return false;
        seen.add(r.id);
        return true;
      })
      .sort((a, b) => b.score - a.score)
      .slice(0, k);
  }
}
```

### Hybrid Search

```javascript
// Combine vector search with keyword filtering
class HybridSearch {
  constructor(vectorStore) {
    this.vectorStore = vectorStore;
    this.keywordIndex = new Map(); // Simple inverted index
  }
  
  buildKeywordIndex(documents) {
    documents.forEach(doc => {
      const words = doc.text.toLowerCase().split(/\s+/);
      words.forEach(word => {
        if (!this.keywordIndex.has(word)) {
          this.keywordIndex.set(word, new Set());
        }
        this.keywordIndex.get(word).add(doc.id);
      });
    });
  }
  
  search(embedding, keywords, k) {
    // Get vector search results
    const vectorResults = this.vectorStore.search(embedding, k * 3);
    
    // Filter by keywords if provided
    if (keywords && keywords.length > 0) {
      const keywordMatches = new Set();
      
      keywords.forEach(keyword => {
        const matches = this.keywordIndex.get(keyword.toLowerCase());
        if (matches) {
          matches.forEach(id => keywordMatches.add(id));
        }
      });
      
      // Combine vector and keyword scores
      return vectorResults
        .map(result => ({
          ...result,
          hybridScore: result.score * (keywordMatches.has(result.id) ? 1.5 : 1)
        }))
        .sort((a, b) => b.hybridScore - a.hybridScore)
        .slice(0, k);
    }
    
    return vectorResults.slice(0, k);
  }
}
```

## Troubleshooting

### Common Issues

1. **"Cannot add documents after finalization"**
   - Solution: Create a new store instance for updates
   - Use versioned deployments for updates

2. **Memory spikes during loading**
   - Solution: Split large files into chunks
   - Use streaming for very large datasets

3. **Slow search performance**
   - Check if store is finalized
   - Reduce k value for interactive use
   - Consider sharding for >1M documents

4. **Inconsistent search results**
   - Ensure embeddings are normalized consistently
   - Check embedding dimensions match

### Debug Helpers

```javascript
// Debug store state
function debugStore(store) {
  console.log({
    size: store.size(),
    isFinalized: store.isFinalized(),
    memoryUsage: process.memoryUsage(),
    nodeVersion: process.version
  });
}

// Validate document format
function validateDocument(doc) {
  const errors = [];
  
  if (!doc.id) errors.push('Missing id');
  if (!doc.text) errors.push('Missing text');
  if (!doc.metadata?.embedding) errors.push('Missing embedding');
  if (doc.metadata?.embedding?.length !== 1536) {
    errors.push(`Wrong embedding size: ${doc.metadata?.embedding?.length}`);
  }
  
  return errors.length === 0 ? null : errors;
}

// Performance profiler
class SearchProfiler {
  constructor(store) {
    this.store = store;
    this.timings = [];
  }
  
  search(embedding, k) {
    const start = process.hrtime.bigint();
    const results = this.store.search(embedding, k);
    const end = process.hrtime.bigint();
    
    const duration = Number(end - start) / 1_000_000; // Convert to ms
    this.timings.push(duration);
    
    return results;
  }
  
  getStats() {
    const sorted = this.timings.sort((a, b) => a - b);
    return {
      count: sorted.length,
      min: sorted[0],
      max: sorted[sorted.length - 1],
      avg: sorted.reduce((a, b) => a + b, 0) / sorted.length,
      p50: sorted[Math.floor(sorted.length * 0.5)],
      p95: sorted[Math.floor(sorted.length * 0.95)],
      p99: sorted[Math.floor(sorted.length * 0.99)]
    };
  }
}
```