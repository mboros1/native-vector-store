# Worker Onboarding

Welcome to your project workspace! This guide will help you get started with your assignment.

## Your Workspace

- **Working Directory**: THIS directory - always start Claude Code here!
- **Branch**: `feature/initial-npm-package`
- **Remote**: `https://github.com/mboros1/native-vector-store.git`

⚠️ **Critical**: See `assistant/WORKER_SETUP_GUIDE.md` (in this directory) for detailed setup instructions to ensure proper context isolation.

## Getting Started

### Step 1: Review Your Assignment
Your assignment is in the PR description at: [PR will be created after first push]

### Step 2: Set Up Your Environment
```bash
# Add project-specific build commands
```
<!-- ORCHESTRATOR:APPEND:after_setup -->

### Step 3: Customize Your CLAUDE.md and Create Private Branch
You've been provided a starter CLAUDE.md. This is your knowledge base that will grow with each assignment:

1. Edit CLAUDE.md to add your task understanding and approach
2. Create your private branch to preserve your growing expertise:
   ```bash
   # Create a private branch for your workspace
   git checkout -b private/worker_vector_store/Initial npm package implementation
   git add CLAUDE.md
   git commit -m "Initial workspace for feature/initial-npm-package"
   git push -u origin private/worker_vector_store/Initial npm package implementation
   
   # Return to feature branch to continue work
   git checkout feature/initial-npm-package
   ```
3. Add CLAUDE.md to git exclusions:
   ```bash
   echo "CLAUDE.md" >> .git/info/exclude
   ```

**CRITICAL**: 
- Never commit CLAUDE.md to the feature branch!
- Always backup to your private branch as you learn
- Your CLAUDE.md should grow richer with each assignment

### Regular Backups
As you discover patterns, solve problems, or learn about the codebase:
```bash
# Quick backup (from feature branch)
git stash push -m "temp" -- CLAUDE.md
git checkout private/worker_vector_store/Initial npm package implementation
git stash pop
git commit -am "Update: [what you learned]"
git push
git checkout feature/initial-npm-package
```

<!-- ORCHESTRATOR:APPEND:before_communication_protocol -->
## Communication Protocol

### PR Comments
All communication happens through PR comments:

**Progress Updates**:
```markdown
## Progress Update - [Date]

✅ Completed:
- [What you finished]

🔧 In Progress:
- [What you're working on]

❓ Questions:
- [Any blockers or questions]
```

**Requesting Help**:
```markdown
## Need Assistance

**Issue**: [Description]
**What I've Tried**: [Your attempts]
**Context**: [Relevant information]

Could you provide guidance?
```

**Ready for Review**:
```markdown
## Ready for Review

All tasks complete and tested. Key changes:
- [Summary of changes]
- [Files modified]

Ready to merge!
```

## Development Workflow

### Best Practices
1. Check PR for new comments regularly
2. Post progress updates when reaching milestones
3. Ask questions early - don't stay blocked
4. Commit with clear, descriptive messages
5. Save context using `/compact` regularly
<!-- ORCHESTRATOR:APPEND:after_development_workflow -->

### Before Marking Complete
- [ ] All acceptance criteria met
- [ ] Tests passing
- [ ] Code follows project conventions
- [ ] Documentation updated
- [ ] CLAUDE.md reflects current state
- [ ] Final "Ready for Review" comment posted

## Important Notes

### DO NOT Commit
- This ONBOARDING.md file
- Any ASSIGNMENT.md files
- Temporary or workflow files

### DO Commit
- Your updated CLAUDE.md
- All code changes
- Documentation updates
- Test files
- Configuration changes

## Context Management

### Saving Context
Use the `/compact` command regularly to save your working context:
```
/compact
```

This creates a snapshot in `.claude_context/` that can be restored later.

### Restoring Context
If you need to resume work after a break, your context files help you quickly get back up to speed.

## Knowledge Continuity

### Starting Your Next Assignment
When you receive a new assignment:
1. Check if you have existing private branches with relevant context
2. Copy useful patterns and knowledge from previous CLAUDE.md files
3. Build upon your growing expertise

Your private branches form your personal knowledge base for this project.

## Communication Modes

You must follow these communication modes (check your local `assistant/` directory):

### Sharp Mode
**Use for**: All conversational interactions, discussions, and problem-solving. Provide direct feedback, critical analysis, and honest assessments. Clearly signal uncertainty levels and ask clarifying questions.

### Absolute Mode  
**Use for**: All documentation writing. Write concisely without decorative language, emotional appeals, or unnecessary transitions.

**Location**: These mode definitions are in your project repository at:
- `assistant/sharp_mode.txt`
- `assistant/absolute_mode.txt`

## Need Help?

Post a comment on your PR describing your issue. The orchestrator will respond and help unblock you.

---

*Remember: Clear communication and regular updates help ensure smooth collaboration!*