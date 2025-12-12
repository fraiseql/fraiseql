# Phase 3: Fix DateRange Strategy

**Phase**: FIX (Update DateRangeOperatorStrategy)
**Duration**: 30 minutes
**Risk**: Low
**Status**: Ready for Execution

---

## Objective

Update `DateRangeOperatorStrategy` to use `_cast_both_sides()` for proper `::daterange` casting on both field and value sides.

**Success**: 9 daterange integration tests pass.

---

## Prerequisites

- [ ] Phases 1-2 completed
- [ ] 112/159 integration tests passing
- [ ] Clean git working directory

---

## Current Bug

**Current SQL** (wrong):
```sql
-- Missing field cast:
data->>'period' @> '2023-06-15'::date
```

**Expected SQL** (correct):
```sql
(data->>'period')::daterange @> '2023-06-15'::date
```

---

## Failing Tests

9 tests in `tests/integration/database/sql/where/temporal/test_daterange_operations.py`:
- `test_daterange_contains_date_operation`
- `test_daterange_in_list_with_casting`
- `test_daterange_nin_operation_with_casting`
- `test_daterange_typical_use_cases`
- `test_daterange_inclusive_exclusive_boundaries`
- Plus 4 more with parameter name issues (fixed in Phase 6)

---

## Implementation

### Update `build_sql()` Method

**File**: `src/fraiseql/sql/operators/postgresql/daterange_operators.py`

**Key changes**:
1. Use `_cast_both_sides()` for comparison operators
2. Cast field to `::daterange` for range operators (@>, &&, etc.)
3. Cast values appropriately (date, daterange)

**Example for contains_date operator**:
```python
if operator == "contains_date":
    # Cast field to daterange, value to date
    casted_path = SQL("({})::{}").format(path_sql, SQL("daterange"))
    casted_value = SQL("{}::{}").format(Literal(str(value)), SQL("date"))
    return SQL("{} @> {}").format(casted_path, casted_value)
```

**For equality/inequality**:
```python
if operator == "eq":
    casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
    return SQL("{} = {}").format(casted_path, casted_value)
```

---

## Verification

```bash
# Run daterange tests
uv run pytest tests/integration/database/sql/where/temporal/test_daterange_operations.py -v

# Expected: 5 tests pass now (4 still fail due to param names - fixed in Phase 6)
```

---

## Commit

```bash
git add src/fraiseql/sql/operators/postgresql/daterange_operators.py

git commit -m "fix(operators): Always cast both sides for DateRange comparisons

Update DateRangeOperatorStrategy to cast field to ::daterange and values
to appropriate types (::date, ::daterange). Fixes 5 integration tests.

SQL Before: data->>'period' @> '2023-06-15'::date  ❌
SQL After:  (data->>'period')::daterange @> '2023-06-15'::date  ✅

Phase: 3/7 (Fix DateRange Strategy)
Fixes: 5 daterange tests (4 more need param name fixes in Phase 6)"
```

---

**Next Phase**: Phase 4 - Fix Network Strategy
**Success Metric**: 117/159 tests passing (+5 from daterange fixes)
