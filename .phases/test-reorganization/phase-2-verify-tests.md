# Phase 2: Update Imports & Verify Tests Pass

## Objective

Update mod.rs to properly import new test modules, run all tests (old + new together), and verify no tests were lost or broken during migration.

## Duration

30 minutes

## Prerequisites

- Phase 1 completed
- New test files created and populated
- Code compiles (warnings about duplicates OK)
- Working directory: `/home/lionel/code/fraiseql/fraiseql_rs`

---

## Step 1: Update mod.rs Imports

Ensure mod.rs imports BOTH old and new test modules for verification.

**File**: `src/mutation/tests/mod.rs`

```rust
//! Tests for mutation module
//!
//! This module contains comprehensive tests for mutation parsing, classification,
//! and response building. Tests are organized by data pipeline stage.
//!
//! ## Test Organization (New Structure)
//!
//! - `parsing.rs` - Stage 1: PostgreSQL JSON → MutationResult
//! - `classification.rs` - Stage 2: Status taxonomy & routing
//! - `response_building.rs` - Stage 3: MutationResult → GraphQL JSON
//! - `integration.rs` - Stage 4: End-to-end scenarios
//! - `properties.rs` - Property-based tests (invariants)

use super::*;
use serde_json::{json, Value};

// ============================================================================
// Test Modules - NEW STRUCTURE (Phase 2: Both old and new active)
// ============================================================================

// New test modules (pipeline-based organization)
mod parsing;                    // NEW: Parsing tests
mod classification;             // RENAMED from status_tests
mod response_building;          // NEW: Response building tests
mod integration;                // RENAMED from integration_tests
mod properties;                 // RENAMED from property_tests

// Old test modules (keep for verification, will remove in Phase 3)
mod format_tests;               // OLD: Will be deleted
mod auto_populate_fields_tests; // OLD: Will be deleted
mod error_array_generation;     // OLD: Will be deleted
mod validation_tests;           // OLD: Will be deleted
mod composite_tests;            // OLD: Will be deleted
mod edge_case_tests;            // OLD: Will be deleted

// Note: status_tests, integration_tests, property_tests were renamed,
// so they should NOT appear here if git mv was used correctly.
```

**Verification**:
```bash
# Check mod.rs syntax
cargo check --lib 2>&1 | grep "src/mutation/tests/mod.rs"
```

---

## Step 2: Count Tests Before Running

Predict how many tests should run (old + new).

```bash
# Count test functions in old files
grep -c "^fn test_" src/mutation/tests/format_tests.rs > /tmp/old-tests.txt
grep -c "^fn test_" src/mutation/tests/auto_populate_fields_tests.rs >> /tmp/old-tests.txt
grep -c "^fn test_" src/mutation/tests/error_array_generation.rs >> /tmp/old-tests.txt
grep -c "^fn test_" src/mutation/tests/validation_tests.rs >> /tmp/old-tests.txt
grep -c "^fn test_" src/mutation/tests/composite_tests.rs >> /tmp/old-tests.txt
grep -c "^fn test_" src/mutation/tests/edge_case_tests.rs >> /tmp/old-tests.txt

# Count test functions in new files
grep -c "^fn test_" src/mutation/tests/parsing.rs > /tmp/new-tests.txt
grep -c "^fn test_" src/mutation/tests/classification.rs >> /tmp/new-tests.txt
grep -c "^fn test_" src/mutation/tests/response_building.rs >> /tmp/new-tests.txt
grep -c "^fn test_" src/mutation/tests/integration.rs >> /tmp/new-tests.txt
grep -c "^fn test_" src/mutation/tests/properties.rs >> /tmp/new-tests.txt

# Sum totals
awk '{sum+=$1} END {print "Old tests:", sum}' /tmp/old-tests.txt
awk '{sum+=$1} END {print "New tests:", sum}' /tmp/new-tests.txt
```

**Expected**: Old + New should be nearly 2x the baseline (from Phase 0), since tests are duplicated.

**Record**:
- Old files: _____ tests
- New files: _____ tests
- Expected cargo test: ~_____ tests (old + new, minus renamed files)

---

## Step 3: Run All Tests

Run the full test suite with both old and new modules active.

```bash
# Run all mutation tests
cargo test --lib mutation 2>&1 | tee /tmp/phase-2-test-results.log

# Extract test result summary
grep "test result:" /tmp/phase-2-test-results.log
```

**Expected Output**:
```
test result: ok. XX passed; 0 failed; 0 ignored
```

**Analysis**:
- Should show INCREASED test count (old + new together)
- 0 failures (all tests must pass)
- Some warnings about duplicate test names OK

---

## Step 4: Check for Test Name Conflicts

Look for warnings about duplicate test names.

```bash
# Check for duplicate test name warnings
grep -i "duplicate" /tmp/phase-2-test-results.log
grep -i "warning.*test" /tmp/phase-2-test-results.log
```

**Expected**: Warnings like `duplicate definitions with name \`test_xyz\``

**This is OK**: We have duplicates because both old and new files are active.

---

## Step 5: Verify No Tests Lost

Compare test counts to ensure all tests from old files are now in new files.

**Manual Verification Required**:

Create `/tmp/test-verification.txt`:

```
Test Migration Verification
===========================

format_tests.rs:
  - XX tests in source file
  - Parsing tests → parsing.rs: XX tests copied
  - Response tests → response_building.rs: XX tests copied
  - Total accounted for: XX tests
  - ✅ All tests migrated

auto_populate_fields_tests.rs:
  - 5 tests in source file
  - All → response_building.rs: 5 tests copied
  - ✅ All tests migrated

error_array_generation.rs:
  - XX tests in source file
  - All → response_building.rs: XX tests copied
  - ✅ All tests migrated

validation_tests.rs:
  - XX tests in source file
  - All → response_building.rs: XX tests copied
  - ✅ All tests migrated

composite_tests.rs:
  - XX tests in source file
  - All → parsing.rs: XX tests copied
  - ✅ All tests migrated

edge_case_tests.rs:
  - XX tests in source file
  - Parsing edge cases → parsing.rs: XX tests copied
  - Response edge cases → response_building.rs: XX tests copied
  - Classification edge cases → classification.rs: XX tests copied
  - Total accounted for: XX tests
  - ✅ All tests migrated

classification.rs (renamed from status_tests.rs):
  - ✅ File renamed, tests unchanged

integration.rs (renamed from integration_tests.rs):
  - ✅ File renamed, tests unchanged

properties.rs (renamed from property_tests.rs):
  - ✅ File renamed, tests unchanged

---
TOTAL VERIFICATION:
  Old files total: XX tests
  New files total: XX tests
  ✅ Counts match - no tests lost
```

**Critical Check**: Old file test count == New file test count

---

## Step 6: Run Individual Test Suites

Verify each new test file works independently.

```bash
# Test each new module individually
cargo test --lib mutation::tests::parsing 2>&1 | grep "test result:"
cargo test --lib mutation::tests::classification 2>&1 | grep "test result:"
cargo test --lib mutation::tests::response_building 2>&1 | grep "test result:"
cargo test --lib mutation::tests::integration 2>&1 | grep "test result:"
cargo test --lib mutation::tests::properties 2>&1 | grep "test result:"
```

**Expected**: All tests pass in each module

**Record Results**:
```
parsing: XX passed
classification: XX passed
response_building: XX passed
integration: XX passed
properties: XX passed
---
TOTAL: XX passed
```

---

## Step 7: Check for Missing Imports or Dependencies

Look for compilation warnings or test failures that indicate missing dependencies.

```bash
# Check for warnings about unused imports
cargo test --lib mutation 2>&1 | grep "warning.*unused"

# Check for errors about missing items
cargo test --lib mutation 2>&1 | grep "error.*cannot find"
```

**Fix Issues**:
- Missing imports: Add to file or mod.rs
- Unused imports: Clean up (low priority)
- Cannot find type/function: Check if helper moved or needs import

---

## Step 8: Verify Helper Functions Work

If helper functions were moved to mod.rs in Phase 1, verify they work.

```bash
# Grep for helper function usage in tests
cd src/mutation/tests
grep -r "create_test_" *.rs | grep -v "^mod.rs"
```

**Verification**: Helper functions are accessible from all test files.

---

## Step 9: Create Phase 2 Verification Report

Document the test verification results.

**File**: `/tmp/phase-2-verification-report.txt`

```
Phase 2 Verification Report
============================

Date: [DATE]
Status: [PASS/FAIL]

Test Execution Results:
-----------------------
Total tests run: XXX
Tests passed: XXX
Tests failed: 0
Tests ignored: 0

New Module Tests:
-----------------
parsing.rs: XX passed
classification.rs: XX passed
response_building.rs: XX passed
integration.rs: XX passed
properties.rs: XX passed

Old Module Tests (still active):
---------------------------------
format_tests.rs: XX passed
auto_populate_fields_tests.rs: 5 passed
error_array_generation.rs: XX passed
validation_tests.rs: XX passed
composite_tests.rs: XX passed
edge_case_tests.rs: XX passed

Verification Checks:
--------------------
[✅] All tests pass
[✅] No tests lost (old count == new count)
[✅] No missing imports
[✅] Helper functions work
[✅] Each module works independently
[✅] Test count increased as expected (old + new)

Issues Found:
-------------
[List any issues, or write "None"]

Ready for Phase 3: [YES/NO]

Next Steps:
-----------
If all checks pass: Proceed to Phase 3 (Remove Old Files)
If issues found: Fix issues and re-run Phase 2 verification
```

---

## Verification Checklist

After Phase 2:
- [ ] mod.rs imports both old and new modules
- [ ] All tests pass (0 failures)
- [ ] Test count increased (old + new together)
- [ ] Individual test modules all pass
- [ ] No tests lost (verification confirms all migrated)
- [ ] No missing imports or dependencies
- [ ] Helper functions accessible from all files
- [ ] Phase 2 verification report created
- [ ] No blocking issues found

---

## Decision Point: Proceed to Phase 3?

**Criteria for proceeding**:
1. ✅ All tests pass
2. ✅ Test count verification confirms no tests lost
3. ✅ No compilation errors
4. ✅ Each new module works independently

**If all criteria met**: Proceed to Phase 3 (Remove Old Files)

**If any criteria not met**:
- Fix issues
- Re-run Phase 2 verification
- Do NOT proceed to Phase 3 until all checks pass

---

## Common Issues and Solutions

### Issue: "Test count doesn't match"

**Symptom**: New files have fewer tests than old files
**Cause**: Some tests not copied during Phase 1
**Solution**: Go back to Phase 1, find missing tests, copy them

### Issue: "Tests failing in new files but passing in old files"

**Symptom**: Same test behaves differently in new vs old file
**Cause**: Missing import, wrong context, or copy error
**Solution**: Compare test in old vs new file, fix differences

### Issue: "Cannot find type in scope"

**Symptom**: Compilation error about missing type
**Cause**: Type defined in old file, not imported in new file
**Solution**: Add `use super::super::old_file::Type;` or move type to mod.rs

### Issue: "Helper function not found"

**Symptom**: Test calls helper function that doesn't exist
**Cause**: Helper not copied to mod.rs or not imported
**Solution**: Add helper to mod.rs or import from old file temporarily

---

## Rollback Procedure

If Phase 2 reveals major issues:

```bash
# Option 1: Fix in place
# - Identify and fix issues
# - Re-run Phase 2 verification

# Option 2: Rollback to Phase 1
git checkout src/mutation/tests/mod.rs
# Fix Phase 1 issues, re-run Phase 1

# Option 3: Full rollback
git checkout test-reorganization-backup
git branch -D refactor/test-reorganization
# Start over from Phase 0
```

---

## Time Estimate

- Step 1 (Update mod.rs): 5 minutes
- Step 2 (Count tests): 5 minutes
- Step 3 (Run tests): 5 minutes
- Step 4 (Check conflicts): 2 minutes
- Step 5 (Verify counts): 10 minutes
- Step 6 (Individual suites): 5 minutes
- Step 7 (Check dependencies): 3 minutes
- Step 8 (Helper functions): 2 minutes
- Step 9 (Report): 5 minutes

**Total**: ~40 minutes

---

## Deliverables

After Phase 2:
1. ✅ mod.rs updated with both old and new imports
2. ✅ All tests pass (0 failures)
3. ✅ Test count verification complete
4. ✅ Individual module tests verified
5. ✅ Phase 2 verification report created
6. ✅ Ready for Phase 3 (remove old files)

---

## Next Phase

Proceed to:
- **Phase 3**: Remove Old Test Files

**Prerequisites for Phase 3**:
- [ ] All Phase 2 checks pass
- [ ] Verification report shows "Ready: YES"
- [ ] No unresolved issues
- [ ] Confident that new files contain all tests

**⚠️ Warning**: Phase 3 deletes old files. Ensure Phase 2 is completely verified before proceeding.

---

**Phase 2 Status**: ✅ Verification → Ready for Phase 3
