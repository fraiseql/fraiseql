# Phase 4: Fix Network Strategy

**Phase**: FIX (Update NetworkOperatorStrategy)
**Duration**: 45 minutes
**Risk**: Medium (many tests affected)
**Status**: Ready for Execution

---

## Objective

Update `NetworkOperatorStrategy` for IP address operators to use `_cast_both_sides()` for `::inet` casting.

**Success**: Network-specific operator tests pass.

---

## Prerequisites

- [ ] Phases 1-3 completed
- [ ] 117/159 integration tests passing

---

## Current Bug

Missing `::inet` casts on IP address comparisons and network operations.

---

## Affected Tests

Network tests in `tests/integration/database/sql/where/network/`:
- `test_ip_operations.py` (3 tests - also need param order fixes)
- `test_jsonb_integration.py` (2 tests - also need param order fixes)
- Several in `test_network_fixes.py` and `test_production_bugs.py`

---

## Implementation

**File**: `src/fraiseql/sql/operators/postgresql/network_operators.py`

**Update operators to cast to ::inet**:
```python
# For inSubnet:
if operator == "inSubnet":
    casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
    return SQL("{} <<= {}").format(casted_path, casted_value)

# For isPrivate (different pattern - check against known ranges):
if operator == "isPrivate":
    casted_path = SQL("({})::{}").format(path_sql, SQL("inet"))
    # Check if IP in private ranges
    return SQL("{} <<= ANY(ARRAY['10.0.0.0/8'::inet, '172.16.0.0/12'::inet, '192.168.0.0/16'::inet])").format(casted_path)
```

---

## Verification

```bash
# Note: Many tests also have parameter order issues
# Those will be fixed in Phase 6

# Run network tests
uv run pytest tests/integration/database/sql/where/network/ -v --tb=short

# Expected: Some tests pass, some still fail due to param order
```

---

## Commit

```bash
git add src/fraiseql/sql/operators/postgresql/network_operators.py

git commit -m "fix(operators): Always cast both sides for IP address operations

Update NetworkOperatorStrategy to cast field and value to ::inet for
network operators (inSubnet, isPrivate, isPublic, etc.).

Note: Some tests still fail due to parameter order issues (fixed Phase 6).

Phase: 4/7 (Fix Network Strategy)"
```

---

**Next Phase**: Phase 5 - Fix Coordinate Strategy
