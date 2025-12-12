# Phase 3: Remove Old Test Files

## Objective

Remove old test files and update mod.rs to only import new test modules. Verify all tests still pass with only new structure active.

## Duration

30 minutes

## Prerequisites

- Phase 2 completed successfully
- All tests pass with old + new files active
- Verification confirms no tests lost
- Working directory: `/home/lionel/code/fraiseql/fraiseql_rs`

---

## ⚠️ Warning

**This phase deletes files. Ensure Phase 2 verification is complete before proceeding.**

**Rollback available**: Old files are in git history and can be recovered.

---

## Step 1: Create Safety Checkpoint

Before deleting anything, create a commit with the current state.

```bash
# Commit current state (old + new files together)
git add -A
git commit -m "test: Add new test structure (before removing old files)

- Created parsing.rs (~470 lines)
- Created response_building.rs (~900 lines)
- Renamed status_tests.rs → classification.rs
- Renamed integration_tests.rs → integration.rs
- Renamed property_tests.rs → properties.rs
- Old files still present for verification (Phase 2)

Phase 2 verification: All tests pass, no tests lost."

# Tag this commit for easy recovery
git tag test-reorg-phase2-complete
```

**Verification**:
```bash
git log -1 --oneline
git tag | grep test-reorg
```

---

## Step 2: Update mod.rs to Remove Old Imports

Comment out old file imports first (safer than deleting immediately).

**File**: `src/mutation/tests/mod.rs`

```rust
//! Tests for mutation module
//!
//! Tests are organized by data pipeline stage for clarity and maintainability.
//!
//! ## Test Organization
//!
//! - `parsing.rs` - Stage 1: PostgreSQL JSON → MutationResult
//! - `classification.rs` - Stage 2: Status taxonomy & routing
//! - `response_building.rs` - Stage 3: MutationResult → GraphQL JSON
//! - `integration.rs` - Stage 4: End-to-end scenarios
//! - `properties.rs` - Property-based tests (invariants)
//!
//! ## Migration History
//!
//! This test structure was reorganized in December 2025 from a fragmented,
//! feature-based structure to a pipeline-based structure. Old test files
//! were consolidated into the new structure for better maintainability.

use super::*;
use serde_json::{json, Value};

// ============================================================================
// Test Modules - Pipeline-Based Organization
// ============================================================================

mod parsing;                    // Stage 1: Parsing
mod classification;             // Stage 2: Classification
mod response_building;          // Stage 3: Response Building
mod integration;                // Stage 4: Integration
mod properties;                 // Property-based tests

// ============================================================================
// OLD Test Modules (REMOVED in Phase 3)
// ============================================================================

// Removed during test reorganization (2025-12-11):
// - format_tests (split into parsing.rs and response_building.rs)
// - auto_populate_fields_tests (merged into response_building.rs)
// - error_array_generation (merged into response_building.rs)
// - validation_tests (merged into response_building.rs)
// - composite_tests (merged into parsing.rs)
// - edge_case_tests (distributed to appropriate files)
// - status_tests (renamed to classification.rs)
// - integration_tests (renamed to integration.rs)
// - property_tests (renamed to properties.rs)

// Phase 3 Step 2: Comment out old imports (before deleting files)
// mod format_tests;               // REMOVED: Split into parsing + response_building
// mod auto_populate_fields_tests; // REMOVED: Merged into response_building
// mod error_array_generation;     // REMOVED: Merged into response_building
// mod validation_tests;           // REMOVED: Merged into response_building
// mod composite_tests;            // REMOVED: Merged into parsing
// mod edge_case_tests;            // REMOVED: Distributed to other files
```

---

## Step 3: Test with Old Imports Commented Out

Verify tests still pass with only new modules active.

```bash
# Rebuild to apply mod.rs changes
cargo clean --package fraiseql_rs
cargo build --lib 2>&1 | tee /tmp/phase-3-build-1.log

# Run tests
cargo test --lib mutation 2>&1 | tee /tmp/phase-3-test-1.log

# Check test result
grep "test result:" /tmp/phase-3-test-1.log
```

**Expected Output**:
```
test result: ok. XX passed; 0 failed; 0 ignored
```

**Test count should be ~50% of Phase 2** (since old files are now inactive).

**Verify**:
```bash
# Compare to baseline from Phase 0
PHASE_0_COUNT=$(grep "test result:" /tmp/test-baseline.log | grep -oP '\d+ passed' | head -1 | cut -d' ' -f1)
PHASE_3_COUNT=$(grep "test result:" /tmp/phase-3-test-1.log | grep -oP '\d+ passed' | head -1 | cut -d' ' -f1)

echo "Phase 0 baseline: $PHASE_0_COUNT tests"
echo "Phase 3 (new only): $PHASE_3_COUNT tests"

# They should match (within 1-2 tests for renames)
if [ "$PHASE_0_COUNT" -eq "$PHASE_3_COUNT" ]; then
    echo "✅ Test count matches baseline"
else
    echo "⚠️  Test count differs: expected $PHASE_0_COUNT, got $PHASE_3_COUNT"
fi
```

---

## Step 4: Delete Old Test Files

If Step 3 tests pass, delete old test files.

```bash
cd src/mutation/tests

# Delete old test files
git rm format_tests.rs
git rm auto_populate_fields_tests.rs
git rm error_array_generation.rs
git rm validation_tests.rs
git rm composite_tests.rs
git rm edge_case_tests.rs

# Note: status_tests.rs, integration_tests.rs, property_tests.rs were renamed
# in Phase 1, so they should already be gone if git mv was used.
# If they still exist, remove them:
# git rm status_tests.rs integration_tests.rs property_tests.rs
```

**Verification**:
```bash
# Check files are staged for deletion
git status | grep "deleted:"

# Expected:
#   deleted: format_tests.rs
#   deleted: auto_populate_fields_tests.rs
#   deleted: error_array_generation.rs
#   deleted: validation_tests.rs
#   deleted: composite_tests.rs
#   deleted: edge_case_tests.rs
```

---

## Step 5: Remove Commented Imports from mod.rs

Clean up mod.rs to remove the commented-out imports.

**File**: `src/mutation/tests/mod.rs`

**Remove this section**:
```rust
// Phase 3 Step 2: Comment out old imports (before deleting files)
// mod format_tests;               // REMOVED: Split into parsing + response_building
// mod auto_populate_fields_tests; // REMOVED: Merged into response_building
// mod error_array_generation;     // REMOVED: Merged into response_building
// mod validation_tests;           // REMOVED: Merged into response_building
// mod composite_tests;            // REMOVED: Merged into parsing
// mod edge_case_tests;            // REMOVED: Distributed to other files
```

**Final mod.rs** should look like:
```rust
//! Tests for mutation module
//!
//! Tests are organized by data pipeline stage for clarity and maintainability.
//!
//! ## Test Organization
//!
//! - `parsing.rs` - Stage 1: PostgreSQL JSON → MutationResult
//! - `classification.rs` - Stage 2: Status taxonomy & routing
//! - `response_building.rs` - Stage 3: MutationResult → GraphQL JSON
//! - `integration.rs` - Stage 4: End-to-end scenarios
//! - `properties.rs` - Property-based tests (invariants)
//!
//! ## Migration History
//!
//! This test structure was reorganized in December 2025 from a fragmented,
//! feature-based structure to a pipeline-based structure. Old test files
//! were consolidated into the new structure for better maintainability.

use super::*;
use serde_json::{json, Value};

// ============================================================================
// Test Modules - Pipeline-Based Organization
// ============================================================================

mod parsing;                    // Stage 1: Parsing
mod classification;             // Stage 2: Classification
mod response_building;          // Stage 3: Response Building
mod integration;                // Stage 4: Integration
mod properties;                 // Property-based tests
```

---

## Step 6: Final Test Run

Run tests one more time to ensure everything still works.

```bash
# Clean build
cargo clean --package fraiseql_rs
cargo build --lib 2>&1 | tee /tmp/phase-3-build-final.log

# Run tests
cargo test --lib mutation 2>&1 | tee /tmp/phase-3-test-final.log

# Check result
grep "test result:" /tmp/phase-3-test-final.log
```

**Verification**:
- All tests pass (0 failures)
- Test count matches Phase 0 baseline
- No warnings about missing modules

---

## Step 7: Verify File Deletion in Filesystem

```bash
cd src/mutation/tests

# List remaining test files
ls -lh *.rs

# Should see only:
# - mod.rs
# - parsing.rs
# - classification.rs
# - response_building.rs
# - integration.rs
# - properties.rs
```

**Expected**: 6 files total (mod.rs + 5 test files)

---

## Step 8: Create Phase 3 Completion Report

**File**: `/tmp/phase-3-completion-report.txt`

```
Phase 3 Completion Report
==========================

Date: [DATE]
Status: [PASS/FAIL]

Files Deleted:
--------------
✅ format_tests.rs
✅ auto_populate_fields_tests.rs
✅ error_array_generation.rs
✅ validation_tests.rs
✅ composite_tests.rs
✅ edge_case_tests.rs

Files Renamed (Phase 1):
------------------------
✅ status_tests.rs → classification.rs
✅ integration_tests.rs → integration.rs
✅ property_tests.rs → properties.rs

Remaining Test Files:
---------------------
✅ mod.rs
✅ parsing.rs
✅ classification.rs
✅ response_building.rs
✅ integration.rs
✅ properties.rs

Test Execution Results:
-----------------------
Total tests run: XXX
Tests passed: XXX
Tests failed: 0
Tests ignored: 0

Verification:
-------------
[✅] All old files deleted
[✅] mod.rs cleaned up (no commented imports)
[✅] All tests pass
[✅] Test count matches baseline
[✅] No compilation warnings
[✅] Git status clean (files staged for commit)

Ready for Phase 4: [YES/NO]

Next Steps:
-----------
Proceed to Phase 4 (Documentation & Cleanup)
```

---

## Step 9: Commit Changes

Create a commit with the old files removed.

```bash
# Check what's staged
git status

# Commit deletion
git add -A
git commit -m "test: Remove old test files after reorganization

Deleted old test files (consolidated into new structure):
- format_tests.rs → split into parsing.rs + response_building.rs
- auto_populate_fields_tests.rs → merged into response_building.rs
- error_array_generation.rs → merged into response_building.rs
- validation_tests.rs → merged into response_building.rs
- composite_tests.rs → merged into parsing.rs
- edge_case_tests.rs → distributed to appropriate files

Test structure now follows data pipeline stages:
- parsing.rs (~470 lines)
- classification.rs (~133 lines)
- response_building.rs (~900 lines)
- integration.rs (~442 lines)
- properties.rs (~92 lines)

All tests pass. Test count matches baseline."

# Tag this commit
git tag test-reorg-phase3-complete
```

---

## Verification Checklist

After Phase 3:
- [ ] Old test files deleted from filesystem
- [ ] Old test files deleted from git (staged for commit)
- [ ] mod.rs no longer imports old files
- [ ] mod.rs has no commented-out imports
- [ ] All tests pass (0 failures)
- [ ] Test count matches Phase 0 baseline
- [ ] Commit created with file deletions
- [ ] Safety tags created (phase2-complete, phase3-complete)
- [ ] Phase 3 completion report created
- [ ] Ready for Phase 4

---

## Rollback Procedures

### If Tests Fail After Deletion

```bash
# Option 1: Restore from git
git checkout HEAD~1 src/mutation/tests/

# Option 2: Restore from phase2 tag
git checkout test-reorg-phase2-complete -- src/mutation/tests/

# Re-run Phase 2 to identify issues
```

### If Wrong Files Deleted

```bash
# Restore all test files
git checkout HEAD~1 src/mutation/tests/

# Review Phase 1 test migration map
# Identify correct files to delete
# Re-run Phase 3
```

---

## Common Issues and Solutions

### Issue: "Cannot find module"

**Symptom**: Compilation error about missing module
**Cause**: mod.rs still tries to import deleted file
**Solution**: Check mod.rs, ensure old imports removed

### Issue: "Test count lower than baseline"

**Symptom**: Fewer tests run than in Phase 0
**Cause**: Some tests not copied to new files
**Solution**: Restore old files, review Phase 1 migration, re-copy missing tests

### Issue: "Tests failing that passed before"

**Symptom**: Tests fail after old files deleted
**Cause**: Missing dependency or import that was in old file
**Solution**: Review failing tests, add missing imports or helpers

---

## Time Estimate

- Step 1 (Safety checkpoint): 3 minutes
- Step 2 (Comment imports): 5 minutes
- Step 3 (Test with comments): 10 minutes
- Step 4 (Delete files): 2 minutes
- Step 5 (Clean mod.rs): 2 minutes
- Step 6 (Final test): 5 minutes
- Step 7 (Verify deletion): 1 minute
- Step 8 (Report): 5 minutes
- Step 9 (Commit): 3 minutes

**Total**: ~35 minutes

---

## Deliverables

After Phase 3:
1. ✅ Old test files deleted
2. ✅ mod.rs cleaned up and final
3. ✅ All tests pass with new structure only
4. ✅ Test count verified against baseline
5. ✅ Commits created with appropriate messages
6. ✅ Safety tags created for rollback
7. ✅ Phase 3 completion report created
8. ✅ Ready for Phase 4 (final cleanup)

---

## Next Phase

Proceed to:
- **Phase 4**: Documentation & Cleanup

**Prerequisites for Phase 4**:
- [ ] All Phase 3 checks pass
- [ ] Old files successfully deleted
- [ ] All tests pass
- [ ] Git history clean

---

**Phase 3 Status**: 🗑️ Cleanup Complete → Ready for Phase 4
