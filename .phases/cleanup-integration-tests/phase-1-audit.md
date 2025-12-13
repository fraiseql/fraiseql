# Phase 1: Audit and Inventory (GREENFIELD)

## Objective
Create a complete, structured inventory of all integration test files requiring cleanup, categorized by the type of issue.

## Context
Before making any changes, we need a clear map of what needs to be done. This phase involves scanning the test suite and creating a machine-readable inventory that will guide the subsequent phases.

## Files to Create
- `tests/integration/.cleanup-inventory.json`

## Implementation Steps

### Step 1: Scan for Files with Process Hints in Names

Run this command to find all test files with problematic suffixes:

```bash
cd /home/lionel/code/fraiseql
find tests/integration -name "*.py" | grep -E "(fix|regression|simple|extended|complex|native|real_db|fixes)" | sort
```

**Expected output**: List of ~20-30 files with process hints in their names

**Action**: Copy this list - you'll need it for the inventory.

### Step 2: Scan for Duplicate Test File Groups

Look for test files that cover the same feature but have different suffixes:

```bash
# Field authorization duplicates
ls -1 tests/integration/auth/test_field_auth*.py 2>/dev/null || echo "No field auth files"

# Error array duplicates
ls -1 tests/integration/graphql/mutations/test_*error*.py 2>/dev/null || echo "No error files"

# Decorator duplicates
ls -1 tests/integration/auth/test_decorator*.py 2>/dev/null || echo "No decorator files"

# Validator duplicates
ls -1 tests/integration/auth/test_validator*.py 2>/dev/null || echo "No validator files"
```

**Expected output**: Groups of 2-4 related files per feature area

**Action**: Note which files are duplicates of each other.

### Step 3: Scan for Development Markers in Content

Search for common development markers across all test files:

```bash
# Work package references
grep -r "WP-[0-9]" tests/integration --include="*.py" | wc -l

# Phase references
grep -r "Phase [0-9]" tests/integration --include="*.py" | wc -l

# TDD markers
grep -r "\[RED\]\|\[GREEN\]\|\[REFACTOR\]" tests/integration --include="*.py" | wc -l

# Regression comments
grep -r "regression test\|verifies the fix\|Before the fix\|After the fix" tests/integration --include="*.py" -i | wc -l
```

**Expected output**: Count of files containing each type of marker

**Action**: Note the counts - this shows the scale of content cleanup needed.

### Step 4: Find Incomplete Tests

Search for placeholder tests:

```bash
grep -r "assert True" tests/integration --include="*.py" -B 5
```

**Expected output**: List of test functions with placeholder implementations

**Action**: Identify which files have incomplete tests.

### Step 5: Create Inventory JSON

Create a structured inventory file with all findings:

```bash
cat > tests/integration/.cleanup-inventory.json << 'EOF'
{
  "audit_date": "2025-12-13",
  "categories": {
    "duplicates": {
      "description": "Files that need to be consolidated",
      "groups": [
        {
          "feature": "field_authorization",
          "keep": "tests/integration/auth/test_field_authorization.py",
          "merge": [
            "tests/integration/auth/test_field_authorization_simple.py",
            "tests/integration/auth/test_field_authorization_fixed.py",
            "tests/integration/auth/test_field_auth_complex.py"
          ],
          "final_name": "tests/integration/auth/test_field_authorization.py"
        },
        {
          "feature": "error_arrays",
          "keep": "tests/integration/graphql/mutations/test_native_error_arrays.py",
          "delete": ["tests/integration/graphql/mutations/test_error_arrays.py"],
          "final_name": "tests/integration/graphql/mutations/test_error_arrays.py"
        },
        {
          "feature": "decorators",
          "keep": "tests/integration/auth/test_decorators_extended.py",
          "merge": [],
          "final_name": "tests/integration/auth/test_decorators.py"
        },
        {
          "feature": "validators",
          "keep": "tests/integration/auth/test_validators_extended.py",
          "merge": [],
          "final_name": "tests/integration/auth/test_validators.py"
        }
      ]
    },
    "renames": {
      "description": "Files that need renaming to remove process hints",
      "files": [
        {
          "old": "tests/integration/graphql/test_json_passthrough_config_fix.py",
          "new": "tests/integration/graphql/test_json_passthrough.py",
          "reason": "Remove '_fix' suffix"
        },
        {
          "old": "tests/integration/graphql/test_enum_conversion_fix.py",
          "new": "tests/integration/graphql/test_enum_conversion.py",
          "reason": "Remove '_fix' suffix"
        },
        {
          "old": "tests/integration/graphql/mutations/test_similar_mutation_names_collision_fix.py",
          "new": "tests/integration/graphql/mutations/test_mutation_name_resolution.py",
          "reason": "Remove '_fix' and improve name clarity"
        },
        {
          "old": "tests/integration/repository/test_graphql_where_repository_fix.py",
          "new": "tests/integration/repository/test_graphql_where_repository.py",
          "reason": "Remove '_fix' suffix"
        },
        {
          "old": "tests/integration/graphql/test_nested_object_tenant_id_fix.py",
          "new": "tests/integration/graphql/test_nested_object_tenant_id.py",
          "reason": "Remove '_fix' suffix"
        },
        {
          "old": "tests/integration/graphql/test_nested_tenant_fix_real_db.py",
          "new": "tests/integration/graphql/test_nested_tenant_integration.py",
          "reason": "Remove '_fix_real_db', use '_integration' suffix"
        },
        {
          "old": "tests/integration/operators/test_network_fixes.py",
          "new": "tests/integration/operators/test_network_filtering.py",
          "reason": "Remove '_fixes', improve name clarity"
        },
        {
          "old": "tests/integration/graphql/mutations/test_simple_mutation_regression.py",
          "new": "tests/integration/graphql/mutations/test_simple_mutations.py",
          "reason": "Remove '_regression' suffix"
        },
        {
          "old": "tests/integration/graphql/test_order_by_list_dict_regression.py",
          "new": "tests/integration/graphql/test_order_by_list_dict.py",
          "reason": "Remove '_regression' suffix"
        },
        {
          "old": "tests/integration/performance/test_performance_regression.py",
          "new": "tests/integration/performance/test_performance.py",
          "reason": "Remove '_regression' suffix"
        },
        {
          "old": "tests/integration/graphql/test_enum_parameter_simple.py",
          "new": "tests/integration/graphql/test_enum_parameters.py",
          "reason": "Remove '_simple' suffix"
        },
        {
          "old": "tests/integration/e2e/test_blog_simple_integration.py",
          "new": "tests/integration/e2e/test_blog_integration.py",
          "reason": "Remove '_simple' suffix"
        },
        {
          "old": "tests/integration/e2e/test_db_integration_simple.py",
          "new": "tests/integration/e2e/test_db_integration.py",
          "reason": "Remove '_simple' suffix"
        },
        {
          "old": "tests/integration/graphql/test_orderby_complex_scenarios.py",
          "new": "tests/integration/graphql/test_orderby_scenarios.py",
          "reason": "Remove '_complex' suffix"
        },
        {
          "old": "tests/integration/repository/test_where_generator_extended.py",
          "new": "tests/integration/repository/test_where_generator.py",
          "reason": "Remove '_extended' suffix"
        },
        {
          "old": "tests/integration/performance/test_n_plus_one_detector_extended.py",
          "new": "tests/integration/performance/test_n_plus_one_detector.py",
          "reason": "Remove '_extended' suffix"
        },
        {
          "old": "tests/integration/meta/test_phase0_validation.py",
          "new": "tests/integration/meta/test_schema_validation.py",
          "reason": "Remove 'phase0', improve name clarity"
        }
      ]
    },
    "content_cleanup": {
      "description": "Patterns to remove from all test files",
      "markers": [
        "WP-XXX work package references",
        "Phase X development phase mentions",
        "RED/GREEN/REFACTOR TDD markers",
        "Regression test for... comments",
        "This test verifies the fix for... comments",
        "Fixed version in docstrings",
        "Version numbers and dates",
        "TODO comments in implementations",
        "Before the fix.../After the fix... comments",
        "old behavior vs new behavior references",
        "Historical architectural decision explanations"
      ],
      "affected_files_count": "~50+ files (most of integration suite)"
    },
    "incomplete_tests": {
      "description": "Files with placeholder tests to remove",
      "files": [
        {
          "path": "tests/integration/graphql/mutations/test_error_arrays.py",
          "action": "DELETE",
          "reason": "Only contains placeholder tests, duplicate of test_native_error_arrays.py"
        }
      ]
    }
  },
  "statistics": {
    "total_files_to_rename": 17,
    "duplicate_groups": 4,
    "files_to_delete": 1,
    "estimated_files_needing_content_cleanup": 50
  }
}
EOF
```

**Expected output**: JSON file created successfully

**Action**: Review the JSON to ensure it matches your findings from steps 1-4.

### Step 6: Verify Inventory Completeness

Check that the inventory is complete and valid:

```bash
# Validate JSON syntax
python3 -m json.tool tests/integration/.cleanup-inventory.json > /dev/null && echo "✓ JSON valid" || echo "✗ JSON invalid"

# Count entries match expectations
echo "Duplicate groups: $(jq '.categories.duplicates.groups | length' tests/integration/.cleanup-inventory.json)"
echo "Rename files: $(jq '.categories.renames.files | length' tests/integration/.cleanup-inventory.json)"
echo "Delete files: $(jq '.categories.incomplete_tests.files | length' tests/integration/.cleanup-inventory.json)"
```

**Expected output**:
```
✓ JSON valid
Duplicate groups: 4
Rename files: 17
Delete files: 1
```

### Step 7: Create Summary Report

Generate a human-readable summary:

```bash
cat > tests/integration/.cleanup-summary.txt << 'EOF'
Integration Tests Cleanup - Audit Summary
==========================================
Date: 2025-12-13

SCOPE OF WORK:
--------------
1. Consolidate 4 groups of duplicate test files
2. Rename 17 test files to remove process hints
3. Delete 1 incomplete test file
4. Clean content in ~50 test files

DUPLICATE FILE GROUPS:
----------------------
- Field Authorization: 4 files → 1 file
- Error Arrays: 2 files → 1 file
- Decorators: 1 file → rename only
- Validators: 1 file → rename only

RENAME CATEGORIES:
------------------
- Remove "_fix" suffix: 6 files
- Remove "_regression" suffix: 3 files
- Remove "_simple/_extended/_complex" suffix: 7 files
- Improve name clarity: 1 file

CONTENT CLEANUP:
----------------
Patterns to remove from all files:
- WP-XXX references
- Phase markers
- TDD cycle markers
- Regression/fix comments
- Version numbers
- Historical explanations

FILES TO DELETE:
----------------
- test_error_arrays.py (placeholder duplicate)

NEXT STEPS:
-----------
1. Proceed to Phase 2: Consolidate duplicates
2. Then Phase 3: Rename files
3. Then Phase 4: Clean content
4. Finally Phase 5: Verify and QA

ESTIMATED EFFORT:
-----------------
Total: ~7 hours across all phases
EOF

cat tests/integration/.cleanup-summary.txt
```

**Expected output**: Summary report displayed

## Verification Commands

Run these commands to verify Phase 1 completion:

```bash
# Check inventory file exists and is valid
test -f tests/integration/.cleanup-inventory.json && echo "✓ Inventory exists" || echo "✗ Inventory missing"
python3 -m json.tool tests/integration/.cleanup-inventory.json > /dev/null && echo "✓ JSON valid" || echo "✗ JSON invalid"

# Check summary exists
test -f tests/integration/.cleanup-summary.txt && echo "✓ Summary exists" || echo "✗ Summary missing"

# Verify counts
echo "Expected: 4 duplicate groups, 17 renames, 1 delete"
echo "Actual: $(jq '.statistics.duplicate_groups' tests/integration/.cleanup-inventory.json) groups, $(jq '.statistics.total_files_to_rename' tests/integration/.cleanup-inventory.json) renames, $(jq '.statistics.files_to_delete' tests/integration/.cleanup-inventory.json) deletes"
```

**Expected output**: All checks pass with ✓

## Acceptance Criteria

- [ ] `.cleanup-inventory.json` exists and is valid JSON
- [ ] `.cleanup-summary.txt` exists and is readable
- [ ] Inventory contains 4 duplicate groups
- [ ] Inventory contains 17 files to rename
- [ ] Inventory identifies 1 file to delete
- [ ] Summary report is clear and complete

## Commit

After verification passes:

```bash
git add tests/integration/.cleanup-inventory.json tests/integration/.cleanup-summary.txt
git commit -m "chore(tests): audit integration tests for cleanup [GREENFIELD]

Create inventory of files needing consolidation, renaming, and content cleanup.

- 4 duplicate file groups identified
- 17 files to rename (remove process hints)
- 1 incomplete file to delete
- ~50 files need content cleanup"
```

## DO NOT

- ❌ Make any changes to test files yet (only create inventory)
- ❌ Skip the verification commands
- ❌ Proceed to Phase 2 without committing this phase
- ❌ Modify the inventory JSON manually without updating counts

## Notes for Junior Engineers

**What is this phase doing?**
Creating a detailed map before making changes. Think of it like surveying a construction site before building.

**Why JSON format?**
Machine-readable format that can be parsed by scripts in later phases if needed.

**What if I find additional files?**
Add them to the inventory JSON in the appropriate category. Update the statistics section to reflect new counts.

**How long should this take?**
~30 minutes if you follow the commands exactly. Take your time to understand what each command reveals.
