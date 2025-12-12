# WHERE Industrial Refactor - Recovery Plan to A+ Quality

**Status**: Current implementation at **B+ (85/100)** - Need to reach **A+ (95+/100)**

**Strategy**: Complete migration to new code path, NO backward compatibility needed

**Timeline**: 5-7 days (focused execution)

---

## Current State Analysis

### ✅ **COMPLETED** (Excellent Quality)
- Phase 1: Canonical representation (WhereClause, FieldCondition)
- Phase 2: Dict normalization
- Phase 3: WhereInput normalization (partial)
- Phase 5: FK metadata support (partial)

### ❌ **INCOMPLETE/BROKEN**
- Phase 4: SQL Generation (partially integrated, 73 test failures)
- Missing operators: vector, fulltext, array, ilike
- Missing `_to_whereinput_dict()` generation
- Missing parameter binding fixture
- Old code removed prematurely

### 🎯 **GAPS TO A+ QUALITY**
1. Complete operator coverage (all operators supported)
2. Full WhereInput integration
3. All tests passing (0 failures)
4. Performance optimization
5. Comprehensive documentation

---

## Recovery Phases

### Phase R1: Fix Critical Blockers [RED] 🔥
**Goal**: Get all existing tests passing with new code path
**Duration**: 2 days
**Priority**: CRITICAL

### Phase R2: Implement Missing Operators [GREEN] 🟢
**Goal**: Support all operators (vector, fulltext, array, etc.)
**Duration**: 2 days
**Priority**: HIGH

### Phase R3: WhereInput Integration [GREEN] 🟢
**Goal**: Full WhereInput support via `_to_whereinput_dict()`
**Duration**: 1 day
**Priority**: HIGH

### Phase R4: Optimization & Cleanup [REFACTOR] ⚡
**Goal**: Performance, code cleanup, final polish
**Duration**: 1-2 days
**Priority**: MEDIUM

### Phase R5: Documentation [QA] 📝
**Goal**: Comprehensive docs, examples, migration guide
**Duration**: 1 day
**Priority**: MEDIUM

---

## Detailed Phase Plans

See individual phase files:
- [Phase R1: Fix Critical Blockers](phase-r1-fix-critical-blockers.md)
- [Phase R2: Implement Missing Operators](phase-r2-implement-missing-operators.md)
- [Phase R3: WhereInput Integration](phase-r3-whereinput-integration.md)
- [Phase R4: Optimization & Cleanup](phase-r4-optimization-cleanup.md)
- [Phase R5: Documentation](phase-r5-documentation.md)

---

## Success Criteria (A+ Quality)

### **Code Quality** (Target: 95/100)
- [ ] All 4,900+ tests passing (0 failures)
- [ ] All operators supported (basic + advanced)
- [ ] Code coverage >90% for new modules
- [ ] No dead code
- [ ] No code smells (linting passes)

### **Architecture** (Target: 98/100)
- [ ] Single code path (no dual implementations)
- [ ] Clean separation of concerns
- [ ] Type safety enforced
- [ ] Observable (logging, debugging)

### **Security** (Target: 100/100)
- [ ] All security tests passing
- [ ] SQL injection protection verified
- [ ] Parameter binding correctness verified

### **Performance** (Target: 95/100)
- [ ] FK optimization working (80%+ rate)
- [ ] Normalization overhead <0.5ms
- [ ] No performance regressions
- [ ] Metrics collection implemented

### **Documentation** (Target: 90/100)
- [ ] API documentation complete
- [ ] Usage examples for all operators
- [ ] FK metadata examples
- [ ] Migration guide for edge cases
- [ ] Architecture documentation

### **Testing** (Target: 95/100)
- [ ] Unit tests comprehensive
- [ ] Integration tests complete
- [ ] Golden file tests passing
- [ ] Parameter binding tests passing
- [ ] Security tests passing

---

## Execution Strategy

### **Day 1-2: Phase R1** (Critical Blockers)
- Fix SQL generation errors (JSONB paths returning None)
- Implement missing `setup_hybrid_table` fixture
- Fix NoneType errors in normalization
- Get core tests passing (target: 90%+ pass rate)

### **Day 3-4: Phase R2** (Missing Operators)
- Implement `ilike` operator
- Implement vector distance operators
- Implement fulltext operators
- Implement array operators
- All operator tests passing

### **Day 5: Phase R3** (WhereInput Integration)
- Generate `_to_whereinput_dict()` method
- Full WhereInput→dict→WhereClause pipeline working
- GraphQL query tests passing

### **Day 6: Phase R4** (Optimization)
- Remove dead code
- Performance benchmarks
- Code cleanup
- Final test run (100% pass)

### **Day 7: Phase R5** (Documentation)
- API docs
- Migration guide
- Examples
- CHANGELOG update

---

## Risk Management

### **Risk 1: SQL Generation Complexity**
**Probability**: High
**Impact**: High
**Mitigation**: Focus Day 1 on fixing JSONB SQL generation
**Rollback**: None needed (moving forward only)

### **Risk 2: Operator Explosion**
**Probability**: Medium
**Impact**: Medium
**Mitigation**: Design extensible operator registry
**Alternative**: Support subset of operators initially, iterate

### **Risk 3: WhereInput Generation**
**Probability**: Low
**Impact**: High
**Mitigation**: Study existing type generation code first
**Fallback**: Manual testing if generation complex

### **Risk 4: Performance Regression**
**Probability**: Low
**Impact**: Medium
**Mitigation**: Benchmark before/after each phase
**Acceptance**: ±5% performance is acceptable

---

## Testing Strategy

### **Continuous Verification** (After Each Phase)
```bash
# Quick test (30 seconds)
uv run pytest tests/unit/test_where_*.py -v

# Integration test (2 minutes)
uv run pytest tests/integration/database/ -v -k "where or filter"

# Full regression (5 minutes)
uv run pytest tests/regression/test_where_*.py -v

# Complete suite (30 minutes)
uv run pytest tests/ -v
```

### **Phase Exit Criteria**
Each phase must meet:
- All phase-specific tests passing
- No new test failures introduced
- Code coverage maintained/improved
- Linting passes (`ruff check`)

---

## Code Quality Checklist

### **Before Each Commit**
- [ ] Run phase-specific tests
- [ ] Run full test suite
- [ ] Check for dead code
- [ ] Run linter (`ruff check src/fraiseql/`)
- [ ] Update CHANGELOG if user-facing change

### **Before Phase Completion**
- [ ] All acceptance criteria met
- [ ] Code reviewed (self-review minimum)
- [ ] Documentation updated
- [ ] Performance verified

---

## Communication Plan

### **Daily Updates**
- End of day: Update phase status in this file
- Blockers: Document in phase file
- Completed: Mark in checklist

### **Phase Completion**
- Update `RECOVERY_PLAN.md` status
- Commit with descriptive message
- Tag if stable milestone

---

## Rollout Plan

### **No Feature Flag Needed**
Since backward compatibility is NOT required:
- Remove old code paths immediately in Phase R1
- Single code path: `_normalize_where()` → `WhereClause.to_sql()`
- Clean migration, no hybrid state

### **Deployment**
1. **After Phase R1**: Code compiles, core tests pass (90%+)
2. **After Phase R2**: All operators working, tests pass (95%+)
3. **After Phase R3**: Full GraphQL integration (100%)
4. **After Phase R4**: Production-ready, optimized
5. **After Phase R5**: Release v1.9.0

---

## Metrics Tracking

### **Test Pass Rate**
- **Baseline** (now): 4,828 / 4,901 = 98.5% (but 73 new failures = regression)
- **After R1**: Target 4,880 / 4,901 = 99.5%
- **After R2**: Target 4,895 / 4,901 = 99.9%
- **After R3**: Target 4,901 / 4,901 = 100%
- **Final**: 4,901 / 4,901 = 100%

### **Code Coverage**
- **Baseline**: Unknown
- **Target**: >90% for new modules
- **Measurement**: `pytest --cov=src/fraiseql/where_clause --cov=src/fraiseql/where_normalization`

### **Performance**
- **Normalization overhead**: <0.5ms (target)
- **FK optimization rate**: >80% (target)
- **SQL generation**: <0.1ms (target)

### **Code Quality**
- **Ruff violations**: 0 (target)
- **Dead code lines**: 0 (target)
- **Test coverage**: >90% (target)

---

## Final Deliverables

### **Code**
1. `src/fraiseql/where_clause.py` - Canonical representation (DONE)
2. `src/fraiseql/where_normalization.py` - Normalization logic (DONE, needs cleanup)
3. `src/fraiseql/db.py` - Integration (PARTIAL, needs completion)
4. `src/fraiseql/sql/graphql_where_generator.py` - WhereInput generation (NEEDS UPDATE)

### **Tests**
1. `tests/unit/test_where_clause.py` - Unit tests (DONE)
2. `tests/unit/test_where_clause_security.py` - Security tests (DONE)
3. `tests/unit/test_where_normalization.py` - Normalization tests (DONE)
4. `tests/integration/test_parameter_binding.py` - Parameter tests (NEEDS FIXTURE)
5. `tests/regression/test_where_golden.py` - Golden tests (NEEDS FIXES)

### **Documentation**
1. `docs/where-architecture.md` - Architecture overview (NEW)
2. `docs/where-operators.md` - Operator reference (NEW)
3. `docs/where-migration-guide.md` - Migration guide (NEW)
4. `README.md` - Updated examples (UPDATE)
5. `CHANGELOG.md` - Release notes (UPDATE)

---

## Version Planning

### **v1.9.0** - WHERE Industrial Refactor (NEXT)
- Complete refactor to single code path
- All operators supported
- Performance optimized
- **BREAKING**: None (internal refactor only)

### **Future** - v2.0.0 (If Needed)
- If breaking changes discovered during implementation
- Clean API surface
- Deprecated features removed

---

## Team Assignments (If Applicable)

### **Solo Developer** (Recommended)
- Execute phases sequentially
- Focus on quality over speed
- Complete testing before moving to next phase

### **2 Developers** (Optional Parallel)
- Dev 1: Phase R1 + R2 (SQL generation, operators)
- Dev 2: Phase R3 + R4 (WhereInput, optimization)
- Both: Phase R5 (documentation)
- **Risk**: Integration conflicts
- **Mitigation**: Daily sync, clear interfaces

---

## Getting Started

### **Step 1: Read Phase Plans**
```bash
cd .phases/where-industrial-refactor
cat phase-r1-fix-critical-blockers.md
```

### **Step 2: Set Up Environment**
```bash
# Ensure latest dependencies
uv sync

# Run baseline tests
uv run pytest tests/ -v --tb=short > baseline_results.txt

# Check current failures
grep "FAILED" baseline_results.txt | wc -l
```

### **Step 3: Start Phase R1**
```bash
# Create feature branch
git checkout -b feature/where-refactor-recovery

# Begin implementation
# Follow phase-r1-fix-critical-blockers.md
```

---

## Success Definition

**A+ Quality Achieved When**:
1. ✅ All 4,901 tests passing (100%)
2. ✅ All operators implemented
3. ✅ Code coverage >90%
4. ✅ Performance targets met
5. ✅ Security tests passing
6. ✅ Documentation complete
7. ✅ No code smells
8. ✅ Ready for production deployment

**Timeline**: 5-7 days of focused work

**Confidence**: High (clear plan, scoped work)

---

**Document Version**: 1.0
**Created**: 2025-12-11
**Author**: Claude Code (QA Specialist)
**Status**: READY FOR EXECUTION
