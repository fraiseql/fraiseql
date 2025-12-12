# WHERE Industrial Refactor - Complete Index

## Quick Navigation

| Document | Purpose | When to Read |
|----------|---------|--------------|
| **[README.md](README.md)** | Overview, phases, timeline | Start here |
| **[IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md)** | Execution strategy, checklist | Before starting |
| **[CRITICAL_IMPROVEMENTS_SUMMARY.md](CRITICAL_IMPROVEMENTS_SUMMARY.md)** | Security & correctness fixes | **READ THIS FIRST** |

---

## Phase Documents

### Phase 1: Define Canonical Representation [RED]
**File:** [phase-1-canonical-representation.md](phase-1-canonical-representation.md)
**Duration:** 1-2 days
**Risk:** Low

**What:** Create `WhereClause` and `FieldCondition` dataclasses with tests
**Key Addition:** ✅ SQL injection protection tests (CRITICAL)
**Next:** Phase 2

---

### Phase 2: Implement Dict Normalization [GREEN]
**File:** [phase-2-dict-normalization.md](phase-2-dict-normalization.md)
**Duration:** 2-3 days
**Risk:** Medium

**What:** Convert dict WHERE to `WhereClause`
**Key Feature:** FK vs JSONB detection logic
**Next:** Phase 3

---

### Phase 3: Implement WhereInput Normalization [GREEN]
**File:** [phase-3-whereinput-normalization.md](phase-3-whereinput-normalization.md)
**Duration:** 2-3 days
**Risk:** Medium

**What:** Convert WhereInput to `WhereClause` (fixes the bug!)
**Key Feature:** Proper Filter object extraction
**Next:** Phase 4

---

### Phase 4: Refactor SQL Generation [REFACTOR]
**File:** [phase-4-sql-generation-refactor.md](phase-4-sql-generation-refactor.md)
**Duration:** 2-3 days
**Risk:** **High** (critical path)

**What:** Single SQL generation from `WhereClause`
**Key Addition:** ✅ Parameter binding correctness tests (CRITICAL)
**Next:** Phase 5

---

### Phase 5: Add Explicit FK Metadata [GREEN]
**File:** [phase-5-explicit-fk-metadata.md](phase-5-explicit-fk-metadata.md)
**Duration:** 1-2 days
**Risk:** Low

**What:** Make FK relationships explicit in metadata
**Key Addition:** ✅ Strict validation by default (CRITICAL)
**Next:** Phase 6

---

### Phase 6: Remove Old Code Paths [REFACTOR]
**File:** [phase-6-cleanup.md](phase-6-cleanup.md)
**Duration:** 1 day
**Risk:** Low

**What:** Delete redundant WHERE processing code
**Key Addition:** ✅ Golden file regression tests (CRITICAL)
**Expected:** 500-800 lines removed
**Next:** Phase 7

---

### Phase 7: Performance Optimization [REFACTOR]
**File:** [phase-7-optimization.md](phase-7-optimization.md)
**Duration:** 1-2 days
**Risk:** Low

**What:** Caching and performance optimization
**Key Additions:**
- ✅ EXPLAIN mode for debugging
- ✅ Performance metrics collection
**Target:** <0.5ms normalization overhead
**Next:** Phase 8

---

### Phase 8: Documentation [QA]
**File:** [phase-8-documentation.md](phase-8-documentation.md)
**Duration:** 1-2 days
**Risk:** Low

**What:** Comprehensive documentation
**Key Addition:** ✅ Migration guide for edge cases
**Deliverables:**
- Architecture docs
- Migration guide
- Updated README
- CHANGELOG
**Next:** **Ship it! 🚀**

---

## Critical Improvements (MUST READ)

### Security & Correctness
1. **SQL Injection Protection** (Phase 1)
   - Tests malicious input escaping
   - Prevents security vulnerabilities

2. **Parameter Binding Tests** (Phase 4)
   - Verifies parameter alignment
   - Prevents silent data corruption

3. **Strict FK Validation** (Phase 5)
   - Catches errors at startup
   - Prevents production failures

4. **Golden File Tests** (Phase 6)
   - Verifies backward compatibility
   - Detects SQL regressions

### Observability & DX
5. **EXPLAIN Mode** (Phase 7)
   - Verifies FK optimization working
   - Helps debug performance

6. **Metrics Collection** (Phase 7)
   - Tracks performance in production
   - Monitors optimization rates

7. **Migration Guide** (Phase 8)
   - Comprehensive edge case docs
   - Clear upgrade path

**See [CRITICAL_IMPROVEMENTS_SUMMARY.md](CRITICAL_IMPROVEMENTS_SUMMARY.md) for details**

---

## Execution Strategies

### Option A: Full Sequential (Safest)
Execute phases 1-8 sequentially, one at a time.

**Timeline:** 2-3 weeks
**Risk:** Low
**Best for:** Solo developer, careful approach

---

### Option B: Early Bug Fix (Recommended)
Ship phases 1-3 as v1.8.1 (bug fix), then continue with refactor.

**Week 1:** Phases 1-3 → **v1.8.1 released** (bug fix)
**Week 2-3:** Phases 4-8 → **v1.9.0 released** (full refactor)

**Timeline:** 2-3 weeks total
**Risk:** Low-Medium
**Best for:** Need bug fix ASAP, then full refactor

---

### Option C: Parallel Development (Fastest)
Run some phases in parallel (requires multiple developers).

**Timeline:** 1-2 weeks
**Risk:** Medium
**Best for:** Team of 2-3 developers

---

## File Structure

```
.phases/where-industrial-refactor/
├── INDEX.md                              # This file
├── README.md                             # Overview & architecture
├── IMPLEMENTATION_GUIDE.md               # Execution strategy
├── CRITICAL_IMPROVEMENTS_SUMMARY.md      # Security & correctness fixes
│
├── phase-1-canonical-representation.md   # [RED] WhereClause dataclasses
├── phase-2-dict-normalization.md         # [GREEN] Dict → WhereClause
├── phase-3-whereinput-normalization.md   # [GREEN] WhereInput → WhereClause (bug fix!)
├── phase-4-sql-generation-refactor.md    # [REFACTOR] Single SQL path
├── phase-5-explicit-fk-metadata.md       # [GREEN] FK relationships
├── phase-6-cleanup.md                    # [REFACTOR] Remove old code
├── phase-7-optimization.md               # [REFACTOR] Caching & perf
└── phase-8-documentation.md              # [QA] Docs & migration
```

---

## Quick Start Checklist

Before starting Phase 1:

- [ ] Read [README.md](README.md) - understand the problem and solution
- [ ] Read [CRITICAL_IMPROVEMENTS_SUMMARY.md](CRITICAL_IMPROVEMENTS_SUMMARY.md) - know what was added
- [ ] Read [IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md) - choose execution strategy
- [ ] Decide on Option A, B, or C (see above)
- [ ] Create feature branch: `feature/where-industrial-refactor`
- [ ] Create tracking epic/issue in GitHub
- [ ] Set up CI/CD for new test suites

During each phase:

- [ ] Read phase document completely
- [ ] Understand objective and context
- [ ] Implement changes from Implementation Steps
- [ ] **Implement critical improvements** (see summary)
- [ ] Run Verification Commands
- [ ] Check all Acceptance Criteria
- [ ] Review DO NOT list
- [ ] Run full test suite
- [ ] Code review (if team)
- [ ] Merge to feature branch

After all phases:

- [ ] All golden file tests pass
- [ ] All security tests pass
- [ ] All parameter binding tests pass
- [ ] FK validation works (strict mode)
- [ ] EXPLAIN mode logs query plans
- [ ] Metrics collection works
- [ ] Migration guide complete
- [ ] Update CHANGELOG
- [ ] Create pull request
- [ ] Deploy to staging
- [ ] Smoke tests
- [ ] Deploy to production
- [ ] Monitor for issues
- [ ] **Celebrate! 🎉**

---

## Testing Strategy Summary

Five levels of testing:

1. **Unit Tests** - WhereClause, FieldCondition
2. **Integration Tests** - Normalization functions
3. **Equivalence Tests** - Dict == WhereInput results
4. **Code Path Tests** - FK optimization used
5. **Performance Tests** - <0.5ms overhead

**Plus Critical Tests:**
- Security tests (SQL injection)
- Parameter binding tests (correctness)
- Golden file tests (regressions)

---

## Key Success Metrics

Technical:
- [ ] All tests pass (100%)
- [ ] Code coverage >85%
- [ ] 500+ lines removed
- [ ] Performance within ±5%
- [ ] FK optimization rate >80%

Quality:
- [ ] Zero "Unsupported operator" warnings
- [ ] FK optimization used 100% when eligible
- [ ] No GitHub issues for WHERE processing

User Experience:
- [ ] Zero breaking changes
- [ ] Zero required migrations
- [ ] Clear upgrade path
- [ ] Positive feedback

---

## Common Commands

```bash
# Run phase tests
uv run pytest tests/unit/test_where_clause.py -v              # Phase 1
uv run pytest tests/unit/test_where_normalization.py -v       # Phases 2-3
uv run pytest tests/integration/test_parameter_binding.py -v  # Phase 4
uv run pytest tests/regression/test_where_golden.py -v        # Phase 6
uv run pytest tests/performance/ -v -s                        # Phase 7

# Run critical tests
uv run pytest tests/unit/test_where_clause_security.py -v     # Security
uv run pytest tests/integration/test_parameter_binding.py -v  # Correctness
uv run pytest tests/regression/test_where_golden.py -v        # Regressions

# Run full suite
uv run pytest tests/ -v -x

# Check metrics
python -c "from fraiseql.where_metrics import WhereMetrics; print(WhereMetrics.get_stats())"

# Test EXPLAIN mode
uv run pytest tests/regression/test_nested_filter_id_field.py -v -s --log-cli-level=INFO
```

---

## Getting Help

- **Questions about phases?** Read phase documents thoroughly first
- **Questions about improvements?** See [CRITICAL_IMPROVEMENTS_SUMMARY.md](CRITICAL_IMPROVEMENTS_SUMMARY.md)
- **Questions about execution?** See [IMPLEMENTATION_GUIDE.md](IMPLEMENTATION_GUIDE.md)
- **Questions about architecture?** See [README.md](README.md) and Phase 8 docs

---

## Document Changelog

- 2025-12-10: Added critical improvements (security, correctness, observability)
- 2025-12-10: Initial phase plans created
- 2025-12-10: Created comprehensive index

---

**Ready to start? Begin with [README.md](README.md) → [CRITICAL_IMPROVEMENTS_SUMMARY.md](CRITICAL_IMPROVEMENTS_SUMMARY.md) → Phase 1**
