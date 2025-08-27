// This file explains the migration from VectorStore v1 to VectorStoreV2

console.log(`
╔════════════════════════════════════════════════════════════╗
║                    MIGRATION NOTICE                         ║
╠════════════════════════════════════════════════════════════╣
║  This test file uses the old VectorStore v1 API which has   ║
║  been replaced by VectorStoreV2.                           ║
║                                                             ║
║  Key differences:                                           ║
║  • v1: Runtime loading with addDocument()                  ║
║  • v2: Pre-built bundles loaded from disk                  ║
║                                                             ║
║  The new API requires:                                     ║
║  1. Creating JSON files with documents                     ║
║  2. Using nvs-pack to create a bundle                      ║
║  3. Loading the bundle with new VectorStore(bundlePath)    ║
║                                                             ║
║  See test/test.js for the updated test implementation.     ║
╚════════════════════════════════════════════════════════════╝
`);

process.exit(0);