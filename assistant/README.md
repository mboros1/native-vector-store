# Assistant Directory

This directory contains instructions and configurations specifically for Claude Code instances (both Orchestrator and Workers). These files typically don't need modification by users.

## Contents

### Communication Modes
- `absolute_mode.txt` - Instructions for writing stripped-down documentation
- `sharp_mode.txt` - Instructions for conversational interactions with critical thinking

### Setup Guides
- `WORKER_SETUP_GUIDE.md` - Critical instructions for proper Claude Code context isolation

## Purpose

These files ensure consistent behavior across all Claude Code instances in your orchestration setup:

1. **Communication Modes** - Define how Claude Code should interact and write documentation
2. **Setup Guides** - Ensure workers start Claude Code from the correct directory to maintain context isolation

## For Users

You typically don't need to modify these files. They're referenced by:
- The Orchestrator's CLAUDE.md
- Worker onboarding templates
- Assignment templates

If you need different communication styles or setup procedures, you can:
1. Create new mode files in this directory
2. Update the references in your templates
3. Or override these instructions in your project-specific CLAUDE.md

## For Claude Code Instances

When you're initialized as an Orchestrator or Worker, check this directory for:
- Communication mode definitions
- Setup verification procedures
- Any project-specific assistant configurations