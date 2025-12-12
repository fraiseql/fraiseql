# Why Unit Tests Passed But Integration Tests Failed

**Critical Finding**: Unit tests and integration tests use **completely different code paths**. This architectural split caused the operator refactor to introduce regressions that unit tests couldn't detect.

---

## The Architectural Split

### Unit Tests (OLD API - Direct Function Calls)

**What they test**:
```python
from fraiseql.sql.where.operators.mac_address import build_mac_eq_sql

# Directly call specialized function
result = build_mac_eq_sql(path_sql, "00:11:22:33:44:55")
```

**Code Path**:
```
Unit Test
    ↓
build_mac_eq_sql()
    ↓
build_comparison_sql(path_sql, value, "=", cast_type="macaddr")
    ↓
Generates: (data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr
✅ CORRECT - Both sides cast
```

### Integration Tests (NEW API - Operator Registry)

**What they test**:
```python
from fraiseql.sql.operators import get_default_registry

registry = get_default_registry()
# Call through operator registry
sql = registry.build_sql("eq", value, path_sql, field_type=MacAddress)
```

**Code Path**:
```
Integration Test
    ↓
OperatorRegistry.build_sql()
    ↓
MacAddressOperatorStrategy.build_sql()
    ↓
BaseOperatorStrategy._cast_path(path_sql, "macaddr", jsonb_column=None)
    ↓
Returns UNCAST path (because jsonb_column=None)
    ↓
BaseOperatorStrategy._build_comparison(uncast_path, value)
    ↓
Generates: data->>'mac_address' = '00:11:22:33:44:55'
❌ WRONG - No casts at all
```

---

## Root Cause Analysis

### Problem 1: `jsonb_column` Parameter Not Passed

**In `BaseOperatorStrategy._cast_path()`**:
```python
def _cast_path(
    self,
    path_sql: Composable,
    target_type: str,
    jsonb_column: Optional[str] = None,  # ← Defaults to None
    use_postgres_cast: bool = False,
) -> Composable:
    if jsonb_column:  # ← Only casts if jsonb_column is provided!
        if use_postgres_cast:
            return SQL("({})::{}").format(path_sql, SQL(target_type))
        return SQL("CAST({} AS {})").format(path_sql, SQL(target_type))

    # Regular column - no cast needed unless explicitly requested
    return path_sql  # ← Returns UNCAST if jsonb_column=None
```

**The Issue**:
- Integration tests call: `registry.build_sql("eq", value, path_sql, field_type=MacAddress)`
- They DON'T pass `jsonb_column` parameter (defaults to None)
- Result: `_cast_path()` returns the path WITHOUT casting

**Why This Assumption Was Wrong**:
The `_cast_path()` method assumes:
- If `jsonb_column` is provided → JSONB field, needs casting
- If `jsonb_column` is None → Regular column, NO casting needed

But this is WRONG for MAC addresses! Even regular MAC columns need PostgreSQL type casting for proper comparison. The assumption works for basic types (text, integer) but breaks for special types (macaddr, inet, ltree, daterange, point).

### Problem 2: Value Side Never Cast

**In `BaseOperatorStrategy._build_comparison()`**:
```python
def _build_comparison(
    self, operator: str, casted_path: Composable, value: Any
) -> Optional[Composable]:
    if operator == "eq":
        return SQL("{} = {}").format(casted_path, Literal(value))
        # ↑ Value is Literal(), never cast to type
```

**The Issue**:
- Even when the path IS cast: `(data->>'mac_address')::macaddr`
- The value is NOT cast: `'00:11:22:33:44:55'` (no ::macaddr)
- Result: `(data->>'mac_address')::macaddr = '00:11:22:33:44:55'` (only left side cast)

**Expected**:
```sql
(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr
```

**Old Specialized Functions Do This Correctly**:
```python
def build_mac_eq_sql(path_sql: SQL, value: str) -> Composed:
    return build_comparison_sql(path_sql, value, "=", "macaddr")
    # ↑ Passes cast_type="macaddr", which casts BOTH sides
```

---

## Why Unit Tests Missed This

### Coverage Gap Matrix

| Test Type | Code Path | API Used | Casts Applied | Status |
|-----------|-----------|----------|---------------|--------|
| **Unit Tests** | Direct function calls | `build_mac_eq_sql()` | Both sides ✓ | ✅ Pass |
| **Integration Tests** | Operator registry | `registry.build_sql()` | Neither side ✗ | ❌ Fail |

### The Disconnect

**Unit tests verify**:
- ✅ Old specialized functions work correctly
- ✅ SQL generation produces proper casts
- ✅ Each operator produces expected SQL

**Unit tests DON'T verify**:
- ❌ Operator registry routes to correct strategy
- ❌ Strategy integration with registry API
- ❌ Parameter passing through registry layers
- ❌ Real-world usage patterns (GraphQL → WHERE clause → SQL)

**Integration tests verify**:
- ✅ Full end-to-end flow (GraphQL → SQL → Database)
- ✅ Operator registry routing
- ✅ Type detection and casting in context
- ✅ Real-world usage patterns

But integration tests **assumed the refactor worked** because unit tests passed.

---

## Affected Operators

### Same Issue Across Multiple Types

| Type | Unit Tests | Integration Tests | Root Cause |
|------|-----------|-------------------|------------|
| **MAC Address** | ✅ Pass | ❌ Fail | `jsonb_column=None` → no casting |
| **DateRange** | ✅ Pass | ❌ Fail | `jsonb_column=None` → no field cast |
| **Coordinate** | ✅ Pass | ❌ Fail | Parameter order + formatting |
| **IP Address** | ✅ Pass | ❌ Fail | Parameter order issues |

**Pattern**: All PostgreSQL special types that need explicit casting are broken in the operator registry path.

---

## The Refactoring Context

### What Happened During Operator Refactor

**Before Refactor** (Working):
```python
# Flat module with direct function calls
from fraiseql.sql.where.operators.mac_address import build_mac_eq_sql

result = build_mac_eq_sql(path_sql, value)
# ✅ Works correctly
```

**After Refactor** (Broken):
```python
# New strategy pattern with registry
from fraiseql.sql.operators import get_default_registry

registry = get_default_registry()
result = registry.build_sql("eq", value, path_sql, field_type=MacAddress)
# ❌ Doesn't cast properly
```

**What Was Missed**:
1. Unit tests continued using OLD API (direct functions)
2. Integration tests switched to NEW API (registry)
3. No bridge tests verified OLD functions → NEW strategies compatibility
4. Assumption: "Unit tests pass → refactor is safe"

### The Test Gap

**Missing Test Category**: **"Integration Unit Tests"** or **"Strategy Integration Tests"**

These would test:
```python
def test_mac_address_strategy_produces_same_sql_as_old_function():
    """Verify MacAddressOperatorStrategy produces same SQL as build_mac_eq_sql."""
    # Old function
    old_sql = build_mac_eq_sql(path_sql, value)

    # New strategy
    registry = get_default_registry()
    new_sql = registry.build_sql("eq", value, path_sql, field_type=MacAddress)

    # Should be identical
    assert old_sql.as_string(None) == new_sql.as_string(None)
```

**This test would have caught the regression** ✅

---

## Parameter Order Issues

### Additional Problem: Signature Changes

**Old Signature** (before refactor):
```python
def build_sql(path_sql, operator, value, field_type):
    ...
```

**New Signature** (after refactor):
```python
def build_sql(operator, value, path_sql, field_type=None, jsonb_column=None):
    ...
```

**Result**: Many integration tests still use old parameter order:
```python
# OLD ORDER (wrong):
result = strategy.build_sql(
    SQL("data->>'ip_address'"),  # path_sql first
    "inSubnet",                   # operator second
    "192.168.1.0/24",            # value third
    IpAddress                     # field_type fourth
)
# ↓ SQL object passed where operator string expected
# ↓ TypeError: unhashable type: 'SQL'
```

**Why Unit Tests Didn't Catch This**:
- Unit tests call OLD direct functions (different signature)
- Integration tests call NEW operator registry (new signature)
- No tests verify signature compatibility

---

## Lessons Learned

### Why This Happened

1. **Incomplete Test Coverage**: Unit tests only covered OLD API, not NEW API
2. **Architecture Split**: Two completely different code paths (old functions vs. new strategies)
3. **False Confidence**: "All tests pass" didn't mean "refactor is safe"
4. **Missing Bridge Tests**: No tests verified OLD → NEW compatibility
5. **Parameter Assumptions**: `jsonb_column=None` assumed "no casting needed"

### What Should Have Been Done

**Phase 1: Write Integration Tests First**
```python
# BEFORE refactoring, write tests using NEW API
def test_registry_mac_address_eq():
    registry = get_default_registry()
    sql = registry.build_sql("eq", value, path_sql, field_type=MacAddress)
    assert "::macaddr" in sql.as_string(None)
    # ↑ This test would FAIL before refactor (no registry exists)
    # ↑ Forces you to implement registry correctly
```

**Phase 2: Verify Compatibility**
```python
# During refactor, verify OLD and NEW produce same output
@pytest.mark.parametrize("operator,value", [
    ("eq", "00:11:22:33:44:55"),
    ("neq", "ff:ff:ff:ff:ff:ff"),
    ("in", ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff"]),
])
def test_mac_strategy_matches_old_function(operator, value):
    old_sql = build_mac_eq_sql(path_sql, value)  # OLD API
    new_sql = registry.build_sql(operator, value, path_sql, field_type=MacAddress)  # NEW API
    assert old_sql.as_string(None) == new_sql.as_string(None)
```

**Phase 3: Migrate Tests Gradually**
```python
# After refactor, migrate unit tests to NEW API
def test_mac_eq_via_registry():  # NEW
    registry = get_default_registry()
    sql = registry.build_sql("eq", value, path_sql, field_type=MacAddress)
    assert "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr" == sql.as_string(None)

@pytest.mark.deprecated
def test_mac_eq_direct_function():  # OLD (kept for backward compat verification)
    sql = build_mac_eq_sql(path_sql, value)
    # ...
```

---

## Current State

### Test Results Summary

| Test Suite | Count | Pass | Fail | Coverage |
|------------|-------|------|------|----------|
| **Unit Tests** | 550+ | 550+ | 0 | ✅ OLD API only |
| **Integration Tests** | 159 | 103 | 56 | ❌ NEW API (broken) |

### Failure Breakdown

| Issue | Tests | Root Cause |
|-------|-------|------------|
| Parameter order | 28 | Tests use old signature |
| MAC casting | 9 | `jsonb_column=None` → no cast |
| DateRange casting | 9 | `jsonb_column=None` → no field cast |
| Parameter names | 10 | Tests use `op=` instead of `operator=` |

**All failures are API usage issues, not test reorganization issues** ✅

---

## Recommendations

### Immediate Actions

1. **Fix Integration Tests** (Phase 1 from FUNCTIONAL-ISSUES-ASSESSMENT.md)
   - Update parameter order (28 tests)
   - Fix parameter names (10 tests)
   - **Estimated**: 30 minutes
   - **Result**: 68% of failures resolved

2. **Fix Operator Strategies** (Phases 2-3)
   - Implement MAC address casting in registry path
   - Implement DateRange field casting
   - **Estimated**: 2-3 hours
   - **Result**: 100% of failures resolved

### Long-Term Improvements

3. **Add Bridge Tests**
   ```python
   # tests/unit/sql/operators/test_strategy_compatibility.py
   """Verify operator strategies produce same SQL as old functions."""

   def test_all_mac_operators_match():
       # For each operator, verify OLD function == NEW strategy
       ...
   ```

4. **Migrate Unit Tests to New API**
   - Gradually update unit tests to use operator registry
   - Keep OLD function tests marked as `@pytest.mark.deprecated`
   - Ensures unit tests catch regressions in registry path

5. **Add Registry Integration Tests**
   ```python
   # tests/unit/sql/operators/test_registry_integration.py
   """Test operator registry routing and SQL generation."""

   def test_registry_routes_mac_to_correct_strategy():
       strategy = registry.get_strategy("eq", MacAddress)
       assert isinstance(strategy, MacAddressOperatorStrategy)

   def test_registry_generates_correct_sql_for_mac():
       sql = registry.build_sql("eq", value, path_sql, field_type=MacAddress)
       assert "::macaddr" in sql.as_string(None)
   ```

6. **Document API Contract**
   ```python
   # src/fraiseql/sql/operators/README.md

   ## API Contract

   When implementing operator strategies:
   1. `supports_operator()` must check field_type
   2. `build_sql()` must apply casts when field_type is special type
   3. Value casting is strategy's responsibility
   4. Don't rely on `jsonb_column` for special type detection
   ```

---

## Conclusion

**Why unit tests passed but integration tests failed**:

1. ✅ Unit tests test OLD API (direct functions) → Working
2. ❌ Integration tests test NEW API (operator registry) → Broken
3. 🔍 No tests verify OLD API ≈ NEW API → Gap not detected
4. 📋 Refactor changed internal implementation → Different code paths
5. 🚨 Operator strategies have wrong assumptions → Missing casts

**The regression was invisible to unit tests** because they never exercised the refactored code path.

**Fix Strategy**:
- **Short-term**: Fix integration tests + operator strategies (2-3 hours)
- **Long-term**: Add bridge tests + migrate unit tests to new API

**Key Insight**: When refactoring, tests must cover BOTH the old and new implementations, and verify they produce identical results. Testing only one path leaves the other path untested.
