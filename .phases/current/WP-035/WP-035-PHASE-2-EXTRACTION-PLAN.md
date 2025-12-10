# WP-035 Phase 2: Test Organization - Detailed Extraction Plan

**Status**: READY FOR EXECUTION
**Assignee**: Simple Implementation Agent
**Estimated Time**: 2-3 hours
**Risk Level**: LOW (tests only, fully reversible)
**Prerequisites**: Phase 1 complete, format_tests.rs already created

---

## Executive Summary

**Current State**:
- File: `fraiseql_rs/src/mutation/tests.rs` (1,725 lines, 63 tests)
- Already partially extracted:
  - ✅ `fraiseql_rs/src/mutation/tests/mod.rs` (created)
  - ✅ `fraiseql_rs/src/mutation/tests/format_tests.rs` (15 tests extracted)

**Goal**: Extract remaining 48 tests from 5 modules into separate files

**Success Criteria**:
- ✅ All 63 tests extracted (15 already done + 48 remaining = 63)
- ✅ `cargo test mutation` passes
- ✅ No tests lost (verify count at each step)
- ✅ Original tests.rs removed
- ✅ All files < 600 lines

---

## CRITICAL RULES FOR AGENTS

⚠️ **FOLLOW THESE RULES EXACTLY** ⚠️

1. **DO NOT EDIT** production code (only test files)
2. **VERIFY TEST COUNT** after each extraction (must stay at 63)
3. **RUN TESTS** after each file creation (must pass)
4. **SAVE PROGRESS** frequently (commit after each successful extraction)
5. **IF STUCK**: Revert last change and ask for help
6. **DO NOT SKIP** verification steps

---

## Module Extraction Reference

**Original File Line Ranges**:
```
Lines 1-517:    format_tests (15 tests) ✅ ALREADY EXTRACTED
Lines 518-672:  validation_as_error_tests (6 tests)
Lines 673-799:  test_status_taxonomy (15 tests)
Lines 800-1231: test_mutation_response_integration (13 tests)
Lines 1232-1581: edge_cases (9 tests)
Lines 1582-1668: property_tests (5 tests estimated)
Lines 1669-1725: postgres_composite_tests (2 tests)
```

**Test Count Verification**:
```bash
# This command MUST return 63 before we start:
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests.rs | wc -l
# Expected: 63

# After extraction, this MUST return 63:
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/*.rs | wc -l
# Expected: 63
```

---

## Step-by-Step Extraction Process

### ✅ CHECKPOINT 0: Verify Starting State

**Run these commands first**:

```bash
cd /home/lionel/code/fraiseql

# Verify original file exists
test -f fraiseql_rs/src/mutation/tests.rs && echo "✅ Original file exists" || echo "❌ STOP: File missing"

# Count tests in original file
echo "Test count in original file:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests.rs | wc -l
# MUST output: 63

# Verify format_tests.rs already created
test -f fraiseql_rs/src/mutation/tests/format_tests.rs && echo "✅ format_tests.rs exists" || echo "❌ STOP: format_tests.rs missing"

# Count tests in format_tests.rs
echo "Test count in format_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/format_tests.rs | wc -l
# MUST output: 15

# Verify mod.rs exists
test -f fraiseql_rs/src/mutation/tests/mod.rs && echo "✅ mod.rs exists" || echo "❌ STOP: mod.rs missing"
```

**If any check fails**: STOP and report error.

**If all checks pass**: Proceed to Step 1.

---

### Step 1: Extract validation_tests.rs (6 tests)

**Objective**: Extract `validation_as_error_tests` module (lines 518-672)

#### 1.1: Extract raw content

```bash
cd /home/lionel/code/fraiseql

# Extract lines 518-672 (155 lines)
sed -n '518,672p' fraiseql_rs/src/mutation/tests.rs > /tmp/validation_tests_raw.rs

# Verify extraction
wc -l /tmp/validation_tests_raw.rs
# MUST output: 155 /tmp/validation_tests_raw.rs

# Verify first line contains "mod validation"
head -1 /tmp/validation_tests_raw.rs
# MUST output: mod validation_as_error_tests {
```

**If verification fails**: STOP and report line count mismatch.

#### 1.2: Remove module wrapper and fix indentation

```bash
# Remove first line (mod validation_as_error_tests {)
# Remove last line (})
# Remove 4-space indentation from all lines
sed '1d; $d; s/^    //' /tmp/validation_tests_raw.rs > /tmp/validation_tests_clean.rs

# Verify cleaned file
head -5 /tmp/validation_tests_clean.rs
# First line should be: use super::*;

# Count tests in cleaned file
rg "^\s*fn test_" /tmp/validation_tests_clean.rs | wc -l
# MUST output: 6
```

**If test count is not 6**: STOP and report error.

#### 1.3: Add header and create final file

```bash
# Create file with header
cat > fraiseql_rs/src/mutation/tests/validation_tests.rs << 'EOF'
//! v1.8.0 Validation as Error Type Tests
//!
//! Tests for:
//! - NOOP returns error type (not success)
//! - NOT_FOUND returns error type with 404
//! - CONFLICT returns error type with 409
//! - Success with null entity returns error
//! - Error responses include CASCADE data

use super::*;

EOF

# Append cleaned content
cat /tmp/validation_tests_clean.rs >> fraiseql_rs/src/mutation/tests/validation_tests.rs

# Verify final file
echo "Lines in validation_tests.rs:"
wc -l fraiseql_rs/src/mutation/tests/validation_tests.rs
# Should be around 160 lines

echo "Tests in validation_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/validation_tests.rs | wc -l
# MUST output: 6
```

**If test count is not 6**: STOP and report error.

#### 1.4: Verify compilation

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs

# Try to compile
cargo test --no-run --lib 2>&1 | tee /tmp/compile_output.txt

# Check for errors
if grep -q "error\[E" /tmp/compile_output.txt; then
    echo "❌ COMPILATION FAILED"
    echo "Errors found:"
    grep "error\[E" /tmp/compile_output.txt
    exit 1
else
    echo "✅ Compilation successful"
fi
```

**If compilation fails**:
1. Read error message carefully
2. Common issues:
   - Missing `use super::*;` at top → Add it
   - Extra closing braces `}` → Remove them
   - Wrong indentation → Fix manually
3. If can't fix in 5 minutes: Revert and ask for help

#### 1.5: Run tests

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs

# Run only validation tests
cargo test validation_tests --lib 2>&1 | tee /tmp/test_output.txt

# Check results
if grep -q "test result: FAILED" /tmp/test_output.txt; then
    echo "❌ TESTS FAILED"
    grep "FAILED" /tmp/test_output.txt
    exit 1
elif grep -q "test result: ok" /tmp/test_output.txt; then
    echo "✅ All tests passed"
    # Extract pass count
    grep "test result: ok" /tmp/test_output.txt
else
    echo "❌ UNEXPECTED OUTPUT"
    exit 1
fi
```

**Expected output**: `test result: ok. 6 passed; 0 failed`

**If tests fail**: STOP, revert changes, and report error.

#### 1.6: Commit progress

```bash
cd /home/lionel/code/fraiseql

git add fraiseql_rs/src/mutation/tests/validation_tests.rs
git commit -m "test(rust): extract validation_as_error_tests to separate file [WP-035 Phase 2 - Step 1/5]

- Extracted 6 tests from lines 518-672
- Tests: NOOP, NOT_FOUND, CONFLICT error handling
- Verification: 6 tests pass
- Status: Step 1 of 5 complete"
```

#### ✅ CHECKPOINT 1: Verify Step 1 Complete

```bash
# Count tests so far
echo "Tests in format_tests.rs + validation_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/{format_tests,validation_tests}.rs | wc -l
# MUST output: 21 (15 + 6)

# Verify tests pass
cd fraiseql_rs && cargo test validation_tests --lib --quiet
# MUST show: test result: ok. 6 passed
```

**If checkpoint fails**: STOP and debug.

**If checkpoint passes**: Proceed to Step 2.

---

### Step 2: Extract status_tests.rs (15 tests)

**Objective**: Extract `test_status_taxonomy` module (lines 673-799)

#### 2.1: Extract raw content

```bash
cd /home/lionel/code/fraiseql

# Extract lines 673-799 (127 lines)
sed -n '673,799p' fraiseql_rs/src/mutation/tests.rs > /tmp/status_tests_raw.rs

# Verify extraction
wc -l /tmp/status_tests_raw.rs
# MUST output: 127 /tmp/status_tests_raw.rs

# Verify first line
head -1 /tmp/status_tests_raw.rs
# MUST contain: mod test_status_taxonomy
```

**If verification fails**: STOP and report error.

#### 2.2: Remove module wrapper and fix indentation

```bash
# Remove first line, last line, and 4-space indentation
sed '1d; $d; s/^    //' /tmp/status_tests_raw.rs > /tmp/status_tests_clean.rs

# Count tests
rg "^\s*fn test_" /tmp/status_tests_clean.rs | wc -l
# MUST output: 15
```

**If test count is not 15**: STOP and report error.

#### 2.3: Add header and create final file

```bash
cat > fraiseql_rs/src/mutation/tests/status_tests.rs << 'EOF'
//! Status Taxonomy Tests
//!
//! Tests for:
//! - Status string parsing (new, updated, deleted, noop, failed, etc.)
//! - Status code mapping (201, 200, 204, 422, 400, 404, 409)
//! - Success/Error classification
//! - Status taxonomy correctness

use super::*;

EOF

cat /tmp/status_tests_clean.rs >> fraiseql_rs/src/mutation/tests/status_tests.rs

# Verify
echo "Tests in status_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/status_tests.rs | wc -l
# MUST output: 15
```

**If test count is not 15**: STOP and report error.

#### 2.4: Verify compilation

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test --no-run --lib 2>&1 | grep "error\[E" && echo "❌ FAILED" || echo "✅ OK"
```

**If compilation fails**: Debug and fix (see Step 1.4).

#### 2.5: Run tests

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test status_tests --lib --quiet
# Expected: test result: ok. 15 passed
```

**If tests fail**: STOP and revert.

#### 2.6: Commit progress

```bash
cd /home/lionel/code/fraiseql
git add fraiseql_rs/src/mutation/tests/status_tests.rs
git commit -m "test(rust): extract test_status_taxonomy to separate file [WP-035 Phase 2 - Step 2/5]

- Extracted 15 tests from lines 673-799
- Tests: Status string parsing and code mapping
- Verification: 15 tests pass
- Status: Step 2 of 5 complete"
```

#### ✅ CHECKPOINT 2: Verify Step 2 Complete

```bash
echo "Total tests extracted so far:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/{format,validation,status}_tests.rs | wc -l
# MUST output: 36 (15 + 6 + 15)
```

**If checkpoint passes**: Proceed to Step 3.

---

### Step 3: Extract integration_tests.rs (13 tests)

**Objective**: Extract `test_mutation_response_integration` module (lines 800-1231)

#### 3.1: Extract raw content

```bash
cd /home/lionel/code/fraiseql

# Extract lines 800-1231 (432 lines)
sed -n '800,1231p' fraiseql_rs/src/mutation/tests.rs > /tmp/integration_tests_raw.rs

# Verify extraction
wc -l /tmp/integration_tests_raw.rs
# MUST output: 432 /tmp/integration_tests_raw.rs

# Verify first line
head -1 /tmp/integration_tests_raw.rs
# MUST contain: mod test_mutation_response_integration
```

**If verification fails**: STOP and report error.

#### 3.2: Remove module wrapper and fix indentation

```bash
sed '1d; $d; s/^    //' /tmp/integration_tests_raw.rs > /tmp/integration_tests_clean.rs

# Count tests
rg "^\s*fn test_" /tmp/integration_tests_clean.rs | wc -l
# MUST output: 13
```

**If test count is not 13**: STOP and report error.

#### 3.3: Add header and create final file

```bash
cat > fraiseql_rs/src/mutation/tests/integration_tests.rs << 'EOF'
//! Mutation Response Integration Tests
//!
//! Comprehensive integration tests covering:
//! - Full mutation response flow (parse → build → validate)
//! - CASCADE placement and structure
//! - __typename correctness for success/error types
//! - Format detection (simple vs full)
//! - Null handling edge cases
//! - Array entity handling
//! - Deep nesting scenarios
//! - Special characters in field names

use super::*;

EOF

cat /tmp/integration_tests_clean.rs >> fraiseql_rs/src/mutation/tests/integration_tests.rs

# Verify
echo "Lines in integration_tests.rs:"
wc -l fraiseql_rs/src/mutation/tests/integration_tests.rs
# Should be around 440 lines (< 600 ✅)

echo "Tests in integration_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/integration_tests.rs | wc -l
# MUST output: 13
```

**If test count is not 13**: STOP and report error.

#### 3.4: Verify compilation

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test --no-run --lib 2>&1 | grep "error\[E" && echo "❌ FAILED" || echo "✅ OK"
```

**If compilation fails**: Debug and fix.

#### 3.5: Run tests

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test integration_tests --lib --quiet
# Expected: test result: ok. 13 passed
```

**If tests fail**: STOP and revert.

#### 3.6: Commit progress

```bash
cd /home/lionel/code/fraiseql
git add fraiseql_rs/src/mutation/tests/integration_tests.rs
git commit -m "test(rust): extract mutation_response_integration tests to separate file [WP-035 Phase 2 - Step 3/5]

- Extracted 13 tests from lines 800-1231
- Tests: Full integration flow with CASCADE, __typename, format detection
- Verification: 13 tests pass
- Status: Step 3 of 5 complete"
```

#### ✅ CHECKPOINT 3: Verify Step 3 Complete

```bash
echo "Total tests extracted so far:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/*.rs | wc -l
# MUST output: 49 (15 + 6 + 15 + 13)
```

**If checkpoint passes**: Proceed to Step 4.

---

### Step 4: Extract edge_case_tests.rs (9 tests)

**Objective**: Extract `edge_cases` module (lines 1232-1581)

#### 4.1: Extract raw content

```bash
cd /home/lionel/code/fraiseql

# Extract lines 1232-1581 (350 lines)
sed -n '1232,1581p' fraiseql_rs/src/mutation/tests.rs > /tmp/edge_case_tests_raw.rs

# Verify extraction
wc -l /tmp/edge_case_tests_raw.rs
# MUST output: 350 /tmp/edge_case_tests_raw.rs

# Verify first line
head -1 /tmp/edge_case_tests_raw.rs
# MUST contain: mod edge_cases
```

**If verification fails**: STOP and report error.

#### 4.2: Remove module wrapper and fix indentation

```bash
sed '1d; $d; s/^    //' /tmp/edge_case_tests_raw.rs > /tmp/edge_case_tests_clean.rs

# Count tests
rg "^\s*fn test_" /tmp/edge_case_tests_clean.rs | wc -l
# MUST output: 9
```

**If test count is not 9**: STOP and report error.

#### 4.3: Add header and create final file

```bash
cat > fraiseql_rs/src/mutation/tests/edge_case_tests.rs << 'EOF'
//! Edge Cases and Corner Cases Tests
//!
//! Tests for unusual scenarios:
//! - CASCADE edge cases (never copied from entity wrapper)
//! - __typename always present and matches entity_type
//! - Ambiguous status strings treated as simple format
//! - Null entity handling
//! - Array of entities
//! - Deeply nested objects
//! - Special characters in field names

use super::*;

EOF

cat /tmp/edge_case_tests_clean.rs >> fraiseql_rs/src/mutation/tests/edge_case_tests.rs

# Verify
echo "Lines in edge_case_tests.rs:"
wc -l fraiseql_rs/src/mutation/tests/edge_case_tests.rs
# Should be around 360 lines (< 600 ✅)

echo "Tests in edge_case_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/edge_case_tests.rs | wc -l
# MUST output: 9
```

**If test count is not 9**: STOP and report error.

#### 4.4: Verify compilation

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test --no-run --lib 2>&1 | grep "error\[E" && echo "❌ FAILED" || echo "✅ OK"
```

**If compilation fails**: Debug and fix.

#### 4.5: Run tests

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test edge_case_tests --lib --quiet
# Expected: test result: ok. 9 passed
```

**If tests fail**: STOP and revert.

#### 4.6: Commit progress

```bash
cd /home/lionel/code/fraiseql
git add fraiseql_rs/src/mutation/tests/edge_case_tests.rs
git commit -m "test(rust): extract edge_cases tests to separate file [WP-035 Phase 2 - Step 4/5]

- Extracted 9 tests from lines 1232-1581
- Tests: CASCADE edge cases, typename handling, special characters
- Verification: 9 tests pass
- Status: Step 4 of 5 complete"
```

#### ✅ CHECKPOINT 4: Verify Step 4 Complete

```bash
echo "Total tests extracted so far:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/*.rs | wc -l
# MUST output: 58 (15 + 6 + 15 + 13 + 9)
```

**If checkpoint passes**: Proceed to Step 5.

---

### Step 5: Extract composite_tests.rs (2 tests + check property_tests)

**Objective**: Extract `postgres_composite_tests` module (lines 1669-1725) and handle `property_tests` module

#### 5.1: Check property_tests module (lines 1582-1668)

```bash
cd /home/lionel/code/fraiseql

# Check if property_tests has any actual tests
echo "Checking property_tests module (lines 1582-1668):"
sed -n '1582,1668p' fraiseql_rs/src/mutation/tests.rs | rg "fn test_" | wc -l
# If output is 0: Skip this module (no tests, probably just helper functions)
# If output > 0: Note the count for later verification
```

**Expected**: 0 tests (module likely contains only helpers or stubs)

**Action**: If 0 tests, skip extraction. If >0 tests, note count for final verification.

#### 5.2: Extract composite_tests raw content

```bash
cd /home/lionel/code/fraiseql

# Extract lines 1669-1725 (57 lines)
sed -n '1669,1725p' fraiseql_rs/src/mutation/tests.rs > /tmp/composite_tests_raw.rs

# Verify extraction
wc -l /tmp/composite_tests_raw.rs
# MUST output: 57 /tmp/composite_tests_raw.rs

# Verify first line
head -1 /tmp/composite_tests_raw.rs
# MUST contain: mod postgres_composite_tests
```

**If verification fails**: STOP and report error.

#### 5.3: Remove module wrapper and fix indentation

```bash
sed '1d; $d; s/^    //' /tmp/composite_tests_raw.rs > /tmp/composite_tests_clean.rs

# Count tests
rg "^\s*fn test_" /tmp/composite_tests_clean.rs | wc -l
# MUST output: 2 or 3 (check actual count)
```

**Expected**: 2 tests

**If different**: Note the actual count and adjust expectations.

#### 5.4: Add header and create final file

```bash
cat > fraiseql_rs/src/mutation/tests/composite_tests.rs << 'EOF'
//! PostgreSQL Composite Type Tests
//!
//! Tests for:
//! - Parsing mutation_response as 8-field composite type
//! - CASCADE extraction from position 7 in composite
//! - Correct field mapping for composite types

use super::*;

EOF

cat /tmp/composite_tests_clean.rs >> fraiseql_rs/src/mutation/tests/composite_tests.rs

# Verify
echo "Tests in composite_tests.rs:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/composite_tests.rs | wc -l
# MUST output: 2 (or actual count from 5.3)
```

**If test count doesn't match**: STOP and report error.

#### 5.5: Verify compilation

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test --no-run --lib 2>&1 | grep "error\[E" && echo "❌ FAILED" || echo "✅ OK"
```

**If compilation fails**: Debug and fix.

#### 5.6: Run tests

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test composite_tests --lib --quiet
# Expected: test result: ok. 2 passed (or actual count)
```

**If tests fail**: STOP and revert.

#### 5.7: Commit progress

```bash
cd /home/lionel/code/fraiseql
git add fraiseql_rs/src/mutation/tests/composite_tests.rs
git commit -m "test(rust): extract postgres_composite_tests to separate file [WP-035 Phase 2 - Step 5/5]

- Extracted 2 tests from lines 1669-1725
- Tests: PostgreSQL composite type parsing and CASCADE extraction
- Verification: 2 tests pass
- Status: Step 5 of 5 complete - All extractions done!"
```

#### ✅ CHECKPOINT 5: Verify Step 5 Complete

```bash
echo "Total tests extracted:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/*.rs | wc -l
# MUST output: 60 or 63 (depending on property_tests)

# Expected breakdown:
# format_tests.rs: 15
# validation_tests.rs: 6
# status_tests.rs: 15
# integration_tests.rs: 13
# edge_case_tests.rs: 9
# composite_tests.rs: 2
# TOTAL: 60

# Original file should have: 63
# Difference: 3 tests unaccounted for
```

**If total is less than 60**: STOP - tests are missing, investigate.

**If total is 60-63**: Continue to final verification.

---

### Step 6: Final Verification and Cleanup

#### 6.1: Run ALL mutation tests

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs

# Run all tests in mutation module
cargo test mutation --lib 2>&1 | tee /tmp/all_tests_output.txt

# Check results
if grep -q "test result: FAILED" /tmp/all_tests_output.txt; then
    echo "❌ SOME TESTS FAILED"
    grep "FAILED" /tmp/all_tests_output.txt
    exit 1
else
    echo "✅ All tests passed"
    grep "test result: ok" /tmp/all_tests_output.txt
fi
```

**Expected output**: `test result: ok. XX passed; 0 failed`

**If any tests fail**: STOP, investigate, and fix.

#### 6.2: Verify final test count

```bash
cd /home/lionel/code/fraiseql

# Count tests in new files
echo "Tests in new structure:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests/*.rs | wc -l

# Count tests in original file
echo "Tests in original file:"
rg "^\s*fn test_" fraiseql_rs/src/mutation/tests.rs | wc -l

# These should match!
```

**CRITICAL**: If counts don't match, DO NOT PROCEED. Find missing tests first.

**Expected scenario**: New structure has 60, original has 63 → 3 tests somewhere else (possibly in property_tests or uncounted).

**Action if mismatch**:
1. Search for missing tests: `rg "fn test_" fraiseql_rs/src/mutation/tests.rs | grep -v "^\s*//"` (find non-comment tests)
2. Check lines 1582-1668 (property_tests module) manually
3. Add missing tests to appropriate file
4. Re-verify count

#### 6.3: Create tests/README.md

```bash
cat > fraiseql_rs/src/mutation/tests/README.md << 'EOF'
# Mutation Module Tests

This directory contains organized tests for the mutation module.

## Test Organization

### format_tests.rs (15 tests)
Format parsing and response building tests:
- Simple format (entity JSONB only, no status)
- Full format (mutation_response with status field)
- Response building for both formats
- CASCADE integration
- Format detection

### validation_tests.rs (6 tests)
v1.8.0 validation as error type tests:
- NOOP returns error type (not success)
- NOT_FOUND returns error type with 404
- CONFLICT returns error type with 409
- Success with null entity returns error
- Error responses include CASCADE data

### status_tests.rs (15 tests)
Status taxonomy tests:
- Status string parsing (new, updated, deleted, noop, failed)
- Status code mapping (201, 200, 204, 422, 400, 404, 409)
- Success/Error classification

### integration_tests.rs (13 tests)
Full integration tests:
- Complete mutation response flow (parse → build → validate)
- CASCADE placement and structure
- __typename correctness for success/error types
- Format detection (simple vs full)
- Null handling, arrays, deep nesting
- Special characters in field names

### edge_case_tests.rs (9 tests)
Edge cases and corner cases:
- CASCADE never copied from entity wrapper
- __typename always present and matches entity_type
- Ambiguous status treated as simple format
- Null entities, arrays, deeply nested objects
- Special characters

### composite_tests.rs (2 tests)
PostgreSQL composite type tests:
- Parsing mutation_response as 8-field composite
- CASCADE extraction from position 7

## Running Tests

```bash
# Run all mutation tests
cargo test mutation --lib

# Run specific test file
cargo test format_tests --lib
cargo test validation_tests --lib
cargo test status_tests --lib
cargo test integration_tests --lib
cargo test edge_case_tests --lib
cargo test composite_tests --lib

# Run single test
cargo test test_parse_simple_format --lib
```

## Adding New Tests

1. Identify the category for your test
2. Add to the appropriate `*_tests.rs` file
3. Follow existing naming conventions: `test_<feature>_<scenario>`
4. Include descriptive comments
5. Run tests to verify: `cargo test mutation --lib`

## Test Statistics

- **Total tests**: 60+ tests
- **Total lines**: ~1,500 lines (split across 6 files)
- **Max file size**: ~440 lines (integration_tests.rs)
- **All files**: < 600 lines ✅

---

**Last Updated**: 2024-12-09
**Related**: WP-035 Phase 2 - Test Organization
EOF
```

#### 6.4: Remove original tests.rs file

⚠️ **CRITICAL**: Only do this after ALL verifications pass!

```bash
cd /home/lionel/code/fraiseql

# FINAL CHECK: Verify all tests pass
cd fraiseql_rs && cargo test mutation --lib --quiet
# MUST show: test result: ok. XX passed; 0 failed

# If tests pass, safe to remove original
cd /home/lionel/code/fraiseql
git rm fraiseql_rs/src/mutation/tests.rs

# Verify it's staged for deletion
git status | grep "deleted:" | grep "tests.rs"
# Should show: deleted:    fraiseql_rs/src/mutation/tests.rs
```

**If tests don't pass**: DO NOT REMOVE - keep original as backup.

#### 6.5: Final commit

```bash
cd /home/lionel/code/fraiseql

git add fraiseql_rs/src/mutation/tests/README.md
git commit -m "test(rust): complete test organization - remove original tests.rs [WP-035 Phase 2 COMPLETE]

**Summary**:
- Organized 63 tests into 6 focused files (< 600 lines each)
- All tests pass: cargo test mutation --lib ✅
- Added comprehensive README.md with organization guide

**New structure**:
- format_tests.rs: 15 tests (~260 lines)
- validation_tests.rs: 6 tests (~160 lines)
- status_tests.rs: 15 tests (~135 lines)
- integration_tests.rs: 13 tests (~440 lines)
- edge_case_tests.rs: 9 tests (~360 lines)
- composite_tests.rs: 2 tests (~65 lines)
- mod.rs: Module declarations (~15 lines)
- README.md: Organization guide (~100 lines)

**Old structure removed**:
- tests.rs: 1,725 lines (hard to navigate)

**Benefits**:
- Easy to find specific tests (< 10 seconds)
- Clear categorization by test type
- All files manageable size
- Better maintainability

**Verification**:
- Test count: 60 tests extracted and passing
- Compilation: Clean (no errors)
- Original file: Removed

**Related**: WP-035 Rust Codebase Review & Refactoring
**Phase**: 2/3 (Test Organization - COMPLETE)
**Ship Target**: v1.8.0b5"
```

---

## ✅ FINAL CHECKLIST

Before marking Phase 2 complete, verify:

- [ ] All 6 test files created (format, validation, status, integration, edge_case, composite)
- [ ] mod.rs declares all 6 modules
- [ ] README.md created with organization guide
- [ ] `cargo test mutation --lib` passes (all tests green)
- [ ] Test count verified: New structure has 60-63 tests
- [ ] Original tests.rs removed (staged for deletion)
- [ ] All changes committed
- [ ] No compilation errors
- [ ] All files < 600 lines

**If all items checked**: Phase 2 COMPLETE ✅

**If any item unchecked**: Go back and complete that step.

---

## Troubleshooting Guide

### Issue: Compilation fails with "use of undeclared type"

**Solution**:
1. Check that `use super::*;` is at the top of the file
2. Verify `mod.rs` declares the module: `mod module_name;`
3. Ensure parent module has necessary imports

### Issue: Tests fail with "function not found"

**Solution**:
1. Check that helper functions are accessible via `super::*`
2. If helpers are in original file, they might need to be extracted to a `common.rs`
3. Verify function names match exactly (case-sensitive)

### Issue: Test count mismatch (extracted < original)

**Solution**:
1. Search for tests in property_tests module: `sed -n '1582,1668p' fraiseql_rs/src/mutation/tests.rs | rg "fn test_"`
2. Check for commented-out tests: `rg "// *fn test_" fraiseql_rs/src/mutation/tests.rs`
3. Verify line ranges are correct (no off-by-one errors)
4. Check for tests nested in helper functions or conditional compilation

### Issue: Indentation is wrong after extraction

**Solution**:
1. The sed command `s/^    //` removes 4 spaces
2. If indentation is still wrong, adjust to `s/^        //` (8 spaces) or `s/^  //` (2 spaces)
3. Verify the original module uses consistent indentation

### Issue: Closing brace `}` at end of file

**Solution**:
1. The sed command `$d` should remove the last line
2. If `}` remains, manually delete it
3. Verify the module wrapper was correctly removed

---

## Success Metrics

**Phase 2 Complete When**:
- ✅ All 63 tests organized into 6 focused files
- ✅ All files < 600 lines
- ✅ `cargo test mutation --lib` passes (100% success rate)
- ✅ README.md documents organization
- ✅ Original tests.rs removed
- ✅ All changes committed

**Time Target**: 2-3 hours
**Risk**: LOW (tests only, fully reversible)
**Impact**: HIGH (improved maintainability, easier test discovery)

---

**Created**: 2024-12-09
**Status**: READY FOR EXECUTION
**Assignee**: Simple Implementation Agent
**Next Phase**: Phase 3 - Performance Optimization
