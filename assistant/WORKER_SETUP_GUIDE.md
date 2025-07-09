# Worker Setup Guide

## Important: Context Isolation

To ensure proper context isolation between the Orchestrator and worker instances, workers must follow these setup instructions carefully.

## Why This Matters

Claude Code searches up the directory tree for CLAUDE.md files. If you start Claude Code from the wrong directory, you'll see the Orchestrator's instructions instead of your own worker context!

## Correct Worker Setup

### 1. Navigate to Your Repository Directory
```bash
# From anywhere in the system
cd [path-to-orchestrator]/project_workspace/worker_X/[your-project-repo]/

# Example for Worker 1:
cd ~/projects/my-orchestrator/project_workspace/worker_1/my-project/
```

### 2. Start Claude Code FROM THIS DIRECTORY
```bash
# Make sure you're in the project repository directory!
pwd  # Should show: .../worker_X/[your-project-repo]/
claude  # or however you launch Claude Code
```

### 3. Verify Correct Context
When Claude Code starts, it should:
- NOT see the Orchestrator's CLAUDE.md
- See your local project files
- Have access to your branch's files
- Be in the correct git repository context

## File Locations for Workers

From your perspective as a worker:
- **Your Root**: `[your-project-repo]/` (where you start Claude Code)
- **Context**: `.claude_context/` (in your root)
- **Assignment**: `ASSIGNMENT.md` (in your root, if provided)
- **Onboarding**: `ONBOARDING.md` (in your root, if provided)

## Common Mistakes to Avoid

❌ **DON'T** start Claude Code from:
- The orchestrator root directory
- The worker_X directory
- Any parent directory

✅ **DO** start Claude Code from:
- Your project repository directory specifically

## Quick Verification

After starting Claude Code, run:
```bash
pwd  # Should show your project repo as current directory
ls   # Should show project files, not orchestrator files
git branch  # Should show your assigned feature branch
```

## If You See Wrong Context

If you see Orchestrator instructions or the wrong CLAUDE.md:
1. Exit Claude Code
2. Navigate to the correct directory
3. Restart Claude Code from your project repository

Remember: The directory you start Claude Code from matters! It determines which context and instructions you'll see.

---

*Proper setup ensures clean context isolation and effective parallel development.*