# Phase 3: Query Builder

**Phase:** GREEN (Make Tests Pass)
**Duration:** 6-8 hours
**Risk:** Medium

---

## Objective

**TDD Phase GREEN:** Extract SQL query building logic from FraiseQLRepository.

Extract:
- `_build_find_query()` → FindQueryBuilder
- `_build_find_one_query()` → FindOneQueryBuilder
- Aggregation query building → AggregateQueryBuilder
- Function execution → FunctionQueryBuilder

---

## Files to Create

1. `src/fraiseql/db/query_builder/base.py` - Base query builder interface
2. `src/fraiseql/db/query_builder/find_builder.py` - find() queries (~250 lines)
3. `src/fraiseql/db/query_builder/aggregate_builder.py` - Aggregation queries (~250 lines)
4. `src/fraiseql/db/query_builder/function_builder.py` - Database functions (~200 lines)

---

## Key Extraction Targets

From `FraiseQLRepository`:
- Lines 1433-1544: `_build_find_query()`
- Lines 1546-1563: `_build_find_one_query()`
- Lines 761-1184: Aggregation methods (sum, avg, min, max, aggregate)
- Lines 241-392: Function execution methods

---

## Implementation Steps

1. Create base QueryBuilder interface
2. Extract find() query building
3. Extract aggregation query building
4. Extract function execution
5. Update FraiseQLRepository to use builders
6. Run tests - all query building tests should PASS

---

## Next Phase

→ **Phase 4:** WHERE Clause Builder
