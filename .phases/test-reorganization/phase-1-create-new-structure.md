# Phase 1: Create New Test Structure

## Objective

Create the new test files (`parsing.rs`, `classification.rs`, `response_building.rs`, `integration.rs`, `properties.rs`) and copy tests from old files to new locations. Keep old files intact for verification.

## Duration

1 hour

## Prerequisites

- Phase 0 completed
- Test migration map (`/tmp/test-migration-map.md`) created
- All current tests passing
- Working directory: `/home/lionel/code/fraiseql/fraiseql_rs`

---

## Step 1: Create parsing.rs

### Create File Structure

```bash
cd src/mutation/tests

# Create new file
touch parsing.rs
```

### Add File Header

**File**: `src/mutation/tests/parsing.rs`

```rust
//! Mutation Result Parsing Tests
//!
//! Tests for parsing PostgreSQL JSON into MutationResult structs.
//!
//! ## What This File Tests
//!
//! 1. **Simple Format** - Entity JSONB only, no status field
//!    - Format detection
//!    - Entity extraction
//!    - Array responses
//!
//! 2. **Full Format** - Complete mutation_response structure
//!    - Status, message, entity_id parsing
//!    - updated_fields extraction
//!    - CASCADE data handling
//!
//! 3. **PostgreSQL Composite Types**
//!    - 8-field mutation_response type
//!    - Type conversion (tuple → struct)
//!
//! 4. **Error Handling**
//!    - Malformed JSON
//!    - Missing required fields
//!    - Invalid field types
//!
//! ## Test Organization
//!
//! Tests are grouped by format type, then by specific behavior.
//! Each test is self-contained with its own setup.

use super::*;

// ============================================================================
// Simple Format Tests - Entity JSONB Only
// ============================================================================

// [Tests will be copied here from format_tests.rs]

// ============================================================================
// Full Format Tests - Complete mutation_response
// ============================================================================

// [Tests will be copied here from format_tests.rs]

// ============================================================================
// PostgreSQL Composite Type Tests
// ============================================================================

// [Tests will be copied here from composite_tests.rs]

// ============================================================================
// Format Detection Tests
// ============================================================================

// [Tests will be copied here from format_tests.rs]

// ============================================================================
// Error Handling Tests
// ============================================================================

// [Tests will be copied here from format_tests.rs]

// ============================================================================
// CASCADE Data Parsing Tests
// ============================================================================

// [Tests will be copied here from format_tests.rs]

// ============================================================================
// Edge Case Tests - Parsing
// ============================================================================

// [Tests will be copied here from edge_case_tests.rs - parsing-related only]
```

### Copy Tests from format_tests.rs

According to migration map, copy these sections:

```bash
# Extract parsing tests from format_tests.rs (lines 1-148 approximately)
# You'll need to manually copy the test functions that match:
# - test_parse_simple_format
# - test_parse_simple_format_array
# - test_parse_full_success_result
# - test_parse_full_error_result
# - test_parse_full_with_updated_fields
# - test_format_detection_simple_vs_full
# - test_parse_missing_status_fails
# - test_parse_invalid_json_fails
# - test_parse_simple_format_with_cascade
```

**Manual Task**: Copy test functions, preserving exact code.

### Copy Tests from composite_tests.rs

```bash
# Copy ALL tests from composite_tests.rs
# This file has ~64 lines, should be straightforward
```

### Copy Parsing Edge Cases from edge_case_tests.rs

**Manual Task**: Based on Phase 0 analysis, copy edge case tests related to parsing.

---

## Step 2: Create classification.rs

**Note**: This is a simple rename, not a new file.

```bash
cd src/mutation/tests

# Rename status_tests.rs to classification.rs
git mv status_tests.rs classification.rs
```

### Update File Header

**File**: `src/mutation/tests/classification.rs`

```rust
//! Mutation Status Classification Tests
//!
//! Tests for:
//! - Status string parsing (success, created, updated, noop:*, failed:*)
//! - Status taxonomy (Success/Error/Noop)
//! - Status code mapping (201, 200, 204, 422, 400, 404, 409, 500)
//! - Success/Error/Noop classification logic
//!
//! ## Status Taxonomy
//!
//! **Success Keywords** (no colon):
//! - success, created, updated, deleted, new
//!
//! **Noop Prefixes** (with colon):
//! - noop:not_found, noop:duplicate, noop:invalid_*
//!
//! **Error Prefixes** (with colon):
//! - failed:validation, failed:conflict, failed:*
//! - unauthorized:*, forbidden:*, timeout:*
//!
//! ## Test Organization
//!
//! Tests are grouped by status type, then by specific behavior.
//! Status code mapping tests verify REST-like codes (422, 404, etc.).

use super::*;

// [Existing tests from status_tests.rs remain unchanged]
```

**Verification**: File renamed, header updated, tests unchanged.

---

## Step 3: Create response_building.rs

### Create File Structure

```bash
cd src/mutation/tests

# Create new file (will be largest, ~900 lines)
touch response_building.rs
```

### Add File Header

**File**: `src/mutation/tests/response_building.rs`

```rust
//! GraphQL Response Building Tests
//!
//! Tests for building GraphQL-compliant JSON responses from MutationResult structs.
//!
//! ## What This File Tests
//!
//! 1. **Success Response Structure**
//!    - Field population (status, message, errors, id, updatedFields)
//!    - Entity wrapping and __typename
//!    - Field ordering consistency
//!
//! 2. **Error Response Structure**
//!    - Code field (422, 404, 409, 500)
//!    - Status routing (v1.8.0 behavior: noop → error type)
//!    - Null entity handling
//!
//! 3. **Error Array Generation**
//!    - Auto-generation from status strings
//!    - Explicit errors from metadata
//!    - Identifier extraction
//!
//! 4. **CASCADE Handling**
//!    - CASCADE placement (success-level, not entity-level)
//!    - Selection filtering
//!    - CamelCase conversion
//!
//! 5. **CamelCase Conversion**
//!    - Field name conversion
//!    - Entity key conversion
//!    - auto_camel_case flag behavior
//!
//! 6. **Array Responses**
//!    - Entity arrays
//!    - Multiple entity handling
//!
//! ## Test Organization
//!
//! Tests are grouped by response type (success/error), then by specific behavior.
//! Each major section has its own comment header for easy navigation.

use super::*;

// ============================================================================
// Success Response Structure Tests
// ============================================================================

// [Tests from format_tests.rs - success response section]
// [Tests from auto_populate_fields_tests.rs - all tests]

// ============================================================================
// Error Response Structure Tests
// ============================================================================

// [Tests from format_tests.rs - error response section]
// [Tests from validation_tests.rs - response routing tests]

// ============================================================================
// Error Array Generation Tests
// ============================================================================

// [ALL tests from error_array_generation.rs]

// ============================================================================
// CASCADE Handling Tests
// ============================================================================

// [Tests from format_tests.rs - CASCADE section]

// ============================================================================
// CamelCase Conversion Tests
// ============================================================================

// [Tests related to camelCase from various files]

// ============================================================================
// Array Response Tests
// ============================================================================

// [Tests from format_tests.rs - array section]

// ============================================================================
// Field Order and Consistency Tests
// ============================================================================

// [Tests from auto_populate_fields_tests.rs - field order test]

// ============================================================================
// Edge Case Tests - Response Building
// ============================================================================

// [Tests from edge_case_tests.rs - response-related only]
```

### Copy Tests - Success Response Section

**From `format_tests.rs`** (lines ~149-250):
- `test_build_simple_format_response`
- `test_build_simple_format_with_status_data_field`
- `test_build_full_success_response`

**From `auto_populate_fields_tests.rs`** (ALL 5 tests):
- `test_success_response_has_status_field`
- `test_success_response_has_errors_field`
- `test_success_response_all_standard_fields`
- `test_success_status_preserves_detail`
- `test_success_fields_order`

### Copy Tests - Error Response Section

**From `format_tests.rs`**:
- `test_build_full_error_response`

**From `validation_tests.rs`** (ALL tests):
- Copy all tests (they all test response routing)

### Copy Tests - Error Array Generation Section

**From `error_array_generation.rs`** (ALL tests):
- Copy entire file content (all tests)

### Copy Tests - CASCADE Section

**From `format_tests.rs`** (CASCADE tests):
- `test_build_simple_format_response_with_cascade`
- Any other CASCADE-related tests

### Copy Tests - Array Response Section

**From `format_tests.rs`**:
- `test_build_simple_format_array_response`

### Copy Response Building Edge Cases

**Manual Task**: Based on Phase 0 analysis, copy edge case tests related to response building.

---

## Step 4: Create integration.rs

**Note**: Simple rename, like classification.rs

```bash
cd src/mutation/tests

# Rename integration_tests.rs to integration.rs
git mv integration_tests.rs integration.rs
```

### Update File Header

**File**: `src/mutation/tests/integration.rs`

```rust
//! End-to-End Integration Tests
//!
//! Tests the complete mutation pipeline from JSON input to JSON output.
//!
//! ## What This File Tests
//!
//! Full pipeline scenarios:
//! - PostgreSQL JSON → Parsing → Classification → Response Building → GraphQL JSON
//! - Real-world mutation patterns
//! - Regression tests for known issues
//!
//! ## Test Organization
//!
//! Each test represents a complete, real-world mutation scenario.
//! Tests use realistic data and verify the entire flow.

use super::*;

// [Existing tests from integration_tests.rs remain unchanged]
```

---

## Step 5: Create properties.rs

**Note**: Simple rename, like classification.rs

```bash
cd src/mutation/tests

# Rename property_tests.rs to properties.rs
git mv property_tests.rs properties.rs
```

### Update File Header

**File**: `src/mutation/tests/properties.rs`

```rust
//! Property-Based Tests
//!
//! Property-based tests using quickcheck or proptest to verify invariants
//! that should hold for ANY input.
//!
//! ## Properties Tested
//!
//! - Parsing: Any valid JSON should parse without panicking
//! - Serialization: Parse → Serialize → Parse should be idempotent
//! - Response Building: Should never produce invalid GraphQL
//!
//! ## Test Organization
//!
//! Each property test defines an invariant and generates random test cases.

use super::*;

// [Existing tests from property_tests.rs remain unchanged]
```

---

## Step 6: Verify File Creation

```bash
cd src/mutation/tests

# Check new files exist
ls -lh parsing.rs classification.rs response_building.rs integration.rs properties.rs

# Check old files still exist (for now)
ls -lh format_tests.rs auto_populate_fields_tests.rs error_array_generation.rs validation_tests.rs composite_tests.rs

# Count lines in new files
wc -l parsing.rs classification.rs response_building.rs integration.rs properties.rs
```

**Expected**:
- 5 new files created (some via git mv)
- Old files still present (will be deleted in Phase 3)
- New files have substantial content (not empty stubs)

---

## Step 7: Add Helper Functions to mod.rs

If any shared helper functions were identified in Phase 0, add them to `mod.rs`.

**File**: `src/mutation/tests/mod.rs`

```rust
//! Tests for mutation module
//!
//! This module contains comprehensive tests for mutation parsing, classification,
//! and response building. Tests are organized by data pipeline stage.
//!
//! ## Test Organization
//!
//! - `parsing.rs` - Stage 1: PostgreSQL JSON → MutationResult
//! - `classification.rs` - Stage 2: Status taxonomy & routing
//! - `response_building.rs` - Stage 3: MutationResult → GraphQL JSON
//! - `integration.rs` - Stage 4: End-to-end scenarios
//! - `properties.rs` - Property-based tests (invariants)

use super::*;
use serde_json::{json, Value};

// ============================================================================
// Shared Test Utilities
// ============================================================================

// [Add any shared helper functions here]
// Example:
// pub fn create_test_mutation_result() -> MutationResult { ... }

// ============================================================================
// Test Modules (NEW STRUCTURE)
// ============================================================================

mod parsing;
mod classification;
mod response_building;
mod integration;
mod properties;

// ============================================================================
// Old Test Modules (Keep for Phase 2 Verification, Remove in Phase 3)
// ============================================================================

// These will be removed after verification in Phase 3
mod format_tests;
mod auto_populate_fields_tests;
mod error_array_generation;
mod validation_tests;
mod composite_tests;
mod edge_case_tests;
```

---

## Step 8: Build and Check for Compilation Errors

```bash
# Try to compile
cargo build --lib 2>&1 | tee /tmp/phase-1-build.log

# Check for errors
grep "^error" /tmp/phase-1-build.log
```

**Expected Issues**:
- Duplicate test names (old + new files both imported)
- Missing imports
- Syntax errors from copy/paste

**Fix Strategy**:
- Duplicate tests: Ignore for now (will be resolved in Phase 3)
- Missing imports: Add `use super::*;` or specific imports
- Syntax errors: Fix immediately

---

## Step 9: Document What Was Created

Create a summary of the new structure:

**File**: `/tmp/phase-1-summary.txt`

```
Phase 1 Summary - New Test Structure Created
=============================================

New Files Created:
1. parsing.rs (~470 lines)
   - Copied from: format_tests.rs (lines 1-148), composite_tests.rs (all), edge_case_tests.rs (parsing-related)
   - Tests: XX parsing tests

2. classification.rs (~133 lines) [RENAMED from status_tests.rs]
   - Tests: XX status tests

3. response_building.rs (~900 lines)
   - Copied from: format_tests.rs (lines 149-405), auto_populate_fields_tests.rs (all),
                  error_array_generation.rs (all), validation_tests.rs (all),
                  edge_case_tests.rs (response-related)
   - Tests: XX response building tests

4. integration.rs (~442 lines) [RENAMED from integration_tests.rs]
   - Tests: XX integration tests

5. properties.rs (~92 lines) [RENAMED from property_tests.rs]
   - Tests: XX property tests

Old Files (Still Present for Verification):
- format_tests.rs
- auto_populate_fields_tests.rs
- error_array_generation.rs
- validation_tests.rs
- composite_tests.rs
- edge_case_tests.rs
- status_tests.rs (if not renamed)
- integration_tests.rs (if not renamed)
- property_tests.rs (if not renamed)

Compilation Status:
[X] Compiles with warnings (duplicate tests expected)
[ ] Compiles without warnings
[ ] Does not compile (needs fixes)

Next Phase:
- Phase 2: Update mod.rs imports and verify all tests pass
```

---

## Verification Checklist

After Phase 1:
- [ ] 5 new test files created (parsing, classification, response_building, integration, properties)
- [ ] Old files still present (not deleted yet)
- [ ] mod.rs imports both old and new modules
- [ ] Code compiles (warnings about duplicates OK)
- [ ] File headers include comprehensive documentation
- [ ] Tests organized into clear sections with comment headers
- [ ] No syntax errors in new files
- [ ] Helper functions moved to mod.rs (if applicable)
- [ ] Phase 1 summary created

---

## Common Issues and Solutions

### Issue: "Test function not found"

**Cause**: Test copied incorrectly or incomplete
**Solution**: Compare with original file, ensure complete function copied

### Issue: "Cannot find `json` in scope"

**Cause**: Missing `use serde_json::{json, Value};`
**Solution**: Add `use super::*;` at top of file (inherits from mod.rs)

### Issue: "Duplicate test name"

**Cause**: Same test in both old and new files
**Solution**: Expected for Phase 1-2, will be resolved in Phase 3

### Issue: "File too large to edit"

**Cause**: response_building.rs is ~900 lines
**Solution**: Use search/replace or copy in sections

---

## Time Estimate

- Step 1 (parsing.rs): 15 minutes
- Step 2 (classification.rs): 2 minutes (rename)
- Step 3 (response_building.rs): 30 minutes (largest file)
- Step 4 (integration.rs): 2 minutes (rename)
- Step 5 (properties.rs): 2 minutes (rename)
- Step 6 (Verify): 2 minutes
- Step 7 (Helper functions): 5 minutes
- Step 8 (Build): 5 minutes
- Step 9 (Documentation): 5 minutes

**Total**: ~70 minutes

---

## Deliverables

After Phase 1:
1. ✅ New test files created (5 files)
2. ✅ Tests copied with proper organization
3. ✅ File headers with comprehensive documentation
4. ✅ mod.rs updated with new imports
5. ✅ Code compiles (warnings OK)
6. ✅ Phase 1 summary document created

---

## Next Phase

Proceed to:
- **Phase 2**: Update Imports & Verify Tests Pass

**Prerequisites for Phase 2**:
- [ ] All Phase 1 deliverables complete
- [ ] New files compile (even with warnings)
- [ ] No syntax errors
- [ ] Ready to run tests (old + new together)

---

**Phase 1 Status**: 🔨 Implementation → Ready for Phase 2
