# Documentation Review Guide for AI Agent

## Overview

You are guiding a human through a comprehensive review of all FraiseQL documentation. Your role is to:

1. Present one documentation file at a time
2. Show the file path clearly
3. Allow the human time to read and provide feedback
4. Accept corrections and update the file, OR move to the next file
5. Track progress and provide a summary at the end

---

## Documentation Files to Review

The following documentation files should be reviewed in order:

### Core Architecture & Design

1. `README.md` - Project overview and getting started
2. `docs/ARCHITECTURE_PRINCIPLES.md` - Core architectural documentation
3. `docs/SECURITY_MIGRATION_v2.1.md` - Security implementation guide

### Development & Implementation

4. `.claude/CLAUDE.md` - Project development guidelines
5. `.claude/IMPLEMENTATION_ROADMAP.md` - Implementation status and roadmap
6. `.phases/README.md` - Phase-based development methodology (if exists)

### Crate-Specific Documentation

7. `crates/fraiseql-core/README.md` - Core engine documentation (if exists)
8. `crates/fraiseql-server/README.md` - Server documentation (if exists)
9. `crates/fraiseql-cli/README.md` - CLI documentation (if exists)
10. `crates/fraiseql-wire/README.md` - Wire protocol documentation (if exists)

### Other Documentation

11. `CONTRIBUTING.md` - Contributing guidelines (if exists)
12. `LICENSE` - License file (if exists)
13. Any other `.md` files found in root or docs/ directory

---

## Review Workflow

### For Each Document

**STEP 1: Display the File**

```
═══════════════════════════════════════════════════════════════
📄 REVIEWING: [FILE_PATH]
═══════════════════════════════════════════════════════════════

[FULL FILE CONTENT HERE]

═══════════════════════════════════════════════════════════════
```

**STEP 2: Present Summary**

```
📋 Summary:
- File size: [X lines]
- Last section: [Name]
- Documentation style: [Type]
```

**STEP 3: Request Feedback**

```
✏️  PLEASE PROVIDE FEEDBACK:

You can respond with:
1. "OK" or "APPROVE" → Move to next file
2. Specific corrections → I'll update and show you the corrected version
3. "SKIP" → Skip to next file
4. "QUESTION: [text]" → Ask questions about the content
5. "CONTEXT: [description]" → Provide context for corrections

Example feedback:
  "Line 42: Change 'foo' to 'bar'. Line 55: Add note about xyz."
  "The examples are outdated, update to use v2 API."
  "OK - looks good"
```

**STEP 4: Handle Feedback**

- **If "OK" / "APPROVE"**: Move to next file in list
- **If corrections**: Show the updated section → Ask if satisfied → If yes, move to next file
- **If "SKIP"**: Move to next file
- **If "QUESTION"**: Answer with current knowledge → Ask if feedback is needed
- **If "CONTEXT"**: Acknowledge context → Apply to document → Show result

---

## Handling Corrections

When the human provides corrections:

1. **Show the before/after**:

   ```
   BEFORE:
   [Original text]

   AFTER:
   [Corrected text]
   ```

2. **Ask for approval**:

   ```
   ✅ Updated. Does this look correct?
   Respond: "YES" to move to next file, or provide additional feedback
   ```

3. **Apply changes**:
   - Once approved, use the Edit tool to update the actual file
   - Commit the change with: `git add [FILE] && git commit -m "docs(...): [description]"`
   - Show confirmation

4. **Stay in file if more changes needed**:
   - If human has more feedback, present the file again with changes applied
   - Continue until they say "OK" or "APPROVE"

---

## Progress Tracking

After each file, display:

```
📊 Progress: [X/Y files reviewed]
   ✅ Completed: [file1], [file2], ...
   ⏳ Current: [current_file]
   ⬜ Remaining: [file_n], [file_n+1], ...
```

---

## Completion Summary

When all files are reviewed, display:

```
🎉 DOCUMENTATION REVIEW COMPLETE!

Summary:
- Total files reviewed: [X]
- Files updated: [Y]
- Files approved without changes: [Z]
- Files skipped: [N]

Changes made:
- [File 1]: [Description of changes]
- [File 2]: [Description of changes]
...

Next steps:
1. All documentation is up to date
2. Ready to create final PR/commit
3. Recommend: git log --oneline [N] to see documentation commits
```

---

## Important Notes for AI Agent

1. **Read files fully** - Always show the complete file content before asking for feedback
2. **Be patient** - Wait for human feedback before proceeding
3. **Show context** - When displaying corrections, show before/after clearly
4. **Track changes** - Keep count of files reviewed and changes made
5. **Preserve structure** - Don't change file structure/formatting unless specifically asked
6. **Commit each change** - Update actual files with git commits when approved
7. **Handle "CONTEXT" responses** - When human provides context, acknowledge it and reread docs with that lens
8. **No assumptions** - If unclear about a correction, ask for clarification

---

## Starting the Review

**To begin, respond with:**

```
🚀 STARTING DOCUMENTATION REVIEW

I will guide you through reviewing all FraiseQL documentation files.

Total files to review: [COUNT]
Time estimate: You control the pace

Ready to begin? Please say "START" or "BEGIN" when ready, or
provide any initial context/instructions for the review.
```

---

## Exit Points

The human can exit the review at any time by saying:

- "STOP" - End review (document progress)
- "DONE" - Same as STOP
- "EXIT" - Same as STOP

When exiting, show:

```
📋 REVIEW SESSION ENDED

Files completed: [X/Y]
Changes made: [N]
Last file reviewed: [NAME]

To resume: Ask me to continue the documentation review

Files still needing review:
- [file1]
- [file2]
...
```

---

**Now, please start the documentation review by reading the instructions above and responding with the ready message.**
