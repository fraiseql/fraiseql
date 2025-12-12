# Phase 5: Fix Coordinate Strategy

**Phase**: FIX (Update CoordinateOperatorStrategy)
**Duration**: 30 minutes
**Risk**: Low
**Status**: Ready for Execution

---

## Objective

Update `CoordinateOperatorStrategy` to use `_cast_both_sides()` for `::point` casting and fix coordinate formatting.

**Success**: Coordinate integration tests pass.

---

## Prerequisites

- [ ] Phases 1-4 completed

---

## Current Issues

1. Missing `::point` casts on some operators
2. Inconsistent POINT() formatting (spacing)

---

## Affected Tests

11 tests in `tests/integration/database/sql/where/spatial/test_coordinate_operations.py`:
- All coordinate operation tests

---

## Implementation

**File**: `src/fraiseql/sql/operators/advanced/coordinate_operators.py`

**Update to use `_cast_both_sides()`**:
```python
if operator == "eq":
    # Convert (lat, lng) tuple to POINT(lng, lat) format
    lng, lat = value[1], value[0]  # Swap order
    point_value = f"POINT({lng}, {lat})"  # Note: space after comma

    casted_path = SQL("({})::{}").format(path_sql, SQL("point"))
    casted_value = SQL("{}").format(SQL(point_value))  # No literal for POINT
    return SQL("{} = {}").format(casted_path, casted_value)
```

**Key**: POINT values are PostgreSQL functions, not literals, so don't use Literal().

---

## Verification

```bash
# Run coordinate tests
uv run pytest tests/integration/database/sql/where/spatial/test_coordinate_operations.py -v

# Expected: All 11 tests pass ✅
```

---

## Commit

```bash
git add src/fraiseql/sql/operators/advanced/coordinate_operators.py

git commit -m "fix(operators): Always cast coordinates to ::point with consistent formatting

Update CoordinateOperatorStrategy to:
- Cast field to ::point
- Use POINT(lng, lat) syntax for values
- Standardize spacing in POINT() output

Phase: 5/7 (Fix Coordinate Strategy)
Fixes: 11 coordinate tests"
```

---

**Next Phase**: Phase 6 - Fix Integration Test Parameter Issues
**Progress**: ~142/159 tests passing (casting bugs mostly fixed)
