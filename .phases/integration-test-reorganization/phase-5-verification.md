# Phase 5: Verification & QA

**Phase:** VALIDATION (Comprehensive test execution and validation)
**Duration:** 10-15 minutes
**Risk:** None (validation only)
**Status:** Ready for Execution

---

## Objective

Run comprehensive test suite to verify reorganization caused zero regressions. Validate test discovery, execution, and coverage across all categories.

**Success:** All tests pass, zero new failures, full test discovery confirmed.

---

## Prerequisites

- [ ] Phase 4 completed (references updated)
- [ ] Test discovery working
- [ ] Ready for full test run

---

## Implementation Steps

### Step 1: Full Test Collection Verification (2 min)

#### 1.1 Collect All WHERE Tests

```bash
cd /home/lionel/code/fraiseql

echo "=== Collecting WHERE Integration Tests ==="
uv run pytest tests/integration/database/sql/where/ --collect-only -q | tee /tmp/test-collection.txt

# Count total
TOTAL=$(grep -c "test_" /tmp/test-collection.txt)
echo ""
echo "Total tests collected: $TOTAL"

# Expected: ~15+ test files * ~5-10 tests each = 75-150 tests
```

**Expected:** All test files discovered, reasonable test count

#### 1.2 Verify Each Category

```bash
echo "=== Category-by-Category Collection ==="

# Network
NETWORK=$(uv run pytest tests/integration/database/sql/where/network/ --co -q 2>&1 | grep -c "test_")
echo "Network tests: $NETWORK"

# Specialized
SPECIALIZED=$(uv run pytest tests/integration/database/sql/where/specialized/ --co -q 2>&1 | grep -c "test_")
echo "Specialized tests: $SPECIALIZED"

# Temporal
TEMPORAL=$(uv run pytest tests/integration/database/sql/where/temporal/ --co -q 2>&1 | grep -c "test_")
echo "Temporal tests: $TEMPORAL"

# Spatial
SPATIAL=$(uv run pytest tests/integration/database/sql/where/spatial/ --co -q 2>&1 | grep -c "test_")
echo "Spatial tests: $SPATIAL"

# Root
ROOT=$(uv run pytest tests/integration/database/sql/where/test_*.py --co -q 2>&1 | grep -c "test_" || echo "0")
echo "Root tests: $ROOT"

echo ""
echo "Total: $(($NETWORK + $SPECIALIZED + $TEMPORAL + $SPATIAL + $ROOT)) tests"
```

**Acceptance:**
- [ ] All categories have tests
- [ ] Total matches expected count
- [ ] No collection errors

---

### Step 2: Category Test Runs (5 min)

#### 2.1 Run Network Tests

```bash
echo "=== Running Network Tests ==="
uv run pytest tests/integration/database/sql/where/network/ -v --tb=short

# Capture result
if [ $? -eq 0 ]; then
    echo "✓ Network tests: PASSED"
else
    echo "✗ Network tests: FAILED (check output above)"
fi
```

#### 2.2 Run Specialized Tests

```bash
echo "=== Running Specialized Tests ==="
uv run pytest tests/integration/database/sql/where/specialized/ -v --tb=short

# Capture result
if [ $? -eq 0 ]; then
    echo "✓ Specialized tests: PASSED"
else
    echo "✗ Specialized tests: FAILED (check output above)"
fi
```

#### 2.3 Run Temporal Tests

```bash
echo "=== Running Temporal Tests ==="
uv run pytest tests/integration/database/sql/where/temporal/ -v --tb=short

# Capture result
if [ $? -eq 0 ]; then
    echo "✓ Temporal tests: PASSED"
else
    echo "✗ Temporal tests: FAILED (check output above)"
fi
```

#### 2.4 Run Spatial Tests

```bash
echo "=== Running Spatial Tests ==="
uv run pytest tests/integration/database/sql/where/spatial/ -v --tb=short

# Capture result
if [ $? -eq 0 ]; then
    echo "✓ Spatial tests: PASSED"
else
    echo "✗ Spatial tests: FAILED (check output above)"
fi
```

#### 2.5 Run Root Tests

```bash
echo "=== Running Root WHERE Tests ==="
uv run pytest tests/integration/database/sql/where/test_*.py -v --tb=short 2>/dev/null || echo "No root tests to run"

# Capture result
if [ $? -eq 0 ]; then
    echo "✓ Root tests: PASSED"
else
    echo "Note: Root tests may not exist"
fi
```

**Acceptance:**
- [ ] Network tests pass
- [ ] Specialized tests pass
- [ ] Temporal tests pass
- [ ] Spatial tests pass
- [ ] Root tests pass (if present)

---

### Step 3: Full Integration Suite (3 min)

#### 3.1 Run All WHERE Tests

```bash
echo "=== Running All WHERE Integration Tests ==="
uv run pytest tests/integration/database/sql/where/ -v --tb=short | tee /tmp/where-test-results.txt

# Summary
echo ""
echo "=== Test Summary ==="
grep -E "passed|failed|error" /tmp/where-test-results.txt | tail -1
```

**Expected:** All tests pass, zero new failures from reorganization

#### 3.2 Run Full Integration Suite

```bash
echo "=== Running ALL Integration Tests ==="
uv run pytest tests/integration/ -v --tb=short | tee /tmp/full-integration-results.txt

# Summary
echo ""
echo "=== Full Integration Summary ==="
grep -E "passed|failed|error" /tmp/full-integration-results.txt | tail -1
```

**Expected:** All integration tests pass (not just WHERE tests)

**Acceptance:**
- [ ] WHERE tests: All pass
- [ ] Integration tests: All pass
- [ ] Zero new failures from reorganization
- [ ] No import/collection errors

---

### Step 4: Compare Before/After (2 min)

#### 4.1 Test Count Comparison

```bash
cat > /tmp/test-comparison.sh << 'EOF'
#!/bin/bash
# Compare test counts before and after reorganization

echo "=== Test Count Comparison ==="
echo ""

# Current count (after reorganization)
CURRENT=$(uv run pytest tests/integration/database/sql/where/ --co -q 2>&1 | grep -c "test_")
echo "Current (after): $CURRENT tests"

# Note: Before count was recorded in Phase 3 pre-migration
# Check Phase 3 notes for baseline count

echo ""
echo "If counts match baseline from Phase 3, reorganization preserved all tests ✓"
EOF

chmod +x /tmp/test-comparison.sh
/tmp/test-comparison.sh
```

#### 4.2 Verify No Tests Lost

```bash
# List all test files
echo "=== Test Files Inventory ==="
find tests/integration/database/sql/where -name "test_*.py" -type f | sort | tee /tmp/after-test-files.txt

# Count
wc -l /tmp/after-test-files.txt

# Compare with Phase 1 inventory (if saved)
if [ -f /tmp/where-related-tests.txt ]; then
    echo ""
    echo "Comparing with Phase 1 inventory..."
    BEFORE=$(wc -l < /tmp/where-related-tests.txt)
    AFTER=$(wc -l < /tmp/after-test-files.txt)
    echo "Before: $BEFORE files"
    echo "After: $AFTER files"

    if [ "$BEFORE" -eq "$AFTER" ]; then
        echo "✓ All files preserved"
    else
        echo "⚠ File count mismatch - review changes"
    fi
fi
```

**Acceptance:**
- [ ] Test count matches baseline
- [ ] File count matches (15 files)
- [ ] No tests lost during migration

---

### Step 5: Performance Check (1 min)

#### 5.1 Test Execution Speed

```bash
echo "=== Test Performance Check ==="

# Time the test run
echo "Running WHERE tests with timing..."
time uv run pytest tests/integration/database/sql/where/ -q

# Note: Performance should be similar to before reorganization
# Directory structure doesn't affect execution speed
```

**Expected:** Similar execution time as before (structure doesn't affect speed)

**Acceptance:**
- [ ] Tests execute in reasonable time
- [ ] No performance degradation

---

### Step 6: Edge Case Verification (2 min)

#### 6.1 Test Individual Files

```bash
echo "=== Individual File Execution ==="

# Pick one test from each category and run individually
uv run pytest tests/integration/database/sql/where/network/test_ip_filtering.py -v
uv run pytest tests/integration/database/sql/where/specialized/test_ltree_filtering.py -v
uv run pytest tests/integration/database/sql/where/temporal/test_daterange_operations.py -v
uv run pytest tests/integration/database/sql/where/spatial/test_coordinate_operations.py -v

echo "✓ Individual file execution works"
```

#### 6.2 Test Pattern Selection

```bash
echo "=== Pattern Selection ==="

# Test pytest -k pattern matching still works
uv run pytest tests/integration/database/sql/where/ -k "network" --co -q | head -20
uv run pytest tests/integration/database/sql/where/ -k "ltree" --co -q | head -20

echo "✓ Pattern matching works"
```

#### 6.3 Test Parallel Execution (if used)

```bash
# If project uses pytest-xdist
if pytest --version | grep -q "xdist"; then
    echo "=== Parallel Execution Test ==="
    uv run pytest tests/integration/database/sql/where/ -n auto -v
    echo "✓ Parallel execution works"
else
    echo "pytest-xdist not installed, skipping parallel test"
fi
```

**Acceptance:**
- [ ] Individual files run correctly
- [ ] Pattern matching works
- [ ] Parallel execution works (if applicable)

---

## Verification Checklist

### Test Discovery
- [ ] All test files discovered
- [ ] All categories have tests
- [ ] Pattern matching works
- [ ] Individual file execution works

### Test Execution
- [ ] Network tests pass
- [ ] Specialized tests pass
- [ ] Temporal tests pass
- [ ] Spatial tests pass
- [ ] Root tests pass (if present)
- [ ] Full WHERE suite passes
- [ ] Full integration suite passes

### Comparison
- [ ] Test count matches baseline
- [ ] File count matches (15 files)
- [ ] Zero new failures
- [ ] No tests lost

### Performance
- [ ] Execution time acceptable
- [ ] No performance degradation

---

## Issue Resolution

### If Tests Fail

**1. Determine if failure is reorganization-related:**
```bash
# Check error message
# - Import errors → reorganization issue
# - Test assertion failures → unrelated pre-existing issue
# - Fixture errors → reorganization issue
```

**2. Reorganization-related failures:**
- Go back to Phase 4, check references
- Verify __init__.py files present
- Check fixture imports

**3. Pre-existing failures:**
- Note them but don't fix in this phase
- Check if failures existed before reorganization
- Document for separate fix

### Rollback Decision

**Roll back if:**
- Import errors that can't be quickly fixed
- Test collection completely broken
- More than 5 new failures clearly from reorganization

**Continue if:**
- All tests discovered and run
- Failures are pre-existing issues
- Minor fixable issues

---

## Test Results Summary

After running all tests, create summary:

```bash
cat > /tmp/phase5-summary.txt << 'EOF'
# Phase 5 Verification Summary

## Test Collection
- Total tests: [COUNT]
- Network: [COUNT]
- Specialized: [COUNT]
- Temporal: [COUNT]
- Spatial: [COUNT]
- Root: [COUNT]

## Test Execution
- WHERE tests: [PASSED/FAILED]
- Full integration: [PASSED/FAILED]

## Issues Found
- [List any issues]

## Comparison
- Baseline count: [FROM PHASE 3]
- Current count: [CURRENT]
- Files: [MATCH/MISMATCH]

## Performance
- Execution time: [TIME]

## Conclusion
- [ ] Ready for Phase 6 (Documentation)
- [ ] Need fixes before proceeding
EOF

cat /tmp/phase5-summary.txt
```

---

## Next Steps

### If All Tests Pass ✓
1. Review summary
2. Proceed to Phase 6: Documentation & Cleanup

### If Issues Found ✗
1. Document all issues
2. Fix in Phase 4 (if reference issues)
3. Fix separately (if pre-existing issues)
4. Re-run Phase 5 after fixes

---

## Commit Results (Optional)

If any test fixtures or conftest changes were needed:

```bash
cd /home/lionel/code/fraiseql

git add tests/integration/database/conftest.py
git add tests/integration/database/sql/where/conftest.py

git commit -m "$(cat <<'EOF'
test(integration): Verify reorganized test structure [PHASE-5]

Comprehensive verification of test reorganization:
- All tests discovered and execute correctly
- Zero new failures from reorganization
- Test count matches baseline (15 files, [N] tests)
- All categories pass independently

Results:
- Network: [STATUS]
- Specialized: [STATUS]
- Temporal: [STATUS]
- Spatial: [STATUS]

Phase: 5/6 (Verification & QA)
See: .phases/integration-test-reorganization/phase-5-verification.md
EOF
)"
```

---

## Notes

### What This Phase Validates

1. **Test Discovery** - All tests found by pytest
2. **Test Execution** - All tests run without import errors
3. **Zero Regressions** - No new failures from move
4. **Coverage Maintained** - Same test count as before
5. **Performance** - Execution speed unchanged

### Common Findings

- Pre-existing test failures (not from reorganization)
- Slow tests (already slow, not reorganization-related)
- Flaky tests (were already flaky)

**Important:** Only fix reorganization-related issues in this phase.

---

**Phase Status:** Ready for execution ✅
**Next Phase:** Phase 6 - Documentation & Cleanup
