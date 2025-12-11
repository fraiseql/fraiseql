# Operator Strategies Industrial Refactoring

**Status:** PLANNED
**Type:** Industrial Refactoring (Similar to WHERE clause refactor)
**Estimated Duration:** 2-3 days
**Risk Level:** Medium

---

## Objective

Refactor `src/fraiseql/sql/operator_strategies.py` (2,149 lines) into specialized, focused strategy modules following the same successful pattern used in the WHERE clause industrial refactor.

**Current Problem:**
- Single monolithic file with 2,149 lines
- Mixes 10+ different specialized operator types
- Difficult to navigate and maintain
- No clear separation between operator families

**Desired Outcome:**
- ~10-12 focused strategy modules (150-250 lines each)
- Clear separation of concerns
- Easier to test individual operator families
- Easier to add new operators
- Maintains 100% backward compatibility

---

## Context

This refactor follows the successful industrial refactoring pattern from the WHERE clause work (commits `93652288`, `87067fbd`, `1985fea2`). The WHERE clause refactor demonstrated that large, complex SQL generation code can be successfully broken down while maintaining full test coverage.

**Reference Success:**
- WHERE clause refactor: Completed in 3 phases (R1, R2, R3)
- All 3,645 tests passing
- Zero regressions
- Improved maintainability

---

## Implementation Phases

Following TDD 4-phase cycle: **RED → GREEN → REFACTOR → QA** + Legacy Cleanup

### Phase 1: Foundation & Test Infrastructure (RED)
**File:** `phase-1-foundation-red.md`
**Goal:** Create base strategy interface, test infrastructure, failing tests
**Duration:** 4-6 hours
**TDD Phase:** RED (write tests first, they fail)

### Phase 2: Core Operators Migration (GREEN)
**File:** `phase-2-core-operators-green.md`
**Goal:** Implement string, numeric, boolean operators to make tests pass
**Duration:** 6-8 hours
**TDD Phase:** GREEN (make tests pass)

### Phase 3: Specialized PostgreSQL Types (GREEN)
**File:** `phase-3-specialized-types-green.md`
**Goal:** Migrate network, ltree, daterange, macaddr operators
**Duration:** 6-8 hours
**TDD Phase:** GREEN (continue making tests pass)

### Phase 4: Advanced Operators (GREEN)
**File:** `phase-4-advanced-operators-green.md`
**Goal:** Migrate array, JSONB, fulltext, vector operators
**Duration:** 6-8 hours
**TDD Phase:** GREEN (final operators to make all tests pass)

### Phase 5: Refactor & Optimize (REFACTOR)
**File:** `phase-5-refactor.md`
**Goal:** Extract common patterns, optimize performance, reduce duplication
**Duration:** 3-4 hours
**TDD Phase:** REFACTOR (improve code while keeping tests green)

### Phase 6: Quality Assurance & Integration (QA)
**File:** `phase-6-qa.md`
**Goal:** Full test suite validation, performance benchmarks, edge cases
**Duration:** 2-3 hours
**TDD Phase:** QA (comprehensive verification)

### Phase 7: Legacy Cleanup (CLEANUP)
**File:** `phase-7-cleanup.md`
**Goal:** Remove old operator_strategies.py, finalize API, update imports
**Duration:** 2-3 hours
**TDD Phase:** CLEANUP (remove deprecated code)

### Phase 8: Documentation (FINAL)
**File:** `phase-8-documentation.md`
**Goal:** Update all documentation, migration guide, examples
**Duration:** 2-3 hours
**TDD Phase:** FINAL (polish and document)

---

## Success Criteria

### Code Quality
- [ ] No file > 300 lines
- [ ] Clear single responsibility per operator module
- [ ] All operator strategies inherit from base class
- [ ] Zero circular dependencies

### Testing
- [ ] All 4,943 tests passing
- [ ] Zero new test failures
- [ ] Operator coverage maintained at 100%
- [ ] Integration tests for all operator families

### Compatibility
- [ ] 100% backward compatible public API
- [ ] All imports from `fraiseql.sql.operator_strategies` work unchanged
- [ ] No changes to `OperatorStrategy` interface
- [ ] Existing code continues to work without modification

### Documentation
- [ ] All operator families documented
- [ ] Migration guide for contributors
- [ ] Updated architecture diagrams
- [ ] Example usage for each operator family

---

## Target File Structure

```
src/fraiseql/sql/operators/
├── __init__.py                    # Public API exports
├── base.py                        # BaseOperatorStrategy abstract class
├── strategy_registry.py           # Operator registration system
│
├── core/
│   ├── __init__.py
│   ├── string_operators.py       # contains, icontains, startswith, etc.
│   ├── numeric_operators.py      # gt, lt, gte, lte, eq, neq
│   ├── boolean_operators.py      # eq, neq, isnull
│   └── date_operators.py         # date comparisons
│
├── array/
│   ├── __init__.py
│   ├── array_operators.py        # contains, overlaps, len_*, any_eq, all_eq
│   └── array_utils.py            # Array detection and handling utilities
│
├── postgresql/
│   ├── __init__.py
│   ├── network_operators.py      # isprivate, ispublic, insubnet, etc.
│   ├── ltree_operators.py        # ancestor_of, descendant_of, matches_lquery
│   ├── daterange_operators.py    # contains_date, overlaps, adjacent, etc.
│   └── macaddr_operators.py      # MAC address operators
│
├── advanced/
│   ├── __init__.py
│   ├── jsonb_operators.py        # has_key, contains, path_exists, etc.
│   ├── fulltext_operators.py     # matches, plain_query, websearch_query, rank_*
│   ├── vector_operators.py       # cosine_distance, l2_distance, etc.
│   └── coordinate_operators.py   # distance_within, etc.
│
└── utils/
    ├── __init__.py
    ├── type_detection.py         # Type detection utilities
    └── sql_builders.py           # Common SQL building helpers
```

**Total Files:** ~20 focused modules instead of 1 monolithic file

---

## Risks & Mitigations

### Risk 1: Breaking Existing Imports
**Impact:** HIGH
**Mitigation:**
- Keep `operator_strategies.py` as facade with deprecation warnings
- All public API re-exported from `__init__.py`
- Run full test suite after each phase

### Risk 2: Circular Dependencies
**Impact:** MEDIUM
**Mitigation:**
- Base strategy in separate `base.py`
- Registry pattern for operator registration
- Dependency injection where needed

### Risk 3: Test Coverage Gaps
**Impact:** MEDIUM
**Mitigation:**
- Run tests after each operator family migration
- Check coverage reports after each phase
- Add missing tests before migration

### Risk 4: Performance Regression
**Impact:** LOW
**Mitigation:**
- Benchmark critical paths before/after
- No additional indirection in hot paths
- Keep operator lookup O(1) with registry

---

## Dependencies

### Prerequisite
- [ ] Fix WHERE clause JSONB path regression first
- [ ] All tests passing before starting
- [ ] Full backup/snapshot of current state

### Blocks
- None (this is self-contained refactoring)

### Enables
- Future operator additions easier
- Per-operator-family testing
- Better documentation structure

---

## Rollback Plan

If issues arise during refactoring:

1. **Phase-level rollback:** Revert to last passing commit
2. **Keep old file:** Don't delete `operator_strategies.py` until Phase 5
3. **Feature flag:** Could add feature flag to toggle new/old implementation
4. **Gradual migration:** Migrate operator families one at a time with fallback

---

## Notes

- Follow TDD 4-phase cycle for each operator family migration
- Commit after each operator family successfully migrated
- Use `[GREEN]` tags for successful migrations
- Reference WHERE clause refactor commits for patterns
- Leverage existing test coverage (don't write new tests unless gaps found)

---

## Related Work

- **WHERE Clause Refactor:** `.phases/industrial-where-refactor/` (COMPLETED)
- **DB.py Refactor:** `.phases/database-layer-refactor/` (PLANNED)
- **Mutation Decorator Refactor:** `.phases/mutation-decorator-refactor/` (PLANNED)
