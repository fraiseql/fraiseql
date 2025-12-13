# Integration Tests Cleanup - Phase Plan Overview

## Objective
Make integration tests evergreen by removing all architectural hints about the software building process while maintaining test quality and coverage.

## Problem Statement
The integration test suite contains numerous files and content that reveal the iterative development process:
- Files with suffixes like `_fix`, `_regression`, `_simple`, `_extended`, `_complex`
- Duplicate test files created during different development iterations
- Comments referencing work packages (WP-XXX), phases, and TDD cycles
- Incomplete placeholder tests
- Class/function names that describe the development process rather than the feature being tested

This makes the test suite look unprofessional and creates maintenance burden.

## Success Criteria
After completion, the integration test suite should:
- Have no file names containing process hints (`_fix`, `_regression`, etc.)
- Have no duplicate test files
- Have no content containing development markers (WP-, Phase, RED/GREEN, etc.)
- Have clear, domain-focused test descriptions
- Maintain 100% test coverage (no tests lost)
- Pass all tests without errors

## Phase Breakdown

### Phase 1: Audit and Inventory (GREENFIELD)
**File**: `phase-1-audit.md`
**Duration**: ~30 minutes
**Purpose**: Create a complete inventory of all files needing cleanup
**Deliverable**: JSON inventory file with categorized issues

### Phase 2: Consolidate Duplicate Test Files (REFACTOR)
**File**: `phase-2-consolidate.md`
**Duration**: ~2 hours
**Purpose**: Merge duplicate test files into single comprehensive files
**Deliverable**: Consolidated test files with merged content

### Phase 3: Rename Files (REFACTOR)
**File**: `phase-3-rename.md`
**Duration**: ~1 hour
**Purpose**: Rename files to remove process hints
**Deliverable**: Clean file names throughout test suite

### Phase 4: Clean Content (REFACTOR)
**File**: `phase-4-clean-content.md`
**Duration**: ~3 hours
**Purpose**: Remove development markers from test content
**Deliverable**: Evergreen test content

### Phase 5: Verification and QA (QA)
**File**: `phase-5-verify.md`
**Duration**: ~30 minutes
**Purpose**: Ensure all tests pass and criteria are met
**Deliverable**: Clean test suite ready for commit

## Execution Instructions

1. **Read each phase file in order** (phase-1 through phase-5)
2. **Complete all steps** in a phase before moving to the next
3. **Run verification commands** after each phase
4. **Commit after each phase** with the specified commit message
5. **If tests fail**, fix immediately before proceeding

## Tools Needed
- Python 3.10+
- `uv` package manager
- `git` for commits
- Text editor or IDE
- `pytest` for running tests

## Estimated Total Time
~7 hours (can be split across multiple sessions)

## Important Notes for Junior Engineers

### What This Cleanup Does
- **Removes**: Historical development artifacts
- **Keeps**: All test functionality and coverage
- **Improves**: Code professionalism and maintainability

### What to Watch Out For
1. **Don't lose test coverage** - when consolidating, merge ALL tests
2. **Don't break imports** - when renaming, check for references
3. **Don't skip verification** - run tests after each phase
4. **Don't batch commits** - commit after each phase completes

### When to Ask for Help
- If consolidating tests and unsure which assertions to keep
- If a test file rename breaks imports you can't find
- If tests fail after a phase and you can't identify why
- If you find additional categories of issues not covered in the plan

### Commit Message Format
Each phase specifies the commit message to use:
```bash
refactor(tests): description [REFACTOR]
chore(tests): description [GREENFIELD]
test(tests): description [QA]
```

## Dependencies Between Phases
- **Phase 2** must complete before **Phase 3** (can't rename files that are being merged)
- **Phase 3** must complete before **Phase 4** (content cleanup references final file names)
- **Phase 5** can only run after all other phases complete

## Rollback Plan
If something goes wrong:
```bash
# Check current phase commit
git log -1

# Rollback to previous phase
git reset --hard HEAD~1

# Or rollback entire cleanup
git reset --hard <commit-before-phase-1>
```

Each phase is a separate commit, so you can roll back to any phase.
