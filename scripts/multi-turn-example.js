#!/usr/bin/env node

/**
 * Multi-turn Conversation Example with GPT-5
 * 
 * Demonstrates how to use GPT-5's Responses API for multi-turn conversations
 * with chain-of-thought (CoT) preservation for code review scenarios.
 */

import OpenAI from 'openai';
import dotenv from 'dotenv';
import { promises as fs } from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

dotenv.config({ path: path.join(__dirname, '..', '.env') });

const openai = new OpenAI({
    apiKey: process.env.OPEN_API_KEY || process.env.OPENAI_API_KEY,
});

/**
 * Multi-turn Code Review Session
 * 
 * This demonstrates how GPT-5 maintains context across multiple turns
 * using the previous_response_id to pass chain-of-thought between calls.
 */
class CodeReviewSession {
    constructor(filename, code) {
        this.filename = filename;
        this.code = code;
        this.responseHistory = [];
        this.lastResponseId = null;
    }

    /**
     * Initial code review
     */
    async initialReview() {
        console.log('📋 Starting initial review...\n');
        
        const response = await openai.responses.create({
            model: 'gpt-5',
            input: `Review this C++ code for correctness and performance issues:
            
File: ${this.filename}
\`\`\`cpp
${this.code}
\`\`\`

Identify the top 3 most critical issues.`,
            reasoning: { effort: 'medium' },
            text: { verbosity: 'medium' }
        });

        this.lastResponseId = response.id;
        this.responseHistory.push({
            turn: 1,
            type: 'initial_review',
            response: response.output_text,
            responseId: response.id,
            reasoningTokens: response.usage?.reasoning_tokens
        });

        console.log('Initial Review:\n', response.output_text);
        console.log('\n---\n');
        
        return response.output_text;
    }

    /**
     * Follow-up question about a specific issue
     */
    async askFollowUp(question) {
        console.log(`💬 Follow-up question: ${question}\n`);
        
        const response = await openai.responses.create({
            model: 'gpt-5',
            input: question,
            previous_response_id: this.lastResponseId,  // Pass CoT from previous turn
            reasoning: { effort: 'medium' },
            text: { verbosity: 'medium' }
        });

        this.lastResponseId = response.id;
        this.responseHistory.push({
            turn: this.responseHistory.length + 1,
            type: 'follow_up',
            question: question,
            response: response.output_text,
            responseId: response.id,
            reasoningTokens: response.usage?.reasoning_tokens
        });

        console.log('Response:\n', response.output_text);
        console.log('\n---\n');
        
        return response.output_text;
    }

    /**
     * Request specific refactoring
     */
    async requestRefactoring(section) {
        console.log(`🔧 Requesting refactoring for: ${section}\n`);
        
        const response = await openai.responses.create({
            model: 'gpt-5',
            input: `Based on the issues identified, please provide a refactored version of ${section}. 
                    Include the complete refactored code with inline comments explaining the changes.`,
            previous_response_id: this.lastResponseId,
            reasoning: { effort: 'high' },  // Use high effort for code generation
            text: { verbosity: 'high' }     // High verbosity for detailed code
        });

        this.lastResponseId = response.id;
        this.responseHistory.push({
            turn: this.responseHistory.length + 1,
            type: 'refactoring',
            section: section,
            response: response.output_text,
            responseId: response.id,
            reasoningTokens: response.usage?.reasoning_tokens
        });

        console.log('Refactored Code:\n', response.output_text);
        console.log('\n---\n');
        
        return response.output_text;
    }

    /**
     * Validate a proposed fix
     */
    async validateFix(proposedFix) {
        console.log('✅ Validating proposed fix...\n');
        
        const response = await openai.responses.create({
            model: 'gpt-5',
            input: `Please validate this proposed fix:
            
\`\`\`cpp
${proposedFix}
\`\`\`

Check for:
1. Does it address the original issue?
2. Are there any new issues introduced?
3. Is it following best practices?`,
            previous_response_id: this.lastResponseId,
            reasoning: { effort: 'high' },
            text: { verbosity: 'medium' }
        });

        this.lastResponseId = response.id;
        this.responseHistory.push({
            turn: this.responseHistory.length + 1,
            type: 'validation',
            proposedFix: proposedFix,
            response: response.output_text,
            responseId: response.id,
            reasoningTokens: response.usage?.reasoning_tokens
        });

        console.log('Validation Result:\n', response.output_text);
        console.log('\n---\n');
        
        return response.output_text;
    }

    /**
     * Generate summary of the entire review session
     */
    async generateSummary() {
        console.log('📊 Generating session summary...\n');
        
        const response = await openai.responses.create({
            model: 'gpt-5',
            input: `Summarize this code review session:
                    - Key issues identified
                    - Solutions provided
                    - Refactorings suggested
                    - Overall code quality assessment
                    
                    Provide actionable next steps for the developer.`,
            previous_response_id: this.lastResponseId,
            reasoning: { effort: 'medium' },
            text: { verbosity: 'medium' }
        });

        this.responseHistory.push({
            turn: this.responseHistory.length + 1,
            type: 'summary',
            response: response.output_text,
            responseId: response.id,
            reasoningTokens: response.usage?.reasoning_tokens
        });

        console.log('Session Summary:\n', response.output_text);
        
        return response.output_text;
    }

    /**
     * Save the session to a markdown file
     */
    async saveSession() {
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
        const outputFile = path.join(__dirname, '..', 'code-reviews', `session_${timestamp}.md`);
        
        let markdown = `# Multi-turn Code Review Session

**File**: ${this.filename}
**Date**: ${new Date().toISOString()}
**Model**: GPT-5
**Total Turns**: ${this.responseHistory.length}
**Total Reasoning Tokens**: ${this.responseHistory.reduce((sum, h) => sum + (h.reasoningTokens || 0), 0)}

---

## Code Reviewed

\`\`\`cpp
${this.code}
\`\`\`

---

## Conversation History

`;

        for (const turn of this.responseHistory) {
            markdown += `### Turn ${turn.turn}: ${turn.type}\n\n`;
            
            if (turn.question) {
                markdown += `**Question**: ${turn.question}\n\n`;
            }
            if (turn.section) {
                markdown += `**Section**: ${turn.section}\n\n`;
            }
            
            markdown += `**Response**:\n${turn.response}\n\n`;
            markdown += `*Response ID: ${turn.responseId}*\n`;
            markdown += `*Reasoning Tokens: ${turn.reasoningTokens || 'N/A'}*\n\n---\n\n`;
        }

        await fs.mkdir(path.dirname(outputFile), { recursive: true });
        await fs.writeFile(outputFile, markdown, 'utf-8');
        
        console.log(`\n💾 Session saved to: ${outputFile}`);
        
        return outputFile;
    }

    /**
     * Get session statistics
     */
    getStats() {
        return {
            totalTurns: this.responseHistory.length,
            totalReasoningTokens: this.responseHistory.reduce((sum, h) => sum + (h.reasoningTokens || 0), 0),
            turnTypes: this.responseHistory.map(h => h.type)
        };
    }
}

/**
 * Example multi-turn session
 */
async function runExampleSession() {
    // Example C++ code with intentional issues for demonstration
    const exampleCode = `
#include <vector>
#include <iostream>

class DataProcessor {
private:
    std::vector<int>* data;
    
public:
    DataProcessor() {
        data = new std::vector<int>();
    }
    
    void processData(int* input, int size) {
        for (int i = 0; i <= size; i++) {  // Bug: should be i < size
            data->push_back(input[i]);
        }
    }
    
    int* getData() {
        return &(*data)[0];  // Dangerous: returns raw pointer to vector data
    }
    
    void clearData() {
        delete data;
        data = nullptr;
    }
};`;

    console.log('🚀 Starting Multi-turn Code Review Session\n');
    console.log('=' .repeat(50) + '\n');
    
    // Create a new session
    const session = new CodeReviewSession('data_processor.cpp', exampleCode);
    
    try {
        // Turn 1: Initial review
        await session.initialReview();
        
        // Turn 2: Ask about memory management
        await session.askFollowUp(
            "Can you elaborate on the memory management issues? What's the risk with the current implementation?"
        );
        
        // Turn 3: Request refactoring
        await session.requestRefactoring("the DataProcessor class to use RAII and modern C++ practices");
        
        // Turn 4: Ask about performance
        await session.askFollowUp(
            "What about performance optimizations? Are there any ways to improve the data processing?"
        );
        
        // Turn 5: Validate a fix for the loop bug
        const proposedFix = `
void processData(int* input, int size) {
    if (input == nullptr || size <= 0) return;
    data->reserve(data->size() + size);  // Pre-allocate
    for (int i = 0; i < size; i++) {
        data->push_back(input[i]);
    }
}`;
        
        await session.validateFix(proposedFix);
        
        // Turn 6: Generate final summary
        await session.generateSummary();
        
        // Save the session
        const outputFile = await session.saveSession();
        
        // Print statistics
        const stats = session.getStats();
        console.log('\n📈 Session Statistics:');
        console.log(`   Total Turns: ${stats.totalTurns}`);
        console.log(`   Total Reasoning Tokens: ${stats.totalReasoningTokens.toLocaleString()}`);
        console.log(`   Turn Types: ${stats.turnTypes.join(', ')}`);
        console.log(`   Output File: ${outputFile}`);
        
    } catch (error) {
        console.error('❌ Error during session:', error.message);
    }
}

/**
 * Interactive mode for custom code review
 */
async function runInteractiveMode(filePath) {
    const code = await fs.readFile(filePath, 'utf-8');
    const filename = path.basename(filePath);
    
    console.log(`📂 Loaded file: ${filename}`);
    console.log(`📏 Lines of code: ${code.split('\n').length}\n`);
    
    const session = new CodeReviewSession(filename, code);
    
    // Initial review
    await session.initialReview();
    
    // You could add readline interface here for interactive Q&A
    // For now, just doing a few automated follow-ups
    
    await session.askFollowUp("What are the most critical security concerns in this code?");
    await session.askFollowUp("How can we improve the error handling?");
    await session.generateSummary();
    
    const outputFile = await session.saveSession();
    console.log(`\n✅ Review complete! Saved to: ${outputFile}`);
}

// Main execution
async function main() {
    const args = process.argv.slice(2);
    
    if (args.length === 0 || args[0] === '--example') {
        // Run example session
        await runExampleSession();
    } else {
        // Run interactive mode with provided file
        const filePath = path.resolve(args[0]);
        await runInteractiveMode(filePath);
    }
}

// Run the script
main().catch(error => {
    console.error('❌ Fatal error:', error);
    process.exit(1);
});