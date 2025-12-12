# Phase 6: Fix Integration Test Parameter Issues

**Phase**: FIX (Update test code)
**Duration**: 45 minutes
**Risk**: Low (test-only changes)
**Status**: Ready for Execution

---

## Objective

Fix integration tests that use wrong parameter order or parameter names when calling `build_sql()`.

**Success**: All 159 integration tests pass.

---

## Prerequisites

- [ ] Phases 1-5 completed
- [ ] ~142/159 tests passing (casting fixed)
- [ ] Remaining 17 failures are parameter issues

---

## Issues to Fix

### Issue 1: Wrong Parameter Order (10 tests)

**OLD signature** (before refactor):
```python
build_sql(path_sql, operator, value, field_type)
```

**NEW signature** (current):
```python
build_sql(operator, value, path_sql, field_type=None)
```

### Issue 2: Wrong Parameter Names (7 tests)

Tests using:
- `op="..."` instead of `operator="..."`
- `val="..."` instead of `value="..."`

---

## Files to Fix

### Network Tests (8 files)

**File**: `tests/integration/database/sql/where/network/test_ip_operations.py`

**Find** (around line 371-373):
```python
result = strategy.build_sql(
    SQL("data->>'ip_address'"),  # WRONG position
    "inSubnet",
    "192.168.1.0/24",
    IpAddress
)
```

**Replace with**:
```python
result = strategy.build_sql(
    "inSubnet",                   # operator first
    "192.168.1.0/24",            # value second
    SQL("data->>'ip_address'"),  # path_sql third
    field_type=IpAddress         # named parameter
)
```

**Apply same fix to**:
- `test_consistency.py` (2 instances)
- `test_ip_filtering.py` (1 instance)
- `test_jsonb_integration.py` (2 instances)
- `test_network_fixes.py` (5 instances)
- `test_production_bugs.py` (7 instances)

**Total**: ~17 fixes in network tests

### DateRange Tests (1 file)

**File**: `tests/integration/database/sql/where/temporal/test_daterange_operations.py`

**Find** (around line 37-42):
```python
sql = registry.build_sql(
    path_sql=path_sql,
    op="overlaps",           # WRONG: should be operator=
    val="[2023-06-01,2023-06-30]",  # WRONG: should be value=
    field_type=DateRangeField,
)
```

**Replace with**:
```python
sql = registry.build_sql(
    operator="overlaps",     # CORRECT
    value="[2023-06-01,2023-06-30]",  # CORRECT
    path_sql=path_sql,
    field_type=DateRangeField,
)
```

**Apply to ~10 test methods** in this file.

---

## Implementation Strategy

### Step 1: Create Search & Replace Script

```bash
# Create helper script
cat > /tmp/fix_param_order.sh << 'EOF'
#!/bin/bash
# Fix parameter order in integration tests

cd /home/lionel/code/fraiseql

# Fix network tests - strategy.build_sql() calls
echo "Fixing network test parameter order..."

# Manual review and fix each file
# (Too complex for automated sed - need manual code review)

echo "Please manually fix these files:"
echo "  tests/integration/database/sql/where/network/test_ip_operations.py"
echo "  tests/integration/database/sql/where/network/test_consistency.py"
echo "  tests/integration/database/sql/where/network/test_ip_filtering.py"
echo "  tests/integration/database/sql/where/network/test_jsonb_integration.py"
echo "  tests/integration/database/sql/where/network/test_network_fixes.py"
echo "  tests/integration/database/sql/where/network/test_production_bugs.py"
echo ""
echo "Pattern to fix:"
echo "  OLD: build_sql(path_sql, operator, value, field_type)"
echo "  NEW: build_sql(operator, value, path_sql, field_type=field_type)"
EOF

chmod +x /tmp/fix_param_order.sh
```

### Step 2: Fix Network Tests Manually

For each file, use editor to find and fix:

**Search for**: `build_sql(`
**Check**: Is path_sql first parameter?
**Fix**: Reorder to (operator, value, path_sql, field_type=...)

**Recommended**: Use IDE with "Find in Files" to locate all instances.

### Step 3: Fix DateRange Tests

**File**: `tests/integration/database/sql/where/temporal/test_daterange_operations.py`

**Search for**: `op=`
**Replace with**: `operator=`

**Search for**: `val=`
**Replace with**: `value=`

```bash
# Can use sed for this one
sed -i 's/op="/operator="/g' tests/integration/database/sql/where/temporal/test_daterange_operations.py
sed -i 's/val="/value="/g' tests/integration/database/sql/where/temporal/test_daterange_operations.py
```

---

## Verification

### After Each File

```bash
# Run tests for the file you just fixed
uv run pytest tests/integration/database/sql/where/network/test_ip_operations.py -v
```

### Final Verification

```bash
# Run ALL WHERE integration tests
uv run pytest tests/integration/database/sql/where/ -v

# Expected: 159/159 passing ✅
```

---

## Detailed Fix List

### test_ip_operations.py

**Line ~371**:
```python
# BEFORE:
result = strategy.build_sql(
    SQL("data->>'ip_address'"), "inSubnet", "192.168.1.0/24", IpAddress
)

# AFTER:
result = strategy.build_sql(
    "inSubnet", "192.168.1.0/24", SQL("data->>'ip_address'"), field_type=IpAddress
)
```

Similar fixes at lines ~380, ~390 (3 total).

### test_consistency.py

**Line ~TBD**: 2 fixes

### test_ip_filtering.py

**Line ~69**: 1 fix

### test_jsonb_integration.py

**Lines ~TBD**: 2 fixes

### test_network_fixes.py

**Lines ~TBD**: 5 fixes

### test_production_bugs.py

**Lines ~TBD**: 7 fixes

### test_daterange_operations.py

**Lines ~37, 49, 54, 66, 71, etc.**: ~10 fixes (parameter names)

---

## Acceptance Criteria

- [ ] All network test files updated (parameter order)
- [ ] DateRange test file updated (parameter names)
- [ ] All 159 integration tests pass
- [ ] No unit test regressions
- [ ] Clean git diff showing only test updates

---

## Commit

```bash
# Stage all test changes
git add tests/integration/database/sql/where/

# Verify what's being committed
git diff --cached --stat

# Commit
git commit -m "fix(tests): Update integration tests to use correct build_sql signature

Update all integration tests to use correct parameter order and names:
- Reorder parameters: (operator, value, path_sql, field_type=...)
- Fix parameter names: op → operator, val → value

Affected files:
- tests/integration/database/sql/where/network/*.py (6 files, 17 fixes)
- tests/integration/database/sql/where/temporal/test_daterange_operations.py (10 fixes)

Result: All 159 integration tests now pass ✅

Phase: 6/7 (Fix Integration Test Parameters)
Fixes: 17 remaining test failures"
```

---

## Progress After Phase 6

| Metric | Value |
|--------|-------|
| Integration tests passing | 159/159 (100%) ✅ |
| Unit tests passing | 550+/550+ (100%) ✅ |
| Casting bugs fixed | 18/18 (100%) ✅ |
| Parameter issues fixed | 17/17 (100%) ✅ |

---

**Next Phase**: Phase 7 - Verification & Cleanup
**Final Step**: Document, verify, and close out the implementation
