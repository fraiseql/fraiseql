================================================================================
                    DOCUMENTATION REVIEW INSTRUCTIONS
================================================================================

You now have everything you need to conduct a comprehensive documentation review
of the FraiseQL project with AI guidance.

FILES CREATED FOR YOU:
  1. /tmp/documentation_review_prompt.md
     → The main prompt to give to an AI agent
     → Contains detailed instructions on how to guide you through the review

  2. /tmp/documentation_files_to_review.md
     → List of all documentation files to review, organized by priority
     → Shows which files are most important

  3. /tmp/README_DOCUMENTATION_REVIEW.txt
     → This file

================================================================================
HOW TO USE THIS SYSTEM:
================================================================================

STEP 1: Read the Prompt
  → Open /tmp/documentation_review_prompt.md
  → Understand the workflow and feedback mechanism

STEP 2: Read the File List
  → Open /tmp/documentation_files_to_review.md
  → Choose your review depth (Priority 1-4)

STEP 3: Start the Review
  → Share the prompt with an AI agent (Claude, etc.)
  → Also share the file list
  → Tell the AI: "Use the documentation_review_prompt.md to guide me through
                  reviewing the documentation files listed in
                  documentation_files_to_review.md. Start with Priority 1 files."

STEP 4: Follow the AI's Guidance
  → Read each file the AI presents
  → Provide feedback using the format shown in the prompt:
    - "OK" → Move to next file
    - "APPROVE" → Same as OK
    - Corrections → "Line X: change foo to bar"
    - "SKIP" → Skip to next file
    - Questions → "QUESTION: What does XYZ mean?"

STEP 5: Review Continues
  → The AI will:
    - Show files one at a time
    - Accept your corrections
    - Update files when approved
    - Track progress
    - Provide completion summary

================================================================================
FEEDBACK EXAMPLES:
================================================================================

✅ SIMPLE APPROVAL:
  "OK"
  "APPROVE"
  "Looks good!"

✅ CORRECTIONS:
  "Line 42: Change 'GraphQL' to 'GraphQL v2'"
  "Section 'Installation': Update all commands to use 'cargo build --release'"
  "Add a note that rate limiting is now enabled by default"

✅ MULTIPLE CORRECTIONS:
  "Line 12: Fix typo 'recieve' → 'receive'
   Line 45: Update version to v2.1
   Section 'Configuration': Add example for FRAISEQL_RATE_LIMITING_ENABLED"

✅ CONTEXT NEEDED:
  "CONTEXT: This is outdated. The API changed in v2.1. Please review knowing
           that we now support rate limiting, OIDC auth, and design audits."

✅ QUESTIONS:
  "QUESTION: Should the rate limiting section mention Redis backends?"
  "QUESTION: Is this still relevant for the current version?"

✅ SKIP:
  "SKIP"
  "Skip this one for now"

✅ EXIT:
  "STOP"
  "DONE"
  "EXIT REVIEW"

================================================================================
RECOMMENDED REVIEW DEPTH:
================================================================================

MINIMUM (30-45 minutes):
  → Priority 1 only (4 core files)
  → Covers: Main vision, development guidelines, architecture, security

STANDARD (1-2 hours):
  → Priority 1 + Priority 2 (9 files total)
  → Covers: Core + operations, development, testing, contributing

COMPREHENSIVE (2-3+ hours):
  → Priority 1 + Priority 2 + Priority 3 (11 files)
  → Covers: Everything above + release notes and design vision

FULL REVIEW (3-4+ hours):
  → All Priority 1-4 files (15 files)
  → Complete documentation audit

You control the pace - take breaks, review at your own speed.

================================================================================
FILES THAT WERE RECENTLY UPDATED:
================================================================================

As of this session:
  ✓ docs/SECURITY_MIGRATION_v2.1.md
    → Added comprehensive rate limiting section
    → Should be thoroughly reviewed

These files received updates during the rate limiting implementation commit,
so pay special attention to them!

================================================================================
QUICK START:
================================================================================

Ready to begin? Here's what to do:

1. Copy the prompt:
   cat /tmp/documentation_review_prompt.md

2. Give it to an AI agent along with this command:
   "Please use the documentation review guide to walk me through reviewing
    the FraiseQL documentation. Start with Priority 1 files, and use this
    file list: /tmp/documentation_files_to_review.md"

3. The AI will present the first file. Read it carefully.

4. Provide feedback in one of the formats above.

5. The AI will either move to the next file or show corrections.

6. Continue until satisfied or you exit.

================================================================================
NOTES:
================================================================================

• All files are checked into the repository
• Approved corrections will be committed to git automatically
• You can always pause and resume the review
• Each session saves progress independently
• The AI will maintain a running summary

Questions? You can:
  - Ask the AI guiding you anything
  - Request clarification on any section
  - Ask for context or background information
  - Ask the AI to slow down or explain something

Happy reviewing!

================================================================================
