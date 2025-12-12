# WHERE Type Generation Industrial Refactor

## Overview

This refactor transforms the FraiseQL WHERE clause processing from a fragile, multi-path system into an industrial-grade, single-path architecture with explicit contracts and comprehensive validation.

## Problem Statement

**Current Architecture Issues:**
1. **Dual Code Paths**: Dict-based and WhereInput-based processing diverge, causing bugs
2. **Late Type Conversion**: WhereInput → SQL → dict loses type information
3. **Implicit Behavior**: FK column detection happens at runtime through pattern matching
4. **Test Coverage Gaps**: Tests verify outcomes, not code paths or SQL correctness
5. **No Canonical Representation**: No single source of truth for WHERE clauses

**Impact:**
- Nested filters work with dicts but fail with WhereInput objects (Issue #XXX)
- Tests pass even when using wrong SQL (full table scan vs FK index)
- Hard to debug (binary SQL objects, implicit behavior)
- Hard to extend (must update multiple code paths)

## Target Architecture

```
User Input (dict/WhereInput)
    → Normalize (validate + resolve FK vs JSONB)
    → Canonical WhereClause (type-safe, inspectable)
    → SQL Generation (single implementation)
    → PostgreSQL
```

**Key Principles:**
1. **Single Source of Truth**: `WhereClause` canonical representation
2. **Normalize Early**: Validation and FK resolution before SQL generation
3. **Explicit Metadata**: FK relationships declared at registration time
4. **Type Safety**: Dataclasses with runtime validation
5. **Observable**: Structured logging, readable repr
6. **Test Internals**: Multi-level testing (normalization, SQL, equivalence, performance)

## Benefits

- ✅ **Correctness**: One code path, one SQL implementation
- ✅ **Performance**: Explicit FK optimization, caching opportunities
- ✅ **Debuggability**: Readable `WhereClause` repr, structured logging
- ✅ **Testability**: Test each layer independently
- ✅ **Maintainability**: 50% less code, clear contracts
- ✅ **Extensibility**: Add operators/types in one place

## Phases Overview

### Phase 1: Define Canonical Representation [RED]
**Goal:** Create `WhereClause` and `FieldCondition` dataclasses with tests

- Define type-safe canonical representation
- Add validation logic
- Write comprehensive tests (TDD: write tests first, expect failures)

**Duration:** 1-2 days
**Risk:** Low (new code, no changes to existing)

### Phase 2: Implement Dict Normalization [GREEN]
**Goal:** Convert dict WHERE to `WhereClause`

- Implement `_normalize_dict_where()`
- Handle nested objects, FK detection
- All Phase 1 tests should pass

**Duration:** 2-3 days
**Risk:** Medium (complex logic, FK detection)

### Phase 3: Implement WhereInput Normalization [GREEN]
**Goal:** Convert WhereInput to `WhereClause`

- Add `_to_whereinput_dict()` method to generated WhereInput classes
- Implement `_normalize_whereinput()`
- Handle Filter objects (UUIDFilter, StringFilter, etc.)

**Duration:** 2-3 days
**Risk:** Medium (must handle all Filter types)

### Phase 4: Refactor SQL Generation [REFACTOR]
**Goal:** Single SQL generation from `WhereClause`

- Implement `WhereClause.to_sql()`
- Refactor `_build_where_clause()` to normalize first
- Ensure all existing tests pass

**Duration:** 2-3 days
**Risk:** High (touches core query logic)

### Phase 5: Add Explicit FK Metadata [GREEN]
**Goal:** Make FK relationships explicit

- Add `fk_relationships` to `register_type_for_view()`
- Attach metadata to generated WhereInput classes
- Validation at generation time

**Duration:** 1-2 days
**Risk:** Low (additive changes)

### Phase 6: Remove Old Code Paths [REFACTOR]
**Goal:** Delete redundant code

- Remove `_where_obj_to_dict()`
- Remove dict-specific branches in `_build_where_clause()`
- Simplify `_convert_dict_where_to_sql()`

**Duration:** 1 day
**Risk:** Low (covered by tests)

### Phase 7: Optimization & Caching [REFACTOR]
**Goal:** Performance improvements

- Cache normalized WhereInput
- Optimize SQL generation
- Add performance benchmarks

**Duration:** 1-2 days
**Risk:** Low (optimization only)

### Phase 8: Documentation & Migration [QA]
**Goal:** Documentation and migration guide

- API documentation
- Migration guide for users
- Performance comparison
- Architecture documentation

**Duration:** 1-2 days
**Risk:** Low (documentation only)

## Total Timeline

**Estimated:** 2-3 weeks
**Minimum Viable:** Phases 1-4 (1 week for core functionality)
**Full Industrial-Grade:** All 8 phases (2-3 weeks)

## Testing Strategy

### Level 1: Unit Tests (Normalization)
Test each normalization function independently:
- Dict → WhereClause
- WhereInput → WhereClause
- Validation logic

### Level 2: Integration Tests (SQL Generation)
Test `WhereClause.to_sql()`:
- FK column SQL
- JSONB path SQL
- Mixed scenarios
- Logical operators (AND, OR, NOT)

### Level 3: Equivalence Tests
Ensure dict and WhereInput produce identical results:
- Same WhereClause after normalization
- Same SQL after generation
- Same query results

### Level 4: Code Path Tests
Ensure correct code paths are taken:
- No warning logs
- FK optimization used
- JSONB fallback when appropriate

### Level 5: Performance Tests
Ensure optimizations work:
- FK filters use index
- No sequential scans
- Cache hits for repeated queries

### Level 6: Regression Tests
All existing tests must pass:
- Run full test suite after each phase
- No behavior changes for users
- Backward compatibility maintained

## Rollout Strategy

### Development
1. Each phase in feature branch
2. All tests pass before merge
3. Code review with architecture checklist

### Deployment
1. Phases 1-3: Additive (safe to deploy incrementally)
2. Phase 4: Enable via feature flag initially
3. Phases 5-6: Full rollout after Phase 4 stable
4. Phases 7-8: Performance and polish

### Rollback Plan
- Phase 4 includes feature flag: can disable new code path
- Keep old code until Phase 6 (safety net)
- Comprehensive logging to detect issues early

## Success Criteria

1. ✅ All existing tests pass
2. ✅ Dict and WhereInput produce identical SQL
3. ✅ No "Unsupported operator" warnings for valid queries
4. ✅ FK nested filters use FK column (verified in logs)
5. ✅ 50%+ reduction in WHERE-related code
6. ✅ Sub-millisecond normalization overhead
7. ✅ Clear error messages for invalid queries
8. ✅ Documentation complete

## Dependencies

- Python 3.10+ (dataclasses, pattern matching)
- psycopg 3.x (SQL composition)
- pytest (testing framework)
- No new external dependencies

## Risks & Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Breaking existing queries | Medium | High | Comprehensive test suite, feature flag |
| Performance regression | Low | Medium | Benchmarks, caching layer |
| Complex FK detection logic | Medium | Medium | Explicit metadata, validation |
| Migration complexity | Low | Low | Backward compatible, gradual rollout |

## Questions & Decisions

### Q: Should we support both dict and WhereInput long-term?
**A:** Yes. Both are valid use cases:
- Dict: Dynamic queries, testing, direct API usage
- WhereInput: GraphQL resolvers, type-safe queries

Both normalize to `WhereClause`, so supporting both is cheap.

### Q: How do we handle backward compatibility?
**A:**
1. Keep old code paths until Phase 6
2. Feature flag for Phase 4 (new SQL generation)
3. Extensive testing before removal
4. Version as minor bump (1.9.0), not major

### Q: What about performance?
**A:**
- Normalization adds ~0.1-0.5ms overhead
- Caching reduces to near-zero for repeated queries
- SQL generation unchanged (same performance)
- FK optimization improves query performance 10-100x

### Q: How do we handle edge cases (e.g., jsonb field named 'id')?
**A:**
- Explicit metadata overrides convention
- `fk_relationships` explicitly declares FK fields
- Non-FK fields use JSONB path even if named 'id'
- Clear error messages for ambiguous cases

## Next Steps

1. Review this plan with team
2. Create tracking issue/epic
3. Begin Phase 1 (define canonical representation)
4. Set up CI/CD for phase-based development

---

**Author:** Claude + Lionel
**Date:** 2025-12-10
**Status:** Planning
**Epic:** WHERE Industrial Refactor
