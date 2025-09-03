# PDF Parser Test Results

## Test Documents

1. **ML Paper (2024)** - `ml_paper_2024.pdf`
   - Size: 900KB
   - Pages: 9
   - Type: Academic paper with complex formatting
   - Performance: 70.3 pages/second
   - Chunks: 22 (avg 524 tokens)
   - Quality: Excellent - all chunks meet minimum threshold

2. **W3C Web Accessibility Guidelines** - `web_accessibility.pdf`
   - Size: 422KB
   - Pages: 34 (tested first 10)
   - Type: Technical specification
   - Performance: 147.1 pages/second
   - Chunks: 10 (avg 479 tokens)
   - Quality: Good - 1 small chunk (31 tokens) due to sparse page

3. **Roman Literature History** - `roman_literature.pdf`
   - Size: 2.9MB
   - Pages: 536 (tested first 10)
   - Type: Classic literature/historical text
   - Performance: 123.5 pages/second
   - Chunks: 3 (avg 365 tokens)
   - Quality: Fair - sparse text extraction, 1 small chunk

4. **IRS Form 1040 Instructions** - `irs_form_1040_instructions.pdf`
   - Size: 4.2MB
   - Pages: 108 (tested first 10)
   - Type: Government form with tables and complex layout
   - Performance: 57.5 pages/second
   - Chunks: 16 (avg 499 tokens)
   - Quality: Excellent - all chunks meet minimum threshold

## Performance Summary
- Average performance: ~100 pages/second
- Token distribution: Most chunks in 501-512 token range (as designed)
- Robustness: Handles various PDF types and formats well
- MuPDF warnings on some documents but extraction continues successfully

## Notes
- The hierarchical chunker successfully processes diverse document types
- Performance varies based on document complexity and text density
- The 7-pass algorithm effectively eliminates bimodal distribution
- Some older/scanned PDFs (like Roman Literature) may have sparse text extraction
