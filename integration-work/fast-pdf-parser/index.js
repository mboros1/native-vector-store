'use strict';

const { HierarchicalChunker } = require('node-gyp-build')(__dirname);

module.exports = { HierarchicalChunker };

// Convenience function for one-shot chunking
module.exports.chunkPdf = function(pdfPath, options = {}) {
    const chunker = new HierarchicalChunker(options);
    return chunker.chunkFile(pdfPath, options.pageLimit);
};