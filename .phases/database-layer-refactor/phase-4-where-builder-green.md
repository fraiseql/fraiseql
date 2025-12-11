# Phase 4: WHERE Clause Builder

**Phase:** GREEN (Make Tests Pass)
**Duration:** 4-6 hours
**Risk:** Medium

---

## Objective

**TDD Phase GREEN:** Extract WHERE clause building, integrate with existing where_clause.py.

Extract:
- `_normalize_where()`
- `_build_where_clause()`
- `_build_dict_where_condition()`
- `_build_basic_dict_condition()`

Integrate with:
- `fraiseql/where_clause.py`
- `fraiseql/where_normalization.py`
- `fraiseql/sql/graphql_where_generator.py`

---

## Files to Create

1. `src/fraiseql/db/where/where_builder.py` - WHERE building (~300 lines)
2. `src/fraiseql/db/where/integration.py` - Integration layer (~100 lines)

---

## Next Phase

→ **Phase 5:** Connection Manager
