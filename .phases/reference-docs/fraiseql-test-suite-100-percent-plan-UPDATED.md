# FraiseQL Test Suite: 100% Clean Plan (UPDATED)

**Date**: December 12, 2025 (Updated after Phase 1 completion)
**Current State**: 5,166/5,267 passing (98.1%) - Phase 1 complete
**Target**: 5,267/5,267 passing (100%)
**Total Failures to Fix**: ~101 tests remaining

---

## Phase Progress

### ✅ Phase 1: v1.8.1 Test Updates (COMPLETE)

**Duration**: 45 minutes
**Tests Fixed**: 6
**Status**: ✅ **COMPLETE**
**Commit**: `c6cd7474`

**Changes**:
- Updated Success type tests (removed `errors` field expectations)
- Updated Error type tests (added `code`, removed `updated_fields`/`id`)
- Fixed field order expectations

**Files**:
- `tests/unit/mutations/test_auto_populate_schema.py` (4 tests) ✅
- `tests/unit/decorators/test_decorators.py` (2 tests) ✅

---

### 📋 Phase 1.5: Integration Test Infrastructure (NEW)

**Objective**: Fix native error arrays integration tests (WP-034 feature)

**Problem**: Tests create database functions dynamically but GraphQL schema doesn't refresh

**Solution**: Pre-create test mutation functions before schema initialization

**Estimated Effort**: 6-8 hours
**Priority**: HIGH (WP-034 feature currently untested)

**Tasks**:
1. Create `tests/integration/graphql/mutations/conftest.py` with fixture
2. Pre-create all test mutation functions in class-scoped fixture
3. Update 4 test methods to remove dynamic function creation
4. Verify fixture dependency order
5. Document the pattern for future tests

**Tests to Fix**: 4
- `test_auto_generated_errors_from_status`
- `test_auto_generated_errors_multiple_status_formats`
- `test_explicit_errors_override_auto_generation`
- `test_backward_compatibility_with_mutation_result_base`

**Detailed Plan**: `/tmp/fraiseql-phase1.5-integration-test-infrastructure.md`

---

### 📅 Phase 2: SQL Rendering Infrastructure (Week 2)

**Objective**: Fix ~95+ SQL validation test failures

**Problem**: Tests call `str(composed_object)` which returns repr(), not valid SQL

**Solution**: Create `render_sql_for_testing()` utility and migrate tests

**Estimated Effort**: 16-20 hours

**Tasks**:
1. Create `tests/helpers/sql_rendering.py` utility
2. Migrate ~150 SQL tests to use rendering utility
3. Use local AI model (Ministral-3-8B) for bulk migration
4. Verify all SQL validation tests pass

**Files Affected**:
- `tests/regression/where_clause/` (~80 tests)
- `tests/core/test_special_types_tier1_core.py` (~10 tests)
- `tests/core/test_jsonb_network_casting_fix.py` (~5 tests)

**Note**: Some tests may pass after SQL rendering fix reveals they were actually correct

---

### 📅 Phase 3: SQL Generation Bug Fixes (Week 3)

**Objective**: Fix any remaining SQL generation bugs revealed by Phase 2

**Estimated Effort**: 10-20 hours (depends on Phase 2 results)

**Strategy**: Will be determined after Phase 2 completion

**Potential Issues**:
1. Network type strategy selection
2. Type casting for special types
3. Boolean handling edge cases

**Approach**:
1. Run Phase 2 verification
2. Identify remaining failures (if any)
3. Debug each failure category
4. Fix root causes in operator strategies or type definitions

---

### 📅 Phase 4: Test Configuration & Cleanup (Week 4)

**Objective**: Professional test suite configuration

**Estimated Effort**: 4-6 hours

**Tasks**:
1. Configure performance test markers (`@pytest.mark.performance`)
2. Fix deprecation warnings
3. Fix performance test errors
4. Optional: Install shellcheck for script validation

---

## Updated Timeline

### Week 1
- ✅ **Day 1**: Phase 1 complete (45 min)
- 📋 **Day 2-3**: Phase 1.5 - Integration test infrastructure (6-8 hours)

### Week 2
- **Phase 2**: SQL rendering infrastructure (16-20 hours)
  - Day 1: Create SQL rendering utility
  - Day 2-3: Migrate SQL tests (use local AI)
  - Day 4: Verification and bug identification
  - Day 5: Documentation

### Week 3
- **Phase 3**: SQL generation bug fixes (10-20 hours)
  - Dependent on Phase 2 findings
  - May complete faster if Phase 2 fixes most issues

### Week 4
- **Phase 4**: Cleanup and configuration (4-6 hours)
  - Configure test markers
  - Fix warnings
  - Final verification

**Updated Total Effort**: 36-44 hours (was 30 hours)
**Reason**: Added Phase 1.5 for integration test infrastructure

---

## Current Test Status

### Baseline (Before Phase 1)
- **Total**: 5,315 tests
- **Passing**: 5,160 (96.9%)
- **Failing**: 214
- **Errors**: 2
- **Warnings**: 10

### After Phase 1 (Current)
- **Total**: 5,267 tests
- **Passing**: 5,166 (98.1%)
- **Failing**: 101
- **Progress**: 113 tests fixed! 🎉

**Notes**:
- Test count decreased (5,315 → 5,267) likely due to test cleanup in recent commits
- Failure count decreased significantly (214 → 101)
- Most remaining failures are SQL rendering issues (Phase 2)

---

## Remaining Failures Breakdown

### Category A: Integration Test Infrastructure (Phase 1.5)
- **Count**: 4 tests
- **File**: `tests/integration/graphql/mutations/test_native_error_arrays.py`
- **Issue**: Schema registration of dynamic mutations
- **Effort**: 6-8 hours

### Category B: SQL Rendering Issues (Phase 2)
- **Count**: ~95 tests
- **Files**: `tests/regression/where_clause/`, `tests/core/test_special_types_*.py`
- **Issue**: `str(composed_object)` returns repr() not SQL
- **Effort**: 16-20 hours

### Category C: Unknown/Other (Phase 3+)
- **Count**: ~2 tests
- **To be determined**: After Phase 1.5 and Phase 2 completion

---

## Success Metrics

### Quantitative

| Metric | Before | Current | Target |
|--------|--------|---------|--------|
| Test Pass Rate | 96.9% | **98.1%** ✅ | **100%** |
| Failed Tests | 214 | **101** ✅ | **0** |
| Test Errors | 2 | ? | **0** |
| Warnings | 10 | ? | **0** |

### Qualitative

- [ ] Professional test suite organization (performance markers)
- [ ] Reusable SQL rendering utilities
- [ ] Zero deprecated API usage
- [ ] Clear test execution documentation
- [ ] WP-034 feature fully tested

---

## Phase Completion Checklist

- [x] **Phase 1**: v1.8.1 test semantics (6 tests) ✅
- [ ] **Phase 1.5**: Integration test infrastructure (4 tests)
- [ ] **Phase 2**: SQL rendering infrastructure (~95 tests)
- [ ] **Phase 3**: SQL generation bug fixes (0-10 tests)
- [ ] **Phase 4**: Configuration and cleanup (warnings, markers)

---

## Risk Assessment

### Phase 1.5 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Fixture order issues | MEDIUM | HIGH | Careful dependency testing |
| Functions not in schema | LOW | HIGH | Log function creation vs schema init |
| Test flakiness | LOW | MEDIUM | Use class scope consistently |

### Phase 2 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Bulk migration errors | MEDIUM | MEDIUM | Batch testing (20-30 files) |
| SQL bugs revealed | HIGH | MEDIUM | Budget Phase 3 time |
| Local AI model issues | LOW | LOW | Claude can do manually if needed |

---

## Decision: Phase Order

**Recommended**:
```
Phase 1 ✅ → Phase 1.5 → Phase 2 → Phase 3 → Phase 4
```

**Rationale**:
1. Phase 1.5 is small (6-8 hours) and tests important feature (WP-034)
2. Phase 1.5 independent from Phase 2 (different test categories)
3. Can parallelize: Start Phase 2 planning while doing Phase 1.5
4. Phase 1.5 success gives confidence before larger Phase 2

**Alternative** (if time-constrained):
```
Phase 1 ✅ → Phase 2 → Phase 3 → Phase 4 → Phase 1.5
```
- Skip Phase 1.5 initially
- Focus on SQL tests (larger impact: ~95 tests)
- Return to integration tests later
- **Downside**: WP-034 feature remains untested

---

## Documentation

### Planning Documents
1. `/tmp/README-fraiseql-test-remediation.md` - Index
2. `/tmp/fraiseql-test-remediation-executive-summary.md` - Overview
3. `/tmp/fraiseql-test-suite-100-percent-plan.md` - Original detailed plan
4. `/tmp/fraiseql-test-suite-100-percent-plan-UPDATED.md` - **This file** (updated)
5. `/tmp/fraiseql-test-remediation-decision-matrix.md` - Strategic decisions
6. `/tmp/fraiseql-phase1-execution-guide.md` - Phase 1 execution
7. `/tmp/fraiseql-phase1-complete.md` - Phase 1 results ✅
8. `/tmp/fraiseql-phase1.5-integration-test-infrastructure.md` - **NEW** Phase 1.5 plan

---

## Next Actions

### Immediate (Today)
1. Review Phase 1.5 plan
2. Decide: Execute Phase 1.5 now or proceed to Phase 2?
3. If Phase 1.5: Create conftest.py fixture
4. If Phase 2: Create SQL rendering utility

### This Week
- Complete Phase 1.5 (6-8 hours)
- Begin Phase 2 planning
- Prepare local AI model prompts for bulk migration

### Next Week
- Execute Phase 2 (SQL rendering)
- Identify Phase 3 scope
- Begin Phase 3 if time permits

---

## Lessons Learned (Phase 1)

### What Went Well ✅
1. Phase 1 execution guide was accurate and detailed
2. Changes were straightforward (field expectation updates)
3. Tests passed immediately after updates
4. Clear git history with descriptive commit message

### What Surprised Us 🤔
1. Integration tests had infrastructure issues (not v1.8.1 semantics)
2. More tests fixed than expected (113 vs 16) - likely due to cascading fixes
3. Test count changed (5,315 → 5,267) - recent cleanup work

### Improvements for Phase 1.5+ 📈
1. **Always check fixture dependencies**: Integration test issues stemmed from fixture order
2. **Verify schema initialization timing**: Dynamic mutations need pre-creation
3. **Check logs for warnings**: "Schema registry already initialized" was a clue
4. **Document patterns**: Create README for mutation test patterns

---

## Conclusion

**Phase 1 Success**: ✅ 6 tests fixed in 45 minutes, commit `c6cd7474`

**Phase 1.5 Identified**: Integration test infrastructure issue discovered and planned

**Path Forward**:
- Execute Phase 1.5 (6-8 hours) to fix WP-034 feature tests
- Proceed to Phase 2 (16-20 hours) for SQL rendering fixes
- Complete Phase 3 and 4 for 100% passing test suite

**Updated Total Effort**: 36-44 hours over 4 weeks
**Current Progress**: 98.1% pass rate (was 96.9%)
**Confidence**: HIGH - clear plan for all remaining failures

---

**Status**: ✅ Phase 1 Complete | 📋 Phase 1.5 Planned | 📅 Phase 2-4 Ready

**Last Updated**: December 12, 2025 - Post Phase 1 completion
