# Backport Critical Fixes from v1.9.2-v1.9.4 to Starlette Implementation

**Date**: January 5, 2026
**Status**: CRITICAL - Must be integrated before v2.0.0 release
**Severity**: HIGH (APQ field selection, WHERE clause handling, ID policy)

---

## Summary

The new Starlette implementation in Phase 2-3 was created before these critical fixes were committed. The implementations **do not have** the latest v1.9.2-v1.9.4 fixes that are essential for production.

**Critical Fixes Missing**:
1. ✅ **APQ Field Selection Fix** (v1.9.4) - Response caching broke field selection
2. ✅ **IDFilter Type Addition** (v1.9.3-v1.9.4) - New filter type for ID fields
3. ✅ **IDPolicy-Aware WHERE Filters** (v1.9.3) - ID type handling in WHERE clauses
4. ✅ **Type Introspection Stubs** (Phase 23) - IDE autocompletion

---

## Critical Fix Details

### 1. APQ Field Selection Bug Fix (v1.9.4)

**Issue**: APQ was caching full responses, breaking field selection
**Impact**: HIGH - Data leak vulnerability
**Status**: Fixed in FastAPI, **NOT in Starlette**

**What Needs to Change in Starlette**:

In `src/fraiseql/starlette/app.py`, the GraphQL handler must NOT cache responses for APQ requests:

```python
# WRONG (current Starlette implementation):
async def graphql_handler(...):
    # Execute query
    result = await execute_graphql(...)
    # Build response
    response = GraphQLResponse(...)
    # Return response
    return await response_formatter.format_response(response)

# CORRECT (from v1.9.4 FastAPI fix):
async def graphql_handler(...):
    # Execute query - ALWAYS, even for APQ hash-only requests
    result = await execute_graphql(...)

    # NOTE: APQ response caching is intentionally NOT implemented.
    # APQ should only cache query strings (persisted queries), not responses.
    # Caching responses breaks field selection because the same persisted query
    # with different field selections would return identical cached data.
    #
    # Correct behavior:
    # 1. Store query by hash (in ApqStorage)
    # 2. On hash-only request, retrieve query by hash
    # 3. Execute query normally with client's field selection
    # 4. Return only the requested fields

    # Build response with field selection intact
    response = GraphQLResponse(...)
    return await response_formatter.format_response(response)
```

**Tests That Must Pass**:
- `tests/integration/test_apq_field_selection.py` (10+ tests)

---

### 2. IDFilter Type Addition (v1.9.3-v1.9.4)

**Issue**: ID fields in WHERE clauses need a dedicated filter type
**Impact**: MEDIUM - WHERE clause consistency
**Status**: Added to WHERE generator, **NOT in Starlette's interface**

**What Needs to Change in Starlette**:

The Starlette implementation doesn't need changes, BUT the WHERE generator must be properly imported and used.

**In `src/fraiseql/http/interface.py`**, no changes needed (it's just the abstraction).

**In `src/fraiseql/starlette/app.py`**, ensure imports are correct:

```python
from fraiseql.graphql.execute import execute_graphql
# This already handles WHERE clause generation correctly
```

**The WHERE Generator Must Have** (from v1.9.3):

```python
# In src/fraiseql/sql/graphql_where_generator.py
@fraise_input
class IDFilter:
    """GraphQL ID field filter operations.

    Used for filtering on ID fields in where clauses. The ID type
    accepts any string value (UUIDs, integers, slugs, etc.) as per
    GraphQL specification.
    """
    eq: ID | None = None
    neq: ID | None = None
    in_: list[ID] | None = fraise_field(default=None, graphql_name="in")
    nin: list[ID] | None = None
    isnull: bool | None = None
```

**Type Mapping Must Include** (from v1.9.4):

```python
# In _get_filter_type_for_field()
type_mapping = {
    str: StringFilter,
    int: IntFilter,
    float: FloatFilter,
    Decimal: DecimalFilter,
    bool: BooleanFilter,
    ID: IDFilter,  # NEW: Always use IDFilter for ID type
    UUID: UUIDFilter,
    date: DateFilter,
    datetime: DateTimeFilter,
    dict: JSONBFilter,
}
```

---

### 3. IDPolicy-Aware WHERE Filtering (v1.9.3)

**Issue**: ID fields should respect IDPolicy in WHERE clauses
**Impact**: MEDIUM - ID type consistency
**Status**: Implemented in WHERE generator, **OK for Starlette**

**Key Change from v1.9.3**:

Before: ID type used different filter types based on IDPolicy
```python
# IDPolicy.UUID → UUIDFilter
# IDPolicy.OPAQUE → IDFilter
```

After: ID type ALWAYS uses IDFilter (Scenario A)
```python
# ALL policies → IDFilter
# UUID validation happens at runtime, not schema level
```

**Why This Matters for Starlette**:
- GraphQL schema stays consistent with frontend (`$id: ID!`)
- No frontend query changes needed when switching policies
- UUID validation happens at runtime via `execute_graphql()`

**For Starlette**: No code changes needed. The WHERE generator handles this correctly.

---

### 4. Type Introspection Stubs (Phase 23)

**Issue**: IDE autocompletion for GraphQL context missing
**Impact**: LOW - Developer experience
**Status**: Added in Phase 23, **OK for Starlette**

**No Code Changes Required**: Type stubs are in `src/fraiseql/stubs/` and automatically used.

---

## Action Plan

### Step 1: Verify Current Fixes in Production Code ✅

- ✅ APQ field selection fix verified in `src/fraiseql/fastapi/routers.py`
- ✅ IDFilter type verified in `src/fraiseql/sql/graphql_where_generator.py`
- ✅ IDPolicy-aware filtering verified in `src/fraiseql/sql/graphql_where_generator.py`

### Step 2: Ensure Starlette Implementation Uses Them ⚠️

The Starlette implementation **doesn't explicitly import or handle** APQ response caching, so it should be SAFE (doesn't have the bug), but we need to verify:

1. **APQ Test Coverage**: Add APQ tests to Starlette parity tests
   ```python
   # In tests/starlette/test_parity.py
   class TestAPQParity:
       def test_apq_field_selection_consistency(self):
           # Query with full fields
           response1 = client.post("/graphql", json={
               "query": "query { users { id name email } }"
           })

           # Same query with APQ (hash only, requesting fewer fields)
           response2 = client.post("/graphql", json={
               "extensions": {
                   "persistedQuery": {
                       "version": 1,
                       "sha256Hash": "abc123"
                   }
               }
           })

           # Should respect field selection
           assert len(response1.json()["data"]["users"][0].keys()) == 3  # id, name, email
           assert len(response2.json()["data"]["users"][0].keys()) == 2  # id, name (not email)
   ```

2. **WHERE Clause Test Coverage**: Add WHERE clause tests with ID fields
   ```python
   # In tests/starlette/test_parity.py
   class TestFieldSelectionParity:
       def test_id_field_filtering(self):
           # Query using ID filter
           query = """
           query {
               users(where: { id: { eq: "user-123" } }) {
                   id
                   name
               }
           }
           """
           response = client.post("/graphql", json={"query": query})
           assert response.status_code == 200
           # Should use IDFilter type correctly
   ```

### Step 3: Backport Tests from v1.9.4 ✅

Critical tests that must exist:

1. `tests/integration/test_apq_field_selection.py` (10+ tests)
   - Test APQ with different field selections
   - Verify response caching doesn't happen

2. `tests/config/test_id_policy.py` (6+ tests)
   - Test ID filter type selection
   - Test UUID validation at runtime

3. WHERE clause tests with ID fields
   - Test ID field filtering
   - Test IDPolicy behavior

### Step 4: Ensure Starlette Handles These Correctly ✅

**APQ Handling**: The Starlette implementation should:
- ✅ Parse APQ extensions correctly (already done)
- ✅ NOT cache responses (Starlette doesn't have the bug because it doesn't implement APQ caching)
- ✅ Pass tests that verify field selection works

**WHERE Clause Handling**: The Starlette implementation should:
- ✅ Use `execute_graphql()` which handles WHERE generation (already done)
- ✅ Pass through ID filter types correctly (no code needed)
- ✅ Pass ID policy tests (handled by query execution layer)

---

## Checklist

### Code Changes

- [ ] APQ field selection parity test added to `tests/starlette/test_parity.py`
- [ ] WHERE clause with ID filtering tests added
- [ ] IDPolicy behavior tests added
- [ ] APQ test suite passes on Starlette (`pytest tests/integration/test_apq_field_selection.py`)
- [ ] ID policy tests pass on Starlette (`pytest tests/config/test_id_policy.py`)

### Verification

- [ ] Run full test suite: `pytest tests/ -v` (should be 5991+ tests)
- [ ] Run parity tests specifically: `pytest tests/starlette/test_parity.py -v`
- [ ] Run APQ tests: `pytest tests/integration/test_apq_field_selection.py -v`
- [ ] Run ID policy tests: `pytest tests/config/test_id_policy.py -v`
- [ ] All tests pass with 0 failures

### Documentation

- [ ] Update Starlette docs to mention APQ field selection behavior
- [ ] Add note about IDPolicy and WHERE clause filtering
- [ ] Update implementation summary with fix verification

---

## Risk Assessment

### Risk: APQ Field Selection Bug in Starlette

**Severity**: HIGH
**Likelihood**: LOW (Starlette doesn't implement response caching)
**Mitigation**:
- ✅ Add parity test that verifies field selection works with APQ
- ✅ Ensure test suite runs before release

### Risk: ID Filter Type Not Used

**Severity**: MEDIUM
**Likelihood**: LOW (handled by query execution layer)
**Mitigation**:
- ✅ Add WHERE clause tests with ID fields
- ✅ Verify tests pass in Starlette

### Risk: IDPolicy Changes Break Starlette

**Severity**: MEDIUM
**Likelihood**: LOW (IDPolicy is handled in query executor, not HTTP layer)
**Mitigation**:
- ✅ Run ID policy test suite against Starlette
- ✅ Verify no regressions

---

## Timeline

### Immediate (Before v2.0.0 Release)

1. Add APQ field selection parity tests (**30 min**)
2. Add WHERE clause tests with ID fields (**30 min**)
3. Run full test suite against Starlette (**2 hours**)
4. Verify all tests pass (**1 hour**)
5. Update documentation (**30 min**)

**Total: ~4.5 hours**

---

## References

### Commits with Fixes

- **v1.9.4**: `c00d8c30` - APQ field selection fix + IDFilter Scenario A
- **v1.9.3**: `e5900d92` - IDPolicy-aware filter mapping
- **v1.9.2**: `9c5cd58d` - WHERE clause enhancements

### Test Files

- `tests/integration/test_apq_field_selection.py` - 10+ APQ tests
- `tests/config/test_id_policy.py` - 6+ ID policy tests
- `tests/starlette/test_parity.py` - Parity tests (need APQ + WHERE additions)

### Key Files to Review

- `src/fraiseql/fastapi/routers.py` - How APQ is handled correctly
- `src/fraiseql/sql/graphql_where_generator.py` - IDFilter implementation
- `src/fraiseql/starlette/app.py` - Starlette implementation

---

## Conclusion

**Status**: Starlette implementation is likely SAFE for APQ and ID policy because:
1. Starlette doesn't implement response caching (so no APQ bug)
2. ID filtering is handled by query execution layer (so no WHERE clause bug)
3. Tests need to be added to VERIFY this is true

**Action**: Add comprehensive parity tests before v2.0.0 release to ensure Starlette handles these cases correctly.

**Estimated Additional Effort**: 4-5 hours of testing and verification
