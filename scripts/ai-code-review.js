#!/usr/bin/env node

/**
 * AI Code Review Script using OpenAI o1-preview model
 * 
 * This script performs comprehensive code reviews on C++ source files
 * using OpenAI's o1-preview model for enhanced reasoning capabilities.
 * 
 * Usage: node ai-code-review.js [file1.cpp file2.cpp ...] or no args for all C++ files
 */

import OpenAI from 'openai';
import { promises as fs } from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import dotenv from 'dotenv';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load environment variables from .env file
dotenv.config({ path: path.join(__dirname, '..', '.env') });

// Initialize OpenAI client
const openai = new OpenAI({
    apiKey: process.env.OPEN_API_KEY || process.env.OPENAI_API_KEY,
});

// System prompt for code review
const SYSTEM_PROMPT = `You are an expert C++ code reviewer with deep knowledge of:
- Modern C++ (C++17/20/23) best practices
- Performance optimization and SIMD operations
- Memory management and safety
- Concurrency and threading
- Design patterns and architecture
- Error handling and robustness

Please review the provided C++ code and provide a comprehensive analysis covering:

1. **Correctness**
   - Logic errors or bugs
   - Undefined behavior
   - Race conditions or threading issues
   - Memory safety issues

2. **Performance**
   - Algorithmic complexity
   - Cache efficiency
   - SIMD optimization opportunities
   - Unnecessary allocations or copies

3. **Maintainability**
   - Code clarity and readability
   - Proper abstraction levels
   - Documentation quality
   - Naming conventions

4. **Design**
   - Architecture and patterns
   - API design
   - Modularity and coupling
   - Future extensibility

5. **Security**
   - Input validation
   - Buffer overflows
   - Integer overflows
   - Security best practices

6. **Compliance**
   - C++ standard compliance
   - Platform compatibility
   - Compiler-specific issues

Provide specific, actionable feedback with code examples where appropriate.
Rate each category on a scale of 1-10 and provide an overall assessment.`;

/**
 * Get all C++ source files in the src directory
 */
async function getCppFiles() {
    const srcDir = path.join(__dirname, '..', 'src');
    const files = await fs.readdir(srcDir);
    
    return files
        .filter(file => file.endsWith('.cpp') || file.endsWith('.cc') || file.endsWith('.h'))
        .filter(file => !file.startsWith('test_'))  // Skip test files
        .map(file => path.join(srcDir, file));
}

/**
 * Read file content
 */
async function readFile(filePath) {
    try {
        const content = await fs.readFile(filePath, 'utf-8');
        return content;
    } catch (error) {
        console.error(`Error reading file ${filePath}:`, error.message);
        return null;
    }
}

/**
 * Perform code review using OpenAI API
 */
async function reviewCode(filename, code) {
    const startTime = Date.now();
    console.log(`🔍 Reviewing ${filename}...`);
    
    try {
        // Using o1-preview for enhanced reasoning
        // Note: o1 models don't support system messages, so we include it in the user message
        const completion = await openai.chat.completions.create({
            model: 'o1-preview',  // Use o1-preview for better reasoning
            messages: [
                {
                    role: 'user',
                    content: `${SYSTEM_PROMPT}\n\n---\n\nFile: ${filename}\n\n\`\`\`cpp\n${code}\n\`\`\`\n\nPlease provide a comprehensive code review.`
                }
            ],
            max_completion_tokens: 4096,
            // o1-preview specific: reasoning_effort can be adjusted if needed
            // reasoning_effort: 'medium'  // 'low', 'medium', or 'high'
        });
        
        const elapsedTime = ((Date.now() - startTime) / 1000).toFixed(1);
        console.log(`✅ Review completed for ${filename} (${elapsedTime}s)`);
        
        return completion.choices[0].message.content;
    } catch (error) {
        console.error(`❌ Error reviewing ${filename}:`, error.message);
        
        // Fallback to gpt-4o if o1-preview is not available or rate limited
        if (error.status === 429 || error.status === 404) {
            console.log(`⚠️ Falling back to gpt-4o for ${filename}...`);
            
            try {
                const completion = await openai.chat.completions.create({
                    model: 'gpt-4o',
                    messages: [
                        {
                            role: 'system',
                            content: SYSTEM_PROMPT
                        },
                        {
                            role: 'user',
                            content: `File: ${filename}\n\n\`\`\`cpp\n${code}\n\`\`\`\n\nPlease provide a comprehensive code review.`
                        }
                    ],
                    max_tokens: 4096,
                    temperature: 0.3  // Lower temperature for more focused analysis
                });
                
                const elapsedTime = ((Date.now() - startTime) / 1000).toFixed(1);
                console.log(`✅ Review completed with gpt-4o for ${filename} (${elapsedTime}s)`);
                
                return completion.choices[0].message.content;
            } catch (fallbackError) {
                console.error(`❌ Fallback also failed for ${filename}:`, fallbackError.message);
                return null;
            }
        }
        
        return null;
    }
}

/**
 * Save review to markdown file
 */
async function saveReview(filename, review) {
    const reviewsDir = path.join(__dirname, '..', 'code-reviews');
    
    // Create reviews directory if it doesn't exist
    try {
        await fs.mkdir(reviewsDir, { recursive: true });
    } catch (error) {
        // Directory might already exist
    }
    
    const basename = path.basename(filename, path.extname(filename));
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
    const outputFile = path.join(reviewsDir, `${basename}_review_${timestamp}.md`);
    
    const markdown = `# Code Review: ${path.basename(filename)}

**Date**: ${new Date().toISOString()}
**Model**: OpenAI o1-preview
**File**: ${filename}

---

${review}

---

*Generated by AI Code Review Script*
`;
    
    await fs.writeFile(outputFile, markdown, 'utf-8');
    console.log(`📝 Review saved to ${outputFile}`);
    
    return outputFile;
}

/**
 * Main execution
 */
async function main() {
    console.log('🚀 AI Code Review Script');
    console.log('========================\n');
    
    // Check API key
    if (!process.env.OPEN_API_KEY && !process.env.OPENAI_API_KEY) {
        console.error('❌ Error: OpenAI API key not found in .env file');
        console.error('Please set OPEN_API_KEY or OPENAI_API_KEY in your .env file');
        process.exit(1);
    }
    
    // Get files to review
    let filesToReview = [];
    
    if (process.argv.length > 2) {
        // Use provided files
        filesToReview = process.argv.slice(2).map(f => path.resolve(f));
    } else {
        // Get all C++ files
        filesToReview = await getCppFiles();
        console.log(`Found ${filesToReview.length} C++ files to review\n`);
    }
    
    // Process each file
    const results = [];
    
    for (const file of filesToReview) {
        const code = await readFile(file);
        
        if (!code) {
            console.log(`⚠️ Skipping ${file} (could not read)\n`);
            continue;
        }
        
        // Check file size (o1-preview has token limits)
        const lines = code.split('\n').length;
        if (lines > 2000) {
            console.log(`⚠️ Warning: ${file} has ${lines} lines, may exceed token limit\n`);
        }
        
        const review = await reviewCode(path.basename(file), code);
        
        if (review) {
            const outputFile = await saveReview(file, review);
            results.push({ file, outputFile, success: true });
        } else {
            results.push({ file, success: false });
        }
        
        console.log(''); // Empty line between files
        
        // Add delay to avoid rate limiting
        if (filesToReview.indexOf(file) < filesToReview.length - 1) {
            console.log('⏳ Waiting 2 seconds before next review...\n');
            await new Promise(resolve => setTimeout(resolve, 2000));
        }
    }
    
    // Summary
    console.log('\n📊 Review Summary');
    console.log('=================');
    console.log(`Total files: ${results.length}`);
    console.log(`Successful: ${results.filter(r => r.success).length}`);
    console.log(`Failed: ${results.filter(r => !r.success).length}`);
    
    if (results.some(r => r.success)) {
        console.log('\n📁 Review files saved in: code-reviews/');
    }
}

// Run the script
main().catch(error => {
    console.error('❌ Fatal error:', error);
    process.exit(1);
});