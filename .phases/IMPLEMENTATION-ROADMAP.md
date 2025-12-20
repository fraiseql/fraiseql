# FraiseQL GraphQL Spec Compliance: Complete Implementation Roadmap

**Date:** December 17, 2025
**Status:** Ready for Implementation
**Total Effort:** 8-11 hours
**Deliverables:** 3 detailed implementation plans

---

## Overview

This roadmap contains **detailed implementation plans** for three GraphQL spec compliance features that align with FraiseQL's view-centric architecture:

1. **Nested Field Fragments** (2-3 hours)
2. **Fragment Cycle Detection** (3-4 hours)
3. **View/Metadata Directives** (2-4 hours)

Each plan includes:
- ✅ Step-by-step implementation instructions
- ✅ Complete code examples
- ✅ Comprehensive test suite (50+ tests total)
- ✅ Success criteria and acceptance tests
- ✅ Risk analysis and mitigation
- ✅ Integration checklist

---

## Implementation Plans

### Plan 1: Nested Field Fragments

**File:** `.phases/implementation-plan-nested-fragments.md`

**What:** Enable fragment spreads in nested field selections (recursive fragment resolution)

**Why:**
- Complex denormalized views have many fields
- Fragment reuse becomes critical as schemas grow
- Enables composition of view selectors

**Current state:**
- ✅ Fragment resolver works at root level
- ❌ Doesn't process nested field selections

**Implementation:**
- Modify `src/fraiseql/core/fragment_resolver.py` (30 lines)
- Recursive field resolution
- Handle nested inline fragments
- Deduplicate at each level

**Testing:**
- 15+ unit tests (nested, deep nesting, aliases, dedup)
- 5+ integration tests (multi-field queries)
- Performance benchmarks

**Risk:** Low
**Complexity:** Low
**Value:** High

---

### Plan 2: Fragment Cycle Detection

**File:** `.phases/implementation-plan-fragment-cycles.md`

**What:** Detect and reject circular fragment references at parse time

**Why:**
- Prevents infinite loops in query execution
- DoS prevention
- Early error detection with clear messages
- Enables safe fragment validation

**Current state:**
- ❌ No cycle detection
- Circular fragments silently allowed
- Can cause runtime failures

**Implementation:**
- Create `src/fraiseql/core/fragment_validator.py` (NEW)
- DFS-based cycle detection with backtracking
- Type compatibility validation
- Integration into query processing pipeline

**Testing:**
- 20+ unit tests (self-ref, mutual, transitive cycles, valid patterns)
- 5+ integration tests (endpoint validation)
- Error message quality tests

**Risk:** Low
**Complexity:** Low-Moderate
**Value:** High (safety/stability)

---

### Plan 3: View/Metadata Directives

**File:** `.phases/implementation-plan-view-directives.md`

**What:** Support metadata directives for views and dependencies

**Why:**
- Document view dependencies (implicit in SQL today)
- Enable schema validation and tooling
- Query cost analysis and planning
- Explicit view refresh strategy

**Directives:**
- `@view_cached(ttl: Int!)` - Cache/refresh TTL
- `@depends_on(views: [String!]!)` - Upstream dependencies
- `@requires_function(name: String!)` - SQL function requirement
- `@cost_units(estimate: Float!)` - Query complexity estimate

**Current state:**
- ❌ No metadata directives
- Dependencies implicit in SQL
- No cost tracking

**Implementation:**
- Create `src/fraiseql/gql/schema_directives.py` (NEW)
- Create `src/fraiseql/gql/directive_validator.py` (NEW)
- Add to schema in `schema_builder.py`
- Optional validation at schema build time

**Testing:**
- 15+ unit tests (directive classes, validation)
- 10+ integration tests (introspection, schema validation)

**Risk:** Low
**Complexity:** Low-Moderate
**Value:** High (documentation/tooling)

---

## Quick Start

### Reading the Plans

Each implementation plan is complete and self-contained:

```bash
# Read implementation plans
cat .phases/implementation-plan-nested-fragments.md      # 1. Fragments
cat .phases/implementation-plan-fragment-cycles.md       # 2. Cycles
cat .phases/implementation-plan-view-directives.md       # 3. Directives
```

### Suggested Reading Order

1. **Start with Nested Fragments** (simplest, no new files)
2. **Then Fragment Cycles** (adds validation, new file)
3. **Finally Directives** (integrates into schema, multiple files)

### Implementation Order

Can be done in any order, but suggested:

1. **Week 1:** Nested Fragments + Fragment Cycles (query safety)
2. **Week 2:** View/Metadata Directives (schema documentation)

---

## Architecture Overview

### Phase 1: Query Safety (Week 1)

```
Nested Fragments
├── Problem: Can't reuse fragments in nested selections
├── Solution: Recursive fragment resolution
└── Impact: Better query composition for complex views

Fragment Cycles
├── Problem: Circular fragments cause infinite loops
├── Solution: DFS cycle detection at parse time
└── Impact: Schema safety, clear error messages
```

### Phase 2: Schema Documentation (Week 2)

```
View/Metadata Directives
├── Problem: View dependencies implicit, not documented
├── Solution: Metadata directives + validation
└── Impact: Tools can analyze schema, cost planning possible
```

---

## Success Metrics

### Code Quality
- [ ] **70+ new tests** (unit + integration + performance)
- [ ] **100% test pass rate**
- [ ] **Zero regressions** in 6000+ existing tests
- [ ] **> 95% code coverage** for new modules
- [ ] **All ruff/black checks** pass

### Functionality
- [ ] **Nested fragments** work in deeply nested selections
- [ ] **Fragment cycles** detected and rejected
- [ ] **Directives** appear in introspection
- [ ] **Validation** works with clear error messages
- [ ] **End-to-end** queries work correctly

### Performance
- [ ] **< 5% variance** from baseline (no regression)
- [ ] **Fragment resolution** < 1ms per query
- [ ] **Cycle detection** < 10ms per query
- [ ] **Schema building** unchanged

### Documentation
- [ ] **Clear error messages** for users
- [ ] **Code comments** explain algorithms
- [ ] **Docstrings** with examples
- [ ] **Migration guide** (if needed)

---

## File Changes Summary

### New Files (5 total)

```
src/fraiseql/core/
├── fragment_validator.py          # Cycle detection
└──

src/fraiseql/gql/
├── schema_directives.py           # Directive definitions
└── directive_validator.py         # Directive validation

tests/unit/core/
├── test_nested_fragments.py       # Fragment tests
├── test_fragment_cycles.py        # Cycle tests

tests/unit/gql/
├── test_schema_directives.py      # Directive tests

tests/integration/fastapi/
├── test_nested_fragments.py
├── test_fragment_cycles.py

tests/integration/gql/
├── test_directives_integration.py

tests/performance/
├── test_fragment_resolution_perf.py  # Benchmarks
```

### Modified Files (3 total)

```
src/fraiseql/core/
├── fragment_resolver.py           # Add recursive resolution (+30 lines)

src/fraiseql/gql/
├── schema_builder.py              # Add directives to schema (+50 lines)

src/fraiseql/analysis/
├── query_complexity.py            # Fix fragment handling (+30 lines)

src/fraiseql/fastapi/
├── routers.py                     # Add cycle validation (+30 lines)
```

**Total changes:** ~140 lines of production code, ~1500 lines of tests

---

## Testing Strategy

### Unit Tests (45+ tests)
- Fragment resolution (15 tests)
- Fragment cycles (20 tests)
- Directives (15 tests)

### Integration Tests (20+ tests)
- Multi-field queries with fragments
- Endpoint validation for cycles
- Directive introspection

### Performance Tests (5+ tests)
- Fragment resolution time
- Cycle detection time
- Schema building time

### Full Suite
- Run existing 6000+ tests
- Verify no regressions
- Benchmark comparison

---

## Risk Assessment

### Risk 1: Breaking Existing Tests
**Probability:** Low
**Impact:** Medium
**Mitigation:** Run full test suite after each change

### Risk 2: Performance Regression
**Probability:** Low
**Impact:** High
**Mitigation:** Benchmark before/after, monitor metrics

### Risk 3: Fragment Complexity Issues
**Probability:** Low
**Impact:** Medium
**Mitigation:** Comprehensive cycle detection tests

### Risk 4: Directive Validation Too Strict
**Probability:** Medium
**Impact:** Low
**Mitigation:** Make validation optional, warnings only

---

## Implementation Timeline

### Recommended Schedule

```
Day 1-2: Nested Fragments
├── Read plan: 30 min
├── Implement: 1-1.5 hours
├── Test: 1 hour
└── Code review: 30 min

Day 3-4: Fragment Cycles
├── Read plan: 30 min
├── Implement: 2-2.5 hours
├── Test: 1 hour
└── Code review: 30 min

Day 5-6: View Directives
├── Read plan: 30 min
├── Implement: 2 hours
├── Test: 1-1.5 hours
└── Code review: 30 min

Day 7: Integration & Verification
├── Run full test suite: 30 min
├── Benchmark: 30 min
├── Final review: 30 min
└── Merge to dev: 15 min
```

**Total:** 7-8 days (can be done faster if working full-time)

---

## Checkpoints

### After Nested Fragments
```bash
# Should pass
pytest tests/unit/core/test_nested_fragments.py -v
pytest tests/integration/fastapi/test_nested_fragments.py -v

# Should not regress
pytest tests/ -k fragment -v
```

### After Fragment Cycles
```bash
# Should pass
pytest tests/unit/core/test_fragment_cycles.py -v
pytest tests/integration/fastapi/test_fragment_cycles.py -v

# Should still pass
pytest tests/unit/core/test_nested_fragments.py -v
```

### After View Directives
```bash
# Should pass
pytest tests/unit/gql/test_schema_directives.py -v
pytest tests/integration/gql/test_directives_integration.py -v

# Should still pass (full suite)
pytest tests/ -v
```

### Final Verification
```bash
# Full test suite
pytest tests/ -v --tb=short

# Performance check
pytest tests/performance/ -v --benchmark-compare

# Type checking
mypy src/ --strict

# Linting
ruff check src/
black --check src/
```

---

## Rollout Strategy

### Phase 1: Development
- [ ] Implement features on feature branches
- [ ] Local testing and verification
- [ ] Address code review feedback

### Phase 2: Integration
- [ ] Merge to dev branch
- [ ] Run full test suite
- [ ] Verify no regressions
- [ ] Performance benchmarking

### Phase 3: Validation
- [ ] Code review approval
- [ ] Documentation complete
- [ ] Ready for release in next minor version

---

## What NOT to Implement

These gaps were rejected based on FraiseQL's architecture:

### ❌ Auto DataLoaders (Gap #3)
- **Reason:** Denormalized views eliminate N+1 queries by design
- **Why:** All joins pre-computed in `tv_*` materialized views
- **Alternative:** Use `tv_user_with_posts` instead of loading separately

### ❌ HTTP Streaming (Gap #4)
- **Reason:** Out of scope for bounded query results
- **Why:** FraiseQL returns complete, pre-shaped results
- **Alternative:** Use WebSocket subscriptions for real streaming

---

## Documentation

### For Developers
- Detailed implementation plans (this file + 3 separate plans)
- Code comments with examples
- Algorithm explanations
- Testing strategy

### For Users
- Error messages with solutions
- Directive descriptions
- Usage examples
- Migration guide (if needed)

### For Tooling Teams
- Directive definitions (enables tools)
- Introspection support
- Dependency graph documentation
- Cost analysis integration

---

## Contact & Questions

For questions while implementing:

1. **Review the relevant plan** - All details documented
2. **Check test examples** - Expected behavior shown
3. **Look at similar code** - FraiseQL patterns to follow

---

## Next Steps

1. **Read the plans** (start with nested fragments)
2. **Set up feature branches** (one per feature)
3. **Implement in order** (fragments → cycles → directives)
4. **Run tests continuously** (after each step)
5. **Get code reviews** (before merging)
6. **Deploy to dev** (prepare for release)

---

## Summary

This roadmap provides everything needed to implement **3 GraphQL spec compliance features** that align with FraiseQL's view-centric architecture:

| Feature | Effort | Impact | Status |
|---------|--------|--------|--------|
| Nested Fragments | 2-3h | High | ✅ Planned |
| Fragment Cycles | 3-4h | High | ✅ Planned |
| View Directives | 2-4h | High | ✅ Planned |

**Total effort:** 8-11 hours
**Total tests:** 70+
**Expected improvement:** 90% → 93% spec compliance
**Architecture alignment:** ✅ 100% (view-centric design)

Each plan is **complete, self-contained, and ready for implementation**.

---

**Document Status:** ✅ Ready for Implementation
**Plans Status:** ✅ Ready for Implementation
**Next Action:** Choose a plan and start implementing
