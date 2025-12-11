# Phase 4: Advanced Operators Migration

**Phase:** GREEN (Make Tests Pass)
**Duration:** 6-8 hours
**Risk:** Medium-High

---

## Objective

**TDD Phase GREEN:** Implement advanced operators to make all remaining tests pass.

Migrate:
- Array operators (contains, overlaps, len_*, any_eq, all_eq)
- JSONB operators (has_key, contains, path_exists, etc.)
- Full-text search operators (matches, rank, websearch, etc.)
- Vector operators (cosine_distance, l2_distance, etc.)
- Coordinate/GIS operators (distance_within, etc.)

---

## Files to Create

### 1. `src/fraiseql/sql/operators/array/array_operators.py`
### 2. `src/fraiseql/sql/operators/advanced/jsonb_operators.py`
### 3. `src/fraiseql/sql/operators/advanced/fulltext_operators.py`
### 4. `src/fraiseql/sql/operators/advanced/vector_operators.py`
### 5. `src/fraiseql/sql/operators/advanced/coordinate_operators.py`

---

## Verification Commands

```bash
# Run array operator tests
uv run pytest tests/ -k "array" -v

# Run JSONB operator tests
uv run pytest tests/ -k "jsonb" -v

# Run fulltext tests
uv run pytest tests/ -k "fulltext or tsquery or tsvector" -v

# Run vector tests
uv run pytest tests/ -k "vector or cosine or l2_distance" -v

# Full test suite
uv run pytest tests/unit/sql/where/ -v
uv run pytest tests/integration/database/ -v
```

---

## Acceptance Criteria

- [ ] All array operators implemented and tested
- [ ] All JSONB operators implemented and tested
- [ ] All fulltext operators implemented and tested
- [ ] All vector operators implemented and tested
- [ ] All coordinate operators implemented and tested
- [ ] 100% of tests passing
- [ ] No regressions

---

## Next Phase

Once all operators are migrated:
→ **Phase 5:** Refactor & Optimize (extract common patterns)
