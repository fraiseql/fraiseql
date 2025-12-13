# Phase 3: Rename Files to Remove Process Hints (REFACTOR)

## Objective
Rename all test files to remove development process hints like `_fix`, `_regression`, `_simple`, `_extended`, `_complex`, etc.

## Context
Many test files have names that reveal the iterative development process. These need clean, professional names that describe WHAT they test, not WHEN or WHY they were created.

## Files to Modify
Based on inventory: 17 files to rename (plus 2 from Phase 2 consolidation)

## Implementation Steps

### Step 1: Rename Files with "_fix" Suffix (6 files)

#### 1.1: JSON passthrough test

```bash
git mv tests/integration/graphql/test_json_passthrough_config_fix.py \
       tests/integration/graphql/test_json_passthrough.py
```

**Verify**: `test -f tests/integration/graphql/test_json_passthrough.py && echo "✓" || echo "✗"`

#### 1.2: Enum conversion test

```bash
git mv tests/integration/graphql/test_enum_conversion_fix.py \
       tests/integration/graphql/test_enum_conversion.py
```

**Verify**: `test -f tests/integration/graphql/test_enum_conversion.py && echo "✓" || echo "✗"`

#### 1.3: Mutation name collision test

```bash
git mv tests/integration/graphql/mutations/test_similar_mutation_names_collision_fix.py \
       tests/integration/graphql/mutations/test_mutation_name_resolution.py
```

**Verify**: `test -f tests/integration/graphql/mutations/test_mutation_name_resolution.py && echo "✓" || echo "✗"`

#### 1.4: GraphQL where repository test

```bash
git mv tests/integration/repository/test_graphql_where_repository_fix.py \
       tests/integration/repository/test_graphql_where_repository.py
```

**Verify**: `test -f tests/integration/repository/test_graphql_where_repository.py && echo "✓" || echo "✗"`

#### 1.5: Nested object tenant ID test

```bash
git mv tests/integration/graphql/test_nested_object_tenant_id_fix.py \
       tests/integration/graphql/test_nested_object_tenant_id.py
```

**Verify**: `test -f tests/integration/graphql/test_nested_object_tenant_id.py && echo "✓" || echo "✗"`

#### 1.6: Nested tenant integration test

```bash
git mv tests/integration/graphql/test_nested_tenant_fix_real_db.py \
       tests/integration/graphql/test_nested_tenant_integration.py
```

**Verify**: `test -f tests/integration/graphql/test_nested_tenant_integration.py && echo "✓" || echo "✗"`

### Step 2: Rename Files with "_regression" Suffix (3 files)

#### 2.1: Simple mutations test

```bash
git mv tests/integration/graphql/mutations/test_simple_mutation_regression.py \
       tests/integration/graphql/mutations/test_simple_mutations.py
```

**Verify**: `test -f tests/integration/graphql/mutations/test_simple_mutations.py && echo "✓" || echo "✗"`

#### 2.2: Order by list/dict test

```bash
git mv tests/integration/graphql/test_order_by_list_dict_regression.py \
       tests/integration/graphql/test_order_by_list_dict.py
```

**Verify**: `test -f tests/integration/graphql/test_order_by_list_dict.py && echo "✓" || echo "✗"`

#### 2.3: Performance test

```bash
git mv tests/integration/performance/test_performance_regression.py \
       tests/integration/performance/test_performance.py
```

**Verify**: `test -f tests/integration/performance/test_performance.py && echo "✓" || echo "✗"`

### Step 3: Rename Files with "_simple/_extended/_complex" Suffix (7 files)

#### 3.1: Enum parameters test

```bash
git mv tests/integration/graphql/test_enum_parameter_simple.py \
       tests/integration/graphql/test_enum_parameters.py
```

**Verify**: `test -f tests/integration/graphql/test_enum_parameters.py && echo "✓" || echo "✗"`

#### 3.2: Blog integration test

```bash
git mv tests/integration/e2e/test_blog_simple_integration.py \
       tests/integration/e2e/test_blog_integration.py
```

**Verify**: `test -f tests/integration/e2e/test_blog_integration.py && echo "✓" || echo "✗"`

#### 3.3: DB integration test

```bash
git mv tests/integration/e2e/test_db_integration_simple.py \
       tests/integration/e2e/test_db_integration.py
```

**Verify**: `test -f tests/integration/e2e/test_db_integration.py && echo "✓" || echo "✗"`

#### 3.4: Order by scenarios test

```bash
git mv tests/integration/graphql/test_orderby_complex_scenarios.py \
       tests/integration/graphql/test_orderby_scenarios.py
```

**Verify**: `test -f tests/integration/graphql/test_orderby_scenarios.py && echo "✓" || echo "✗"`

#### 3.5: Where generator test

```bash
git mv tests/integration/repository/test_where_generator_extended.py \
       tests/integration/repository/test_where_generator.py
```

**Verify**: `test -f tests/integration/repository/test_where_generator.py && echo "✓" || echo "✗"`

#### 3.6: N+1 detector test

```bash
git mv tests/integration/performance/test_n_plus_one_detector_extended.py \
       tests/integration/performance/test_n_plus_one_detector.py
```

**Verify**: `test -f tests/integration/performance/test_n_plus_one_detector.py && echo "✓" || echo "✗"`

#### 3.7: Decorators test (from Phase 2 consolidation)

```bash
git mv tests/integration/auth/test_decorators_extended.py \
       tests/integration/auth/test_decorators.py
```

**Verify**: `test -f tests/integration/auth/test_decorators.py && echo "✓" || echo "✗"`

#### 3.8: Validators test (from Phase 2 consolidation)

```bash
git mv tests/integration/auth/test_validators_extended.py \
       tests/integration/auth/test_validators.py
```

**Verify**: `test -f tests/integration/auth/test_validators.py && echo "✓" || echo "✗"`

### Step 4: Rename Files with Other Process Hints (2 files)

#### 4.1: Network filtering test

```bash
git mv tests/integration/operators/test_network_fixes.py \
       tests/integration/operators/test_network_filtering.py
```

**Verify**: `test -f tests/integration/operators/test_network_filtering.py && echo "✓" || echo "✗"`

#### 4.2: Schema validation test (meta)

```bash
git mv tests/integration/meta/test_phase0_validation.py \
       tests/integration/meta/test_schema_validation.py
```

**Verify**: `test -f tests/integration/meta/test_schema_validation.py && echo "✓" || echo "✗"`

### Step 5: Rename Native Error Arrays (from Phase 2)

```bash
git mv tests/integration/graphql/mutations/test_native_error_arrays.py \
       tests/integration/graphql/mutations/test_error_arrays.py
```

**Verify**: `test -f tests/integration/graphql/mutations/test_error_arrays.py && echo "✓" || echo "✗"`

### Step 6: Check for Import References

Some files might import these renamed test files (rare but possible):

```bash
# Search for any imports of old file names
grep -r "test_.*_fix\|test_.*_regression\|test_.*_simple" tests/ --include="*.py" | grep "import"
```

**Expected output**: No imports found (test files rarely import each other)

**If imports found**: Update the import statements to use new file names

### Step 7: Verify All Renames

```bash
# Check old names don't exist anymore
echo "Checking old file names are gone..."
! find tests/integration -name "*_fix.py" && echo "✓ No _fix files" || echo "✗ Found _fix files"
! find tests/integration -name "*_regression.py" && echo "✓ No _regression files" || echo "✗ Found _regression files"
! find tests/integration -name "*_simple.py" && echo "✓ No _simple files" || echo "✗ Found _simple files"
! find tests/integration -name "*_extended.py" && echo "✓ No _extended files" || echo "✗ Found _extended files"
! find tests/integration -name "*_complex.py" && echo "✓ No _complex files" || echo "✗ Found _complex files"
! find tests/integration -name "*_native_*.py" && echo "✓ No _native files" || echo "✗ Found _native files"
! find tests/integration -name "*_fixes.py" && echo "✓ No _fixes files" || echo "✗ Found _fixes files"
! find tests/integration -name "*phase*.py" && echo "✓ No phase files" || echo "✗ Found phase files"
```

**Expected output**: All checks pass with ✓

### Step 8: Run Full Test Suite

```bash
# Run all integration tests to ensure renames didn't break anything
uv run pytest tests/integration/ -v --tb=short
```

**Expected output**: All tests pass

**If tests fail**:
- Should not fail due to renames alone (Python imports by file path)
- If failures occur, likely unrelated to renames
- Check git status to see which files changed

## Verification Commands

```bash
# Verify count of renamed files
echo "Expected: 19 renamed files"
git status | grep renamed | wc -l

# Verify no process hints in file names
find tests/integration -name "*.py" | grep -E "(fix|regression|simple|extended|complex|native|fixes|phase[0-9])" | wc -l
# Should output: 0

# Run tests
uv run pytest tests/integration/ -v
```

**Expected output**:
- 19 renamed files in git status
- 0 files with process hints
- All tests pass

## Acceptance Criteria

- [ ] All 19 files successfully renamed
- [ ] No file names contain: _fix, _regression, _simple, _extended, _complex, _native, _fixes, phase
- [ ] Git status shows 19 renamed files
- [ ] No import errors
- [ ] Full integration test suite passes

## Commit

After verification passes:

```bash
git add tests/integration/
git commit -m "refactor(tests): remove process hints from integration test file names [REFACTOR]

Rename test files to use clean, descriptive names:
- Remove _fix suffix (6 files)
- Remove _regression suffix (3 files)
- Remove _simple/_extended/_complex suffixes (8 files)
- Remove _native and _fixes suffixes (2 files)

All tests passing. No functionality changes."
```

## DO NOT

- ❌ Use `mv` instead of `git mv` (won't track rename history)
- ❌ Rename multiple files in one command (error-prone)
- ❌ Skip verification after each rename
- ❌ Proceed to Phase 4 without running full test suite

## Troubleshooting

**Problem**: `git mv` fails with "source file doesn't exist"
**Solution**:
1. Check if file was already renamed or deleted in Phase 2
2. Verify exact file path with `ls tests/integration/**/*.py | grep <filename>`
3. Update path in command

**Problem**: Tests fail after rename
**Solution**:
1. Check if failure is related to rename (unlikely)
2. Run `git diff` to see if any code changed unexpectedly
3. Check pytest discovery isn't confused (test files must start with `test_`)

**Problem**: Git status shows "deleted" and "untracked" instead of "renamed"
**Solution**: You used `mv` instead of `git mv`. Fix with:
```bash
git add -A  # This will detect the rename
git status  # Should now show "renamed"
```

## Notes for Junior Engineers

**Why use `git mv` instead of `mv`?**
`git mv` preserves file history. When you view the file later, git will show its full history including commits before the rename.

**What if I make a typo in the new name?**
Use `git mv` again to fix it:
```bash
git mv tests/integration/graphql/test_typo.py tests/integration/graphql/test_correct_name.py
```

**Do I need to update anything inside the files?**
Not yet - that's Phase 4. Right now we're only changing file names.

**Why so many verification commands?**
Renaming 19 files is error-prone. Each verification catches mistakes early.

**Time estimate**: ~1 hour
- Renames: ~30 minutes
- Verification: ~15 minutes
- Test run: ~15 minutes
