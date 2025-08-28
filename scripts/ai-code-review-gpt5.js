#!/usr/bin/env node

/**
 * AI Code Review Script using GPT-5
 * 
 * This script performs comprehensive code reviews on C++ source files
 * using GPT-5's advanced reasoning capabilities via the Responses API.
 * 
 * Usage: node ai-code-review-gpt5.js [file1.cpp file2.cpp ...] or no args for all C++ files
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
const CODE_REVIEW_PROMPT = `You are an expert C++ code reviewer with deep knowledge of modern C++ best practices, performance optimization, memory management, concurrency, and security.

Please perform a comprehensive code review of the provided C++ source file, analyzing:

1. **Correctness** (Logic errors, undefined behavior, race conditions, memory safety)
2. **Performance** (Algorithm complexity, cache efficiency, SIMD opportunities, allocations)
3. **Maintainability** (Code clarity, documentation, naming conventions, modularity)
4. **Design** (Architecture, patterns, API design, extensibility)
5. **Security** (Input validation, buffer safety, integer overflows)
6. **Compliance** (C++ standard compliance, platform compatibility)

For each category:
- Provide a rating (1-10)
- List specific issues found with line numbers when possible
- Suggest concrete improvements with code examples
- Highlight what's done well

Format your response as structured markdown with clear sections.`;

/**
 * Configuration for different review depths
 */
const REVIEW_CONFIGS = {
    quick: {
        reasoning: { effort: 'minimal' },
        text: { verbosity: 'low' },
        description: 'Quick review focusing on critical issues'
    },
    standard: {
        reasoning: { effort: 'medium' },
        text: { verbosity: 'medium' },
        description: 'Standard comprehensive review'
    },
    deep: {
        reasoning: { effort: 'high' },
        text: { verbosity: 'high' },
        description: 'Deep analysis with detailed explanations'
    }
};

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
 * Perform code review using GPT-5 Responses API
 */
async function reviewCode(filename, code, config = 'standard', previousResponseId = null) {
    const startTime = Date.now();
    const reviewConfig = REVIEW_CONFIGS[config];
    
    console.log(`🔍 Reviewing ${filename} (${reviewConfig.description})...`);
    
    try {
        // Build the input for GPT-5
        const input = `${CODE_REVIEW_PROMPT}

File: ${filename}
Lines of code: ${code.split('\n').length}

\`\`\`cpp
${code}
\`\`\`

Please provide a comprehensive code review following the structure outlined above.`;

        // Create request parameters
        const requestParams = {
            model: 'gpt-5',  // Use gpt-5 for complex code analysis
            input: input,
            reasoning: reviewConfig.reasoning,
            text: reviewConfig.text
        };

        // Add previous response ID for multi-turn conversation if available
        if (previousResponseId) {
            requestParams.previous_response_id = previousResponseId;
        }

        // Call GPT-5 via Responses API
        const response = await openai.responses.create(requestParams);
        
        const elapsedTime = ((Date.now() - startTime) / 1000).toFixed(1);
        console.log(`✅ Review completed for ${filename} (${elapsedTime}s)`);
        
        // Log reasoning token usage if available
        if (response.usage) {
            console.log(`   Reasoning tokens: ${response.usage.reasoning_tokens || 'N/A'}`);
            console.log(`   Output tokens: ${response.usage.output_tokens || 'N/A'}`);
        }
        
        return {
            content: response.output_text,
            responseId: response.id,  // Save for potential follow-up
            reasoningTokens: response.usage?.reasoning_tokens || 0
        };
        
    } catch (error) {
        console.error(`❌ Error reviewing ${filename}:`, error.message);
        
        // Fallback to gpt-5-mini for faster/cheaper alternative
        if (error.status === 429 || error.status === 503) {
            console.log(`⚠️ Falling back to gpt-5-mini for ${filename}...`);
            
            try {
                const response = await openai.responses.create({
                    model: 'gpt-5-mini',
                    input: input,
                    reasoning: { effort: 'medium' },
                    text: { verbosity: 'medium' }
                });
                
                const elapsedTime = ((Date.now() - startTime) / 1000).toFixed(1);
                console.log(`✅ Review completed with gpt-5-mini for ${filename} (${elapsedTime}s)`);
                
                return {
                    content: response.output_text,
                    responseId: response.id,
                    reasoningTokens: response.usage?.reasoning_tokens || 0
                };
            } catch (fallbackError) {
                console.error(`❌ Fallback also failed for ${filename}:`, fallbackError.message);
                return null;
            }
        }
        
        return null;
    }
}

/**
 * Perform follow-up analysis on specific issues
 */
async function followUpAnalysis(filename, issue, previousResponseId) {
    console.log(`🔎 Performing follow-up analysis on ${issue}...`);
    
    try {
        const response = await openai.responses.create({
            model: 'gpt-5',
            input: `Based on the previous code review of ${filename}, please provide a deeper analysis of: ${issue}
            
Suggest specific refactoring steps or alternative implementations.`,
            previous_response_id: previousResponseId,  // Pass CoT from previous turn
            reasoning: { effort: 'high' },
            text: { verbosity: 'medium' }
        });
        
        return response.output_text;
    } catch (error) {
        console.error(`❌ Follow-up analysis failed:`, error.message);
        return null;
    }
}

/**
 * Save review to markdown file
 */
async function saveReview(filename, review, config, stats = {}) {
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
**Model**: GPT-5
**Configuration**: ${config} (${REVIEW_CONFIGS[config].description})
**File**: ${filename}
${stats.reasoningTokens ? `**Reasoning Tokens Used**: ${stats.reasoningTokens}` : ''}

---

${review}

---

*Generated by GPT-5 Code Review Script*
`;
    
    await fs.writeFile(outputFile, markdown, 'utf-8');
    console.log(`📝 Review saved to ${outputFile}`);
    
    return outputFile;
}

/**
 * Main execution
 */
async function main() {
    console.log('🚀 GPT-5 Code Review Script');
    console.log('===========================\n');
    
    // Check API key
    if (!process.env.OPEN_API_KEY && !process.env.OPENAI_API_KEY) {
        console.error('❌ Error: OpenAI API key not found in .env file');
        console.error('Please set OPEN_API_KEY or OPENAI_API_KEY in your .env file');
        process.exit(1);
    }
    
    // Parse command line arguments
    const args = process.argv.slice(2);
    let config = 'standard';
    let filesToReview = [];
    
    // Check for --config flag
    const configIndex = args.indexOf('--config');
    if (configIndex !== -1 && args[configIndex + 1]) {
        config = args[configIndex + 1];
        if (!REVIEW_CONFIGS[config]) {
            console.error(`❌ Invalid config: ${config}. Use quick, standard, or deep`);
            process.exit(1);
        }
        // Remove config args from file list
        args.splice(configIndex, 2);
    }
    
    // Check for --follow-up flag
    const followUp = args.includes('--follow-up');
    if (followUp) {
        args.splice(args.indexOf('--follow-up'), 1);
    }
    
    // Get files to review
    if (args.length > 0) {
        // Use provided files
        filesToReview = args.map(f => path.resolve(f));
    } else {
        // Get all C++ files
        filesToReview = await getCppFiles();
        console.log(`Found ${filesToReview.length} C++ files to review`);
    }
    
    console.log(`Using config: ${config} (${REVIEW_CONFIGS[config].description})\n`);
    
    // Process each file
    const results = [];
    let totalReasoningTokens = 0;
    
    for (const file of filesToReview) {
        const code = await readFile(file);
        
        if (!code) {
            console.log(`⚠️ Skipping ${file} (could not read)\n`);
            continue;
        }
        
        // Check file size (GPT-5 handles long context well but warn for very large files)
        const lines = code.split('\n').length;
        if (lines > 5000) {
            console.log(`⚠️ Warning: ${file} has ${lines} lines, review may be slower\n`);
        }
        
        const reviewResult = await reviewCode(path.basename(file), code, config);
        
        if (reviewResult) {
            totalReasoningTokens += reviewResult.reasoningTokens;
            
            // Optionally perform follow-up analysis
            let fullReview = reviewResult.content;
            
            if (followUp && reviewResult.responseId) {
                // Extract critical issues for follow-up
                const criticalMatch = reviewResult.content.match(/Critical Issue[s]?:(.*?)(?=\n##|\n\*\*|$)/s);
                if (criticalMatch) {
                    const followUpContent = await followUpAnalysis(
                        path.basename(file),
                        'the critical issues identified',
                        reviewResult.responseId
                    );
                    if (followUpContent) {
                        fullReview += '\n\n## Follow-up Analysis\n\n' + followUpContent;
                    }
                }
            }
            
            const outputFile = await saveReview(file, fullReview, config, {
                reasoningTokens: reviewResult.reasoningTokens
            });
            results.push({ file, outputFile, success: true });
        } else {
            results.push({ file, success: false });
        }
        
        console.log(''); // Empty line between files
        
        // Add delay to avoid rate limiting (GPT-5 has higher limits but still good practice)
        if (filesToReview.indexOf(file) < filesToReview.length - 1) {
            console.log('⏳ Waiting 1 second before next review...\n');
            await new Promise(resolve => setTimeout(resolve, 1000));
        }
    }
    
    // Summary
    console.log('\n📊 Review Summary');
    console.log('=================');
    console.log(`Model: GPT-5 (${config} config)`);
    console.log(`Total files: ${results.length}`);
    console.log(`Successful: ${results.filter(r => r.success).length}`);
    console.log(`Failed: ${results.filter(r => !r.success).length}`);
    if (totalReasoningTokens > 0) {
        console.log(`Total reasoning tokens: ${totalReasoningTokens.toLocaleString()}`);
    }
    
    if (results.some(r => r.success)) {
        console.log('\n📁 Review files saved in: code-reviews/');
    }
    
    // Provide usage tips
    console.log('\n💡 Tips:');
    console.log('  • Use --config quick for faster, focused reviews');
    console.log('  • Use --config deep for thorough analysis');
    console.log('  • Add --follow-up for additional analysis on critical issues');
    console.log('  • Pass specific files to review only those files');
}

// Run the script
main().catch(error => {
    console.error('❌ Fatal error:', error);
    process.exit(1);
});