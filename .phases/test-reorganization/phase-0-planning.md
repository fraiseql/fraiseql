# Phase 0: Planning & Preparation

## Objective

Create a complete test inventory, analyze edge_case_tests.rs to understand what it contains, and prepare for the reorganization.

## Duration

30 minutes

## Prerequisites

- Current working directory: `/home/lionel/code/fraiseql/fraiseql_rs`
- All existing tests passing
- Clean git status (no uncommitted changes)

---

## Step 1: Create Backup Branch

```bash
# Check current branch
git branch --show-current

# Create backup branch from current state
git checkout -b test-reorganization-backup

# Return to main/working branch
git checkout main  # or whatever branch you're on

# Create feature branch for reorganization
git checkout -b refactor/test-reorganization
```

**Verification**:
```bash
git branch | grep test-reorganization
# Should show:
#   test-reorganization-backup
# * refactor/test-reorganization
```

---

## Step 2: Run Baseline Test Count

```bash
# Run all mutation tests and count them
cargo test --lib mutation 2>&1 | tee /tmp/test-baseline.log

# Extract test count
grep "test result:" /tmp/test-baseline.log
```

**Expected Output**:
```
test result: ok. XX passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Record this number**: ___________ tests passed

**Verification**: All tests must pass before proceeding.

---

## Step 3: Create Test Inventory

Create a complete mapping of every test function to its destination file.

```bash
# List all test functions with their file and line number
cd src/mutation/tests
grep -n "^fn test_" *.rs | sort > /tmp/test-inventory-source.txt

# Count total tests
wc -l /tmp/test-inventory-source.txt
```

**Manual Task**: Review `/tmp/test-inventory-source.txt` and categorize each test.

### Inventory Template

Create `/tmp/test-inventory-categorized.txt` with format:
```
# SOURCE_FILE:LINE:TEST_NAME → DESTINATION_FILE [CATEGORY]

format_tests.rs:17:test_parse_simple_format → parsing.rs [SIMPLE_FORMAT]
format_tests.rs:35:test_parse_simple_format_array → parsing.rs [SIMPLE_FORMAT]
...
```

---

## Step 4: Analyze edge_case_tests.rs

**Unknown File**: We need to understand what `edge_case_tests.rs` contains before we can categorize it.

```bash
# Read the file
cat src/mutation/tests/edge_case_tests.rs | head -50

# List all test functions in the file
grep "^fn test_" src/mutation/tests/edge_case_tests.rs
```

**Manual Analysis Required**:
1. Read file header comment (what does it claim to test?)
2. Identify test categories:
   - Parsing edge cases → `parsing.rs`
   - Response building edge cases → `response_building.rs`
   - Status classification edge cases → `classification.rs`
   - Other edge cases → Keep as separate section or distribute

**Decision Point**:
- Option A: Distribute edge case tests to appropriate files based on what they test
- Option B: Keep `edge_cases.rs` as a separate file for truly weird edge cases
- **Recommendation**: Option A (distribute), but mark them with `// EDGE CASE:` comment

**Output**: Document edge case test categorization in inventory.

---

## Step 5: Create Complete Test Mapping

Based on analysis, create final mapping document.

**File**: `/tmp/test-migration-map.md`

**Format**:
```markdown
# Test Migration Map

## parsing.rs (Target: ~470 lines)

### From format_tests.rs (9 tests)
- Line 17: test_parse_simple_format
- Line 35: test_parse_simple_format_array
- Line 50: test_parse_full_success_result
- Line 72: test_parse_full_error_result
- Line 93: test_parse_full_with_updated_fields
- Line 116: test_format_detection_simple_vs_full
- Line 133: test_parse_missing_status_fails
- Line 141: test_parse_invalid_json_fails
- Line 326: test_parse_simple_format_with_cascade

### From composite_tests.rs (all tests)
- [List all composite test functions]

### From edge_case_tests.rs (parsing-related)
- [List edge cases related to parsing]

## classification.rs (Target: ~133 lines)

### From status_tests.rs (rename, all tests)
- [All tests kept as-is, just file renamed]

### From edge_case_tests.rs (status-related)
- [List edge cases related to status]

## response_building.rs (Target: ~900 lines)

### Section: Success Response Structure
From format_tests.rs:
- Line 152: test_build_simple_format_response
- Line 184: test_build_simple_format_with_status_data_field
- Line 219: test_build_full_success_response

From auto_populate_fields_tests.rs:
- Line 8: test_success_response_has_status_field
- Line 43: test_success_response_has_errors_field
- Line 79: test_success_response_all_standard_fields
- Line 123: test_success_status_preserves_detail
- Line 152: test_success_fields_order

### Section: Error Response Structure
From format_tests.rs:
- Line 252: test_build_full_error_response

From validation_tests.rs:
- Line 15: test_noop_returns_error_type_v1_8
- Line 40: test_not_found_returns_error_type_with_404
- Line 65: test_conflict_returns_error_type_with_409
- Line 90: test_success_with_null_entity_returns_error
- Line 115: test_error_response_includes_cascade

### Section: Error Array Generation
From error_array_generation.rs (all):
- [List all error array generation tests]

### Section: CASCADE Handling
From format_tests.rs:
- Line 344: test_build_simple_format_response_with_cascade

From edge_case_tests.rs (cascade-related):
- [List cascade edge cases]

### Section: Array Responses
From format_tests.rs:
- Line 288: test_build_simple_format_array_response

### From edge_case_tests.rs (response-related)
- [List response building edge cases]

## integration.rs (Target: ~442 lines)

### From integration_tests.rs (rename, all tests)
- [All tests kept as-is, just file renamed]

## properties.rs (Target: ~92 lines)

### From property_tests.rs (rename, all tests)
- [All tests kept as-is, just file renamed]
```

---

## Step 6: Verify Test Count Math

Calculate expected test distribution:

```
Source Files:
- format_tests.rs:             XX tests
- auto_populate_fields_tests.rs: 5 tests
- error_array_generation.rs:   XX tests
- validation_tests.rs:         XX tests
- edge_case_tests.rs:          XX tests
- composite_tests.rs:          XX tests
- status_tests.rs:             XX tests
- integration_tests.rs:        XX tests
- property_tests.rs:           XX tests
-------------------------------------------
TOTAL:                         XX tests (must match Step 2)

Target Files:
- parsing.rs:                  XX tests
- classification.rs:           XX tests
- response_building.rs:        XX tests
- integration.rs:              XX tests
- properties.rs:               XX tests
-------------------------------------------
TOTAL:                         XX tests (must match source total)
```

**Verification**: Source total == Target total

---

## Step 7: Document Risky Areas

Identify tests that might be tricky to migrate:

### Tests with Cross-File Dependencies

Look for tests that:
- Use helper functions from other test files
- Share fixtures or test data
- Have ordering dependencies

**Command**:
```bash
# Find test helper functions (not test_ prefixed)
cd src/mutation/tests
grep -n "^fn [^test_]" *.rs | grep -v "^mod.rs"
```

**Document**:
- Which helpers exist
- Which test files use them
- Whether helpers should move to mod.rs

### Tests with External File Dependencies

Look for tests that:
- Load fixture files
- Reference test data files

**Command**:
```bash
# Find file I/O in tests
grep -n "File::\|Path::\|include_str!\|include_bytes!" src/mutation/tests/*.rs
```

---

## Step 8: Create Phase 1 Prep Checklist

**Checklist**: `/tmp/phase-1-prep.txt`

```
Pre-Phase-1 Checklist:
[ ] Backup branch created
[ ] Feature branch created
[ ] Baseline test count recorded: _____ tests
[ ] Test inventory created
[ ] edge_case_tests.rs analyzed and categorized
[ ] Complete test migration map created
[ ] Test count math verified (source == target)
[ ] Helper functions documented
[ ] No external file dependencies found
[ ] All current tests passing
[ ] Git status clean
```

---

## Deliverables

After Phase 0, you should have:

1. ✅ Backup branch: `test-reorganization-backup`
2. ✅ Feature branch: `refactor/test-reorganization`
3. ✅ Baseline test count: `_____ tests`
4. ✅ Test inventory: `/tmp/test-inventory-categorized.txt`
5. ✅ edge_case_tests.rs analysis: Documented categorization
6. ✅ Complete migration map: `/tmp/test-migration-map.md`
7. ✅ Helper functions inventory: Documented in map
8. ✅ Phase 1 prep checklist: Completed

---

## Expected Output Files

```
/tmp/
├── test-baseline.log                    # Test output from cargo test
├── test-inventory-source.txt            # Raw test list
├── test-inventory-categorized.txt       # Categorized test list
├── test-migration-map.md                # Complete migration mapping
└── phase-1-prep.txt                     # Pre-Phase-1 checklist
```

---

## Decision Points

### 1. edge_case_tests.rs Strategy

After analyzing the file, decide:
- [ ] Distribute all tests to appropriate files (recommended)
- [ ] Keep some "truly edge" cases in a separate file
- [ ] Merge into `response_building.rs` with "Edge Cases" section

**Recommendation**: Distribute, mark with `// EDGE CASE:` comments

### 2. Helper Function Location

If helper functions are found:
- [ ] Move to `mod.rs` as shared utilities
- [ ] Duplicate in each file that needs them
- [ ] Keep in original files and use `use super::super::old_file::helper`

**Recommendation**: Move shared helpers to `mod.rs`

### 3. Test Naming Conventions

Should we rename tests to reflect new organization?
- [ ] Keep original names (easier to track in git history)
- [ ] Rename to match new file structure (more consistent)

**Recommendation**: Keep original names for Phase 1-2, consider renaming in Phase 4

---

## Verification Commands

```bash
# Verify backup branch exists
git branch | grep test-reorganization-backup

# Verify on feature branch
git branch | grep "^\* refactor/test-reorganization"

# Verify test count
grep "test result:" /tmp/test-baseline.log

# Verify inventory complete
wc -l /tmp/test-inventory-categorized.txt

# Verify migration map exists
test -f /tmp/test-migration-map.md && echo "✅ Map exists" || echo "❌ Map missing"
```

---

## Troubleshooting

### "Tests are failing"
**Solution**: Fix tests before proceeding. All tests must pass in baseline.

### "Can't find edge_case_tests.rs"
**Solution**: Check if file was renamed or deleted. Update inventory accordingly.

### "Test count doesn't match"
**Solution**: Recount carefully. Some tests might be `#[ignore]` or conditional.

### "Helper functions are complex"
**Solution**: Document complexity, plan to refactor in separate PR if needed.

---

## Next Phase

After completing Phase 0, proceed to:
- **Phase 1**: Create New Test Structure

**Prerequisites for Phase 1**:
- [ ] All Phase 0 deliverables created
- [ ] Test migration map reviewed and approved
- [ ] Decision points resolved
- [ ] Ready to create new test files

---

## Time Estimate

- Step 1 (Backup): 2 minutes
- Step 2 (Baseline): 5 minutes
- Step 3 (Inventory): 5 minutes
- Step 4 (Analyze edge_case): 10 minutes
- Step 5 (Migration map): 5 minutes
- Step 6 (Verify math): 2 minutes
- Step 7 (Risky areas): 5 minutes
- Step 8 (Checklist): 1 minute

**Total**: ~35 minutes

---

**Phase 0 Status**: 📋 Planning Complete → Ready for Phase 1
