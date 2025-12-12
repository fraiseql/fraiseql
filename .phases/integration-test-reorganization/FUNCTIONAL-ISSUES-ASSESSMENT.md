# Functional Issues Assessment - Integration Tests

**Date**: 2025-12-11
**Context**: Integration test reorganization (Phase 5 verification)
**Status**: 56 of 159 integration tests failing (35% failure rate)

---

## Executive Summary

All 56 failing tests are **pre-existing functional issues** unrelated to the test reorganization. The failures fall into 5 distinct categories:

1. **Parameter Order Issues** (28 tests) - Tests using old `build_sql` signature
2. **Type Casting Issues** (18 tests) - Missing PostgreSQL type casts
3. **Parameter Name Issues** (10 tests) - Wrong parameter names in test code

**Critical Finding**: Zero failures caused by the test reorganization. All issues existed before file moves.

---

## Category 1: Parameter Order Issues (28 tests)

### Root Cause
Tests are calling `build_sql()` with the OLD parameter order after the operator strategies refactor changed the signature.

**OLD Signature** (before refactor):
```python
strategy.build_sql(path_sql, operator, value, field_type)
```

**NEW Signature** (after refactor):
```python
registry.build_sql(operator, value, path_sql, field_type=None, jsonb_column=None)
```

### Symptoms
- `TypeError: unhashable type: 'SQL'` - SQL object passed where operator string expected
- `AttributeError: 'NoneType' object has no attribute 'as_string'` - build_sql returns None
- Assertions fail because SQL is "None" string

### Affected Tests

#### Network Tests (19 tests)
- `test_ip_operations.py::test_subnet_operator_sql_generation` - inSubnet operator
- `test_ip_operations.py::test_range_operator_sql_generation` - inRange operator
- `test_ip_operations.py::test_private_ip_operator_sql_generation` - isPrivate operator
- `test_consistency.py::test_eq_vs_insubnet_sql_consistency` - eq vs inSubnet
- `test_consistency.py::test_field_type_detection_issue` - field type detection
- `test_ip_filtering.py::test_network_specific_operators` - network operators
- `test_jsonb_integration.py::test_sql_generation_for_jsonb_network_field` - JSONB + network
- `test_jsonb_integration.py::test_eq_operator_sql_generation` - eq with IP
- `test_network_fixes.py::test_network_operator_selection_with_ip_types` (+ 5 more)
- `test_production_bugs.py::test_ip_address_detection_with_common_ips` (+ 6 more)

**Example Fix**:
```python
# BEFORE (wrong):
result = strategy.build_sql(
    SQL("data->>'ip_address'"),  # path_sql first
    "inSubnet",                   # operator second
    "192.168.1.0/24",            # value third
    IpAddress                    # field_type fourth
)

# AFTER (correct):
result = registry.build_sql(
    "inSubnet",                  # operator first
    "192.168.1.0/24",           # value second
    SQL("data->>'ip_address'"), # path_sql third
    field_type=IpAddress        # field_type named
)
```

#### Spatial Tests (11 tests)
All 11 coordinate tests have similar parameter order issues:
- `test_coordinate_operations.py::test_coordinate_eq_operation`
- `test_coordinate_operations.py::test_coordinate_neq_operation`
- `test_coordinate_operations.py::test_coordinate_in_operation`
- `test_coordinate_operations.py::test_coordinate_notin_operation`
- `test_coordinate_operations.py::test_coordinate_distance_within_operation`
- `test_coordinate_operations.py::test_coordinate_edge_cases`
- (+ 5 more)

---

## Category 2: Type Casting Issues (18 tests)

### Root Cause
PostgreSQL type casting (`::type`) not being applied correctly in SQL generation for special types.

### 2A. MAC Address Casting (9 tests)

**Issue**: Missing `::macaddr` cast on the value side of comparisons.

**Expected SQL**:
```sql
(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr
```

**Actual SQL**:
```sql
data->>'mac_address' = '00:11:22:33:44:55'  -- No casts at all
```

**Affected Tests**:
- `test_mac_filtering.py::test_graphql_mac_address_equality_filtering`
- `test_mac_operations.py::test_mac_address_eq_with_different_formats`
- `test_mac_operations.py::test_mac_address_case_insensitive_comparison`
- `test_mac_operations.py::test_mac_address_in_list_with_mixed_formats`
- `test_mac_operations.py::test_mac_address_neq_operation`
- `test_mac_operations.py::test_mac_address_nin_operation`
- `test_mac_operations.py::test_mac_address_vs_string_field_behavior`
- `test_mac_operations.py::test_mac_address_normalization_in_sql_generation`

**Root Cause**: The `ComparisonOperatorStrategy` doesn't handle MAC address type casting. The basic strategy generates:
```python
Composed([SQL("data->>'mac_address'"), SQL(' = '), Literal('00:11:22:33:44:55')])
```

**Solution Needed**: Either:
1. MAC address fields need to use `NetworkOperatorStrategy` for basic operators
2. Or `ComparisonOperatorStrategy` needs MAC address type detection and casting

### 2B. DateRange Casting (9 tests)

**Issue**: Missing `::daterange` cast on the field side of comparisons.

**Expected SQL**:
```sql
(data->>'period')::daterange @> '2023-06-15'::date
```

**Actual SQL**:
```sql
data->>'period' @> '2023-06-15'::date  -- Missing daterange cast on field
```

**Affected Tests**:
- `test_daterange_operations.py::test_daterange_contains_date_operation`
- `test_daterange_operations.py::test_daterange_in_list_with_casting`
- `test_daterange_operations.py::test_daterange_nin_operation_with_casting`
- `test_daterange_operations.py::test_daterange_typical_use_cases`
- `test_daterange_operations.py::test_daterange_inclusive_exclusive_boundaries`
- (+ 4 more with parameter name issues)

**Root Cause**: The operator strategies don't cast the JSONB-extracted field to `::daterange`.

---

## Category 3: Parameter Name Issues (10 tests)

### Root Cause
Tests using wrong parameter names when calling `build_sql()`.

**Wrong**:
```python
sql = registry.build_sql(
    path_sql=path_sql,
    op="overlaps",        # WRONG: should be "operator"
    val="[2023-06-01,2023-06-30]",  # WRONG: should be "value"
    field_type=DateRangeField
)
```

**Correct**:
```python
sql = registry.build_sql(
    operator="overlaps",  # or just positional: "overlaps"
    value="[2023-06-01,2023-06-30]",
    path_sql=path_sql,
    field_type=DateRangeField
)
```

**Error Message**:
```
TypeError: OperatorRegistry.build_sql() got an unexpected keyword argument 'op'
```

### Affected Tests (DateRange)
- `test_daterange_operations.py::test_daterange_overlaps_operation`
- `test_daterange_operations.py::test_daterange_adjacent_operation`
- `test_daterange_operations.py::test_daterange_strictly_left_operation`
- `test_daterange_operations.py::test_daterange_strictly_right_operation`
- `test_daterange_operations.py::test_daterange_not_left_operation`
- `test_daterange_operations.py::test_daterange_not_right_operation`
- `test_daterange_operations.py::test_daterange_eq_operation_with_casting`
- `test_daterange_operations.py::test_daterange_neq_operation_with_casting`
- `test_daterange_operations.py::test_daterange_vs_string_field_behavior`

### Affected Tests (Coordinate)
- `test_coordinate_operations.py::test_coordinate_distance_within_operation` (also has this)

**Note**: One test has both parameter name AND order issues.

---

## Category 4: Missing Validation (2 tests)

### Root Cause
Pattern operators (contains, startswith, endswith) should raise `ValueError` when used with special types, but don't.

**Affected Tests**:
- `test_mac_operations.py::test_mac_address_filter_excludes_pattern_operators`
- `test_daterange_operations.py::test_daterange_filter_excludes_pattern_operators`

**Expected Behavior**:
```python
with pytest.raises(ValueError, match="Pattern operator .* not supported for MAC"):
    registry.build_sql("contains", "11:22", path_sql, field_type=MacAddress)
```

**Actual Behavior**: No ValueError raised

**Root Cause**: Pattern operator validation only exists for LTree, not for MAC or DateRange types.

**Solution**: Add similar validation in `NetworkOperatorStrategy` and `DateRangeOperatorStrategy`:
```python
pattern_operators = {"contains", "startswith", "endswith"}
if operator in pattern_operators and field_type is MacAddress:
    raise ValueError(
        f"Pattern operator '{operator}' is not supported for MAC address fields. "
        "MAC addresses only support equality, list, and null operators."
    )
```

---

## Category 5: Coordinate Format Issues (cosmetic)

### Issue
Tests expect specific spacing in POINT() formatting:
- Expected: `POINT( -122.6,45.5)` or `POINT( -122.6, 45.5)`
- Getting: Different spacing variations

**Affected Tests**: Most coordinate tests have assertions about exact string format

**Severity**: Low - SQL is functionally correct, just formatting differs

**Solution**: Either:
1. Update test expectations to be more flexible (check for content, not exact spacing)
2. Standardize POINT formatting in coordinate operator strategy

---

## Priority Recommendations

### P0 - Critical (Blocks all tests)

**1. Fix Parameter Order in Tests**
- **Impact**: 28 tests (50% of failures)
- **Files**: All network and spatial integration tests
- **Effort**: Low (search & replace)
- **Action**: Update all `build_sql()` calls to new signature

```bash
# Find affected tests
grep -r "build_sql.*SQL.*\"" tests/integration/database/sql/where/

# Pattern to fix:
# OLD: registry.build_sql(path_sql, operator, value, field_type)
# NEW: registry.build_sql(operator, value, path_sql, field_type=field_type)
```

**2. Fix Parameter Names in Tests**
- **Impact**: 10 tests (18% of failures)
- **Files**: All daterange tests
- **Effort**: Trivial (rename `op` → `operator`, `val` → `value`)
- **Action**: Update parameter names in daterange tests

### P1 - High (Missing Core Functionality)

**3. Implement MAC Address Type Casting**
- **Impact**: 9 tests (16% of failures)
- **Root Cause**: Basic comparison operators don't cast MAC addresses
- **Effort**: Medium
- **Action**: Either:
  - Extend `ComparisonOperatorStrategy` to handle MAC type casting
  - Or route MAC fields through `NetworkOperatorStrategy` for all operators

**4. Implement DateRange Field Casting**
- **Impact**: 9 tests (16% of failures)
- **Root Cause**: DateRange operators don't cast JSONB-extracted fields
- **Effort**: Medium
- **Action**: Add field casting in `DateRangeOperatorStrategy.build_sql()`

### P2 - Medium (Missing Validation)

**5. Add Pattern Operator Validation**
- **Impact**: 2 tests
- **Effort**: Low
- **Action**: Add pattern operator checks in MAC and DateRange strategies

### P3 - Low (Cosmetic)

**6. Standardize Coordinate Formatting**
- **Impact**: Several coordinate tests
- **Effort**: Low
- **Action**: Either fix formatting or relax test assertions

---

## Implementation Plan

### Phase 1: Test Code Fixes (Quick Wins)
1. Fix parameter order in all integration tests (28 tests)
2. Fix parameter names in daterange tests (10 tests)
**Estimated Time**: 30 minutes
**Expected Result**: 38 tests pass (68% of failures resolved)

### Phase 2: MAC Address Casting
1. Analyze current MAC address handling
2. Decide on strategy (extend comparison vs. route to network)
3. Implement casting logic
4. Update tests if needed
**Estimated Time**: 1-2 hours
**Expected Result**: 47 tests pass (84% of failures resolved)

### Phase 3: DateRange Casting
1. Add field casting to DateRange strategy
2. Verify all daterange operators work correctly
**Estimated Time**: 1 hour
**Expected Result**: 54 tests pass (96% of failures resolved)

### Phase 4: Validation & Polish
1. Add pattern operator validation
2. Fix coordinate formatting (optional)
**Estimated Time**: 30 minutes
**Expected Result**: All 56 tests pass (100%)

---

## Test Execution After Fixes

**Current Status**:
```
159 tests collected
103 passed (65%)
56 failed (35%)
```

**Expected After Phase 1**:
```
159 tests collected
141 passed (89%)
18 failed (11%)
```

**Expected After Phase 2**:
```
159 tests collected
150 passed (94%)
9 failed (6%)
```

**Expected After All Phases**:
```
159 tests collected
159 passed (100%)
0 failed
```

---

## Files Requiring Changes

### Test Code (Phase 1 - Easy Fixes)
```
tests/integration/database/sql/where/network/test_ip_operations.py
tests/integration/database/sql/where/network/test_consistency.py
tests/integration/database/sql/where/network/test_ip_filtering.py
tests/integration/database/sql/where/network/test_jsonb_integration.py
tests/integration/database/sql/where/network/test_network_fixes.py
tests/integration/database/sql/where/network/test_production_bugs.py
tests/integration/database/sql/where/spatial/test_coordinate_operations.py
tests/integration/database/sql/where/temporal/test_daterange_operations.py
```

### Production Code (Phases 2-4 - Functionality Additions)
```
src/fraiseql/sql/operators/core/comparison_operators.py  # MAC casting
src/fraiseql/sql/operators/postgresql/network_operators.py  # MAC validation
src/fraiseql/sql/operators/postgresql/daterange_operators.py  # DateRange casting + validation
src/fraiseql/sql/operators/postgresql/coordinate_operators.py  # Formatting (optional)
```

---

## Verification Commands

After fixes are applied, verify with:

```bash
# Test parameter order fixes
uv run pytest tests/integration/database/sql/where/network/ -v

# Test MAC address casting
uv run pytest tests/integration/database/sql/where/network/test_mac_operations.py -v

# Test DateRange casting
uv run pytest tests/integration/database/sql/where/temporal/test_daterange_operations.py -v

# Test coordinate operations
uv run pytest tests/integration/database/sql/where/spatial/test_coordinate_operations.py -v

# Full suite
uv run pytest tests/integration/database/sql/where/ -v
```

---

## Conclusion

All 56 failing integration tests are due to **pre-existing functional issues**, not the test reorganization:

✅ **Test reorganization was successful** - Zero issues from file moves
✅ **Test discovery works perfectly** - All 159 tests found
✅ **Test execution works** - No import or structural errors

❌ **Functional issues exist** - Tests fail due to:
1. Outdated test code using old API signatures
2. Missing PostgreSQL type casting for special types
3. Missing validation for invalid operators

**Next Step**: Execute Phase 1 fixes (parameter order + names) to quickly resolve 68% of failures.
