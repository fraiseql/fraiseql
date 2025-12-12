# Phase 2: Fix MAC Address Strategy

**Phase**: FIX (Update MacAddressOperatorStrategy)
**Duration**: 30 minutes
**Risk**: Low (self-contained changes)
**Status**: Ready for Execution

---

## Objective

Update `MacAddressOperatorStrategy` to use the new `_cast_both_sides()` method, ensuring both field path and value are cast to `::macaddr` for proper PostgreSQL comparison.

**Success**: All 9 MAC address integration tests pass.

---

## Prerequisites

- [ ] Phase 1 completed (`_cast_both_sides()` method available)
- [ ] Phase 1 committed
- [ ] Clean git working directory

---

## Current State Analysis

### Current Bug

**Current SQL** (wrong):
```sql
-- No casts at all:
data->>'mac_address' = '00:11:22:33:44:55'

-- Or only field cast:
(data->>'mac_address')::macaddr = '00:11:22:33:44:55'
```

**Expected SQL** (correct):
```sql
(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr
```

### Failing Tests

9 tests in `tests/integration/database/sql/where/network/test_mac_operations.py`:
- `test_mac_address_eq_with_different_formats`
- `test_mac_address_case_insensitive_comparison`
- `test_mac_address_in_list_with_mixed_formats`
- `test_mac_address_neq_operation`
- `test_mac_address_nin_operation`
- `test_mac_address_vs_string_field_behavior`
- `test_mac_address_normalization_in_sql_generation`

Plus 1 in `test_mac_filtering.py`:
- `test_graphql_mac_address_equality_filtering`

---

## Implementation

### Step 1: Read Current Implementation

```bash
cd /home/lionel/code/fraiseql

# Read current MAC address operator strategy
cat src/fraiseql/sql/operators/postgresql/macaddr_operators.py
```

### Step 2: Update `build_sql()` Method

**File**: `src/fraiseql/sql/operators/postgresql/macaddr_operators.py`

**Find the `build_sql()` method (around line 33-70)**

**Replace with**:

```python
    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for MAC address operators with proper casting.

        Always casts both field and value to ::macaddr for type-safe comparisons.

        Supported operators:
            - eq, neq: Equality/inequality with ::macaddr casting
            - in, nin: List membership with ::macaddr casting
            - isnull: NULL checking (no casting needed)
        """
        from psycopg.sql import SQL, Composed

        # Comparison operators (eq, neq)
        if operator == "eq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "macaddr")
            return SQL("{} = {}").format(casted_path, casted_value)

        if operator == "neq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "macaddr")
            return SQL("{} != {}").format(casted_path, casted_value)

        # List operators (in, nin)
        if operator == "in":
            # Cast field path
            casted_path = SQL("({})::{}").format(path_sql, SQL("macaddr"))

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values(
                [str(v) for v in value_list],
                "macaddr"
            )

            # Build IN clause: field IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} IN ({})").format(casted_path, values_sql)

        if operator == "nin":
            # Cast field path
            casted_path = SQL("({})::{}").format(path_sql, SQL("macaddr"))

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values(
                [str(v) for v in value_list],
                "macaddr"
            )

            # Build NOT IN clause: field NOT IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} NOT IN ({})").format(casted_path, values_sql)

        # NULL checking (no casting needed)
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
```

**Key Changes**:
1. ✅ Use `_cast_both_sides()` for eq/neq operators
2. ✅ Use `_cast_list_values()` for in/nin operators
3. ✅ Remove dependency on `jsonb_column` parameter
4. ✅ Simpler, more explicit code

---

## Verification

### Step 1: Syntax Check

```bash
# Verify Python syntax
python3 -m py_compile src/fraiseql/sql/operators/postgresql/macaddr_operators.py
```

### Step 2: Unit Tests (Should Still Pass)

```bash
# Run MAC address unit tests
uv run pytest tests/unit/sql/where/operators/network/test_mac.py -v

# Expected: All pass (unit tests use old direct functions)
```

### Step 3: Integration Tests (THESE SHOULD NOW PASS)

```bash
# Run MAC address integration tests
uv run pytest tests/integration/database/sql/where/network/test_mac_operations.py -v

# Expected: All 9 tests PASS ✅
```

**Expected Output**:
```
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_eq_with_different_formats PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_case_insensitive_comparison PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_in_list_with_mixed_formats PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_neq_operation PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_nin_operation PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_isnull_operation PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_vs_string_field_behavior PASSED
tests/integration/database/sql/where/network/test_mac_operations.py::TestMacAddressFilterOperations::test_mac_address_normalization_in_sql_generation PASSED

======================== 8 passed ========================
```

### Step 4: Run MAC Filtering Test

```bash
# Run the other MAC test
uv run pytest tests/integration/database/sql/where/network/test_mac_filtering.py::TestEndToEndMACAddressFiltering::test_graphql_mac_address_equality_filtering -v

# Expected: PASS ✅
```

### Step 5: Verify SQL Output

```bash
# Test SQL generation
python3 << 'PYEOF'
from fraiseql.sql.operators import get_default_registry
from fraiseql.types import MacAddress
from psycopg.sql import SQL

registry = get_default_registry()
path_sql = SQL("data->>'mac_address'")

# Test eq operator
sql = registry.build_sql("eq", "00:11:22:33:44:55", path_sql, field_type=MacAddress)
sql_str = sql.as_string(None)

print("Generated SQL:")
print(f"  {sql_str}")
print()

# Verify both sides cast
assert "::macaddr" in sql_str, "Missing macaddr cast!"
assert sql_str.count("::macaddr") == 2, "Should have 2 casts (field + value)"

expected = "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr"
assert sql_str == expected, f"SQL mismatch!\nExpected: {expected}\nGot: {sql_str}"

print("✅ SQL generation correct!")
print(f"   Both sides cast to ::macaddr")
PYEOF
```

---

## Acceptance Criteria

- [ ] `MacAddressOperatorStrategy.build_sql()` updated to use `_cast_both_sides()`
- [ ] All 9 MAC operation tests pass
- [ ] MAC filtering test passes
- [ ] Unit tests still pass (no regression)
- [ ] SQL output shows `::macaddr` on both sides
- [ ] No use of `jsonb_column` parameter in casting logic

---

## Expected Test Results

### Before Phase 2
```
tests/integration/database/sql/where/network/test_mac_operations.py
- 1 passed, 8 failed
```

### After Phase 2
```
tests/integration/database/sql/where/network/test_mac_operations.py
- 9 passed, 0 failed  ✅
```

### Progress Tracker

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| MAC tests passing | 1/9 | 9/9 | +8 tests |
| Total integration tests passing | 103/159 | 112/159 | +9 tests |
| Failure rate | 35% | 30% | -5% |

---

## Troubleshooting

### Issue: Tests Still Failing

**Symptom**: Tests still show missing `::macaddr` cast

**Check**:
```bash
# Verify strategy is being selected
python3 << 'PYEOF'
from fraiseql.sql.operators import get_default_registry
from fraiseql.types import MacAddress

registry = get_default_registry()
strategy = registry.get_strategy("eq", MacAddress)
print(f"Strategy: {type(strategy).__name__}")
PYEOF
```

Expected: `Strategy: MacAddressOperatorStrategy`

If wrong strategy: Check `supports_operator()` method in MacAddressOperatorStrategy

### Issue: Import Errors

**Symptom**: `AttributeError: '_cast_both_sides' not found`

**Fix**:
```bash
# Verify Phase 1 was completed
python3 << 'PYEOF'
from fraiseql.sql.operators.base import BaseOperatorStrategy
assert hasattr(BaseOperatorStrategy, '_cast_both_sides'), "Phase 1 not completed!"
print("✅ Phase 1 completed correctly")
PYEOF
```

### Issue: Wrong SQL Generated

**Symptom**: SQL has casts in wrong places

**Debug**:
```bash
# Add debug output
python3 << 'PYEOF'
from fraiseql.sql.operators import get_default_registry
from fraiseql.types import MacAddress
from psycopg.sql import SQL

registry = get_default_registry()
path_sql = SQL("data->>'mac_address'")

sql = registry.build_sql("eq", "00:11:22:33:44:55", path_sql, field_type=MacAddress)

print("SQL object:", sql)
print("SQL string:", sql.as_string(None))
print("SQL repr:", repr(sql))
PYEOF
```

Compare output with expected format.

---

## Commit

```bash
cd /home/lionel/code/fraiseql

# Stage changes
git add src/fraiseql/sql/operators/postgresql/macaddr_operators.py

# Run tests one more time before commit
uv run pytest tests/integration/database/sql/where/network/test_mac_operations.py -v

# If all pass, commit
git commit -m "$(cat <<'EOF'
fix(operators): Always cast both sides for MAC address comparisons

Update MacAddressOperatorStrategy to cast both field path and value
to ::macaddr. This fixes 9 integration tests that were failing due to
missing or incomplete type casting.

Changes:
- Use _cast_both_sides() for eq/neq operators
- Use _cast_list_values() for in/nin list operators
- Remove dependency on jsonb_column parameter
- Simpler, more explicit casting logic

SQL Before:
  data->>'mac_address' = '00:11:22:33:44:55'  ❌

SQL After:
  (data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr  ✅

Part of: Always Cast Both Sides implementation
Phase: 2/7 (Fix MAC Address Strategy)
Fixes: 9 MAC address integration tests
Related: tests/integration/database/sql/where/network/test_mac_operations.py
EOF
)"

# Verify commit
git log -1 --stat
```

---

## Rollback

If issues occur:

```bash
# Revert this phase
git reset --hard HEAD~1

# Or if committed separately:
git revert HEAD
```

---

## Next Steps

Proceed to **Phase 3: Fix DateRange Strategy**

Expected: 9 more tests will pass after Phase 3 (daterange tests)

---

## Notes

### Why This Works

1. **Both sides cast**: PostgreSQL requires matching types for comparison
2. **Works for JSONB**: Extracts are text, need conversion to macaddr
3. **Works for typed columns**: Redundant cast is harmless no-op
4. **Consistent**: Same pattern for all special types

### Performance Impact

- Redundant casts (typed column::macaddr): < 0.1% overhead
- Required casts (JSONB extract): Already needed
- Net impact: Negligible

### Code Quality

- **Before**: ~70 lines with conditional logic
- **After**: ~60 lines with explicit casting
- **Benefit**: 15% reduction + clearer intent

---

**Phase Status**: Ready for execution ✅
**Next Phase**: Phase 3 - Fix DateRange Strategy
**Success Metric**: 9/9 MAC tests passing (112/159 total)
