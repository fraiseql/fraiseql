# WHERE Industrial Refactor - Recovery Plan Index

**Current Quality**: B+ (85/100)
**Target Quality**: A+ (95+/100)
**Timeline**: 5-7 days
**Status**: READY FOR EXECUTION

---

## Quick Navigation

| Document | Purpose | Duration | Status |
|----------|---------|----------|--------|
| **[RECOVERY_PLAN.md](RECOVERY_PLAN.md)** | Master recovery plan | - | ✅ Ready |
| **[Phase R1](phase-r1-fix-critical-blockers.md)** | Fix critical blockers | 2 days | 🔴 Not Started |
| **[Phase R2](phase-r2-implement-missing-operators.md)** | Missing operators | 2 days | ⏸️ Blocked |
| **[Phase R3](phase-r3-whereinput-integration.md)** | WhereInput integration | 1 day | ⏸️ Blocked |
| **[Phase R4](phase-r4-optimization-cleanup.md)** | Optimization & cleanup | 1-2 days | ⏸️ Blocked |
| **[Phase R5](phase-r5-documentation.md)** | Documentation | 1 day | ⏸️ Blocked |

---

## Phase Dependencies

```
Phase R1 (Fix Critical Blockers)
    ↓
Phase R2 (Missing Operators) ← Can start after R1
    ↓
Phase R3 (WhereInput) ← Can start after R1+R2
    ↓
Phase R4 (Optimization) ← Needs R1+R2+R3 complete
    ↓
Phase R5 (Documentation) ← Needs all phases complete
    ↓
Release v1.9.0 🚀
```

---

## Current State Summary

### ✅ What's Working (B+ Quality)
- **Phase 1** (Original): Canonical representation (WhereClause, FieldCondition) - EXCELLENT
- **Phase 2** (Original): Dict normalization - EXCELLENT
- **Phase 3** (Original): WhereInput normalization (partial) - GOOD
- **Phase 5** (Original): FK metadata support - GOOD
- **Security**: SQL injection protection verified
- **Tests**: 48 new tests (100% passing)

### ❌ What's Broken (Need to Fix)
- **73 test failures** - Various causes (see R1)
- **Missing operators** - Vector, fulltext, array (see R2)
- **WhereInput integration incomplete** - Missing `_to_whereinput_dict()` (see R3)
- **SQL generation errors** - JSONB paths returning None
- **Missing fixtures** - `setup_hybrid_table` for tests

### 🎯 Gaps to A+ Quality
1. All tests passing (0 failures)
2. Complete operator coverage
3. Performance optimization
4. Comprehensive documentation
5. Production-ready polish

---

## Execution Checklist

### Before Starting
- [ ] Read [RECOVERY_PLAN.md](RECOVERY_PLAN.md) completely
- [ ] Understand current state (QA report)
- [ ] Set up development environment
- [ ] Run baseline tests
- [ ] Create feature branch: `feature/where-refactor-recovery`

### Phase R1: Fix Critical Blockers (MUST DO FIRST)
- [ ] Fix JSONB path SQL generation (Step 1)
- [ ] Fix `_normalize_where()` edge cases (Step 2)
- [ ] Update `_build_where_clause()` integration (Step 3)
- [ ] Update/delete tests referencing old methods (Step 4)
- [ ] Create `setup_hybrid_table` fixture (Step 5)
- [ ] Fix "SQL values must be strings, got None" errors (Step 6)
- [ ] Remove dead code (Step 7)
- [ ] **Verification**: Test pass rate >99.5%

### Phase R2: Implement Missing Operators
- [ ] Add string operators (`ilike`, `like`)
- [ ] Add vector operators (5 operators)
- [ ] Add fulltext operators (8 operators)
- [ ] Add array operators (11 operators)
- [ ] Create operator documentation
- [ ] **Verification**: All operator tests passing

### Phase R3: WhereInput Integration
- [ ] Analyze WhereInput generation code
- [ ] Implement `_to_whereinput_dict()` generation
- [ ] Test Filter object conversion
- [ ] Integration with normalization
- [ ] GraphQL query end-to-end tests
- [ ] **Verification**: GraphQL tests passing

### Phase R4: Optimization & Cleanup
- [ ] Remove all dead code
- [ ] Add performance metrics (WhereMetrics)
- [ ] Add EXPLAIN mode
- [ ] Performance benchmarking
- [ ] Code quality polish (linting, types, docs)
- [ ] **Verification**: Performance targets met

### Phase R5: Documentation
- [ ] Architecture documentation
- [ ] Usage examples
- [ ] Migration guide
- [ ] CHANGELOG update
- [ ] README update
- [ ] API documentation
- [ ] **Verification**: All docs complete

### Release Preparation
- [ ] All 4,901 tests passing (100%)
- [ ] Performance targets met (<0.5ms overhead)
- [ ] Security tests passing
- [ ] Documentation complete
- [ ] CHANGELOG updated
- [ ] Git tag: `v1.9.0`
- [ ] **SHIP IT! 🚀**

---

## Success Metrics

### Code Quality (Target: 95/100)
| Metric | Current | Target | Phase |
|--------|---------|--------|-------|
| Test pass rate | 98.5% (73 failures) | 100% | R1-R3 |
| Operator coverage | 60% | 100% | R2 |
| Code coverage | Unknown | >90% | R4 |
| Linting violations | Unknown | 0 | R4 |
| Dead code lines | ~20 | 0 | R4 |

### Architecture (Target: 98/100)
| Metric | Current | Target | Phase |
|--------|---------|--------|-------|
| Single code path | ✅ Yes | ✅ Yes | Done |
| Type safety | ✅ Yes | ✅ Yes | Done |
| Observability | ❌ No | ✅ Yes | R4 |

### Performance (Target: 95/100)
| Metric | Current | Target | Phase |
|--------|---------|--------|-------|
| Normalization overhead | Unknown | <0.5ms | R4 |
| FK optimization rate | Unknown | >80% | R4 |
| Performance regressions | Unknown | 0 | R4 |

### Documentation (Target: 90/100)
| Metric | Current | Target | Phase |
|--------|---------|--------|-------|
| Architecture docs | ❌ None | ✅ Complete | R5 |
| Usage examples | ❌ Partial | ✅ Complete | R5 |
| Migration guide | ❌ None | ✅ Complete | R5 |
| API reference | ❌ None | ✅ Complete | R5 |

---

## Risk Assessment

| Risk | Probability | Impact | Mitigation | Phase |
|------|-------------|--------|------------|-------|
| SQL generation too complex | Medium | High | Incremental fixes, good tests | R1 |
| Operator explosion | Low | Medium | Extensible design | R2 |
| WhereInput generation hard | Low | High | Study existing code first | R3 |
| Performance targets missed | Low | Medium | Profile before optimizing | R4 |
| Timeline overrun | Medium | Low | Realistic estimates, daily tracking | All |

---

## Daily Progress Tracking

### Day 1: Phase R1 Start
**Goal**: Fix JSONB SQL generation, normalize edge cases
- [ ] Morning: Steps 1-2
- [ ] Afternoon: Step 3
- [ ] EOD: 50% of R1 complete

### Day 2: Phase R1 Finish
**Goal**: Complete R1, all core tests passing
- [ ] Morning: Steps 4-5
- [ ] Afternoon: Steps 6-7
- [ ] EOD: R1 complete, >99% tests passing

### Day 3: Phase R2 Start
**Goal**: String + Vector operators
- [ ] Morning: Step 1 (string)
- [ ] Afternoon: Step 2 (vector)
- [ ] EOD: 40% of R2 complete

### Day 4: Phase R2 Finish
**Goal**: Fulltext + Array operators
- [ ] Morning: Step 3 (fulltext)
- [ ] Afternoon: Step 4 (array)
- [ ] EOD: R2 complete, all operator tests passing

### Day 5: Phase R3
**Goal**: WhereInput integration complete
- [ ] Morning: Steps 1-3
- [ ] Afternoon: Steps 4-5
- [ ] EOD: R3 complete, GraphQL tests passing

### Day 6: Phase R4
**Goal**: Optimization & cleanup
- [ ] Morning: Steps 1-3
- [ ] Afternoon: Steps 4-6
- [ ] EOD: R4 complete, performance targets met

### Day 7: Phase R5
**Goal**: Documentation & release
- [ ] Morning: Steps 1-3
- [ ] Afternoon: Steps 4-6
- [ ] EOD: R5 complete, ready for release

---

## Commands Quick Reference

### Testing
```bash
# Quick test (30s)
uv run pytest tests/unit/test_where_*.py -v

# Integration (2min)
uv run pytest tests/integration/database/ -v -k "where"

# Full suite (30min)
uv run pytest tests/ -v

# Specific phase tests
uv run pytest tests/unit/test_where_clause.py -v              # R1
uv run pytest tests/integration/test_vector_e2e.py -v         # R2
uv run pytest tests/integration/graphql/test_whereinput*.py -v # R3
uv run pytest tests/performance/ --benchmark-only             # R4
```

### Code Quality
```bash
# Linting
ruff check src/fraiseql/where*.py

# Type checking
mypy src/fraiseql/where_clause.py

# Coverage
pytest --cov=src/fraiseql/where_clause --cov-report=html

# Metrics
python -c "from fraiseql.where_metrics import WhereMetrics; print(WhereMetrics.get_stats())"
```

### Git
```bash
# Create branch
git checkout -b feature/where-refactor-recovery

# Commit after each phase
git add .
git commit -m "feat(where): complete Phase R1 - fix critical blockers"

# Tag for release
git tag -a v1.9.0 -m "Release v1.9.0 - WHERE Industrial Refactor Complete"
```

---

## Getting Help

### Documentation
- **QA Report**: See main conversation for detailed assessment
- **Original Plans**: `.phases/where-industrial-refactor/phase-*.md`
- **Recovery Plan**: [RECOVERY_PLAN.md](RECOVERY_PLAN.md)

### Issues
If blocked, document in phase file:
- Describe blocker
- Steps attempted
- Error messages
- Hypothesis for solution

---

## Completion Criteria

**A+ Quality Achieved When**:
- ✅ All 4,901 tests passing (100%)
- ✅ All operators implemented (30+ operators)
- ✅ Code coverage >90%
- ✅ Performance <0.5ms overhead
- ✅ FK optimization >80%
- ✅ Security tests passing
- ✅ Documentation complete
- ✅ No code smells
- ✅ Ready for production

**Then**: Release v1.9.0 🚀

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2025-12-11 | Initial recovery plan created |

---

**Status**: READY FOR EXECUTION
**Confidence**: HIGH
**Risk**: LOW-MEDIUM (well-scoped, clear plan)

**Next Action**: Read [phase-r1-fix-critical-blockers.md](phase-r1-fix-critical-blockers.md) and begin execution.
