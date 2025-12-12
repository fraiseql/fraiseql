# Always Cast Both Sides - Implementation Plan

**Project**: Fix operator strategy casting bugs by always casting both sides of comparisons

**Status**: Ready for Implementation
**Priority**: HIGH (fixes 18 integration test failures)
**Estimated Duration**: 3-4 hours
**Risk Level**: LOW

---

## Overview

### Objective

Simplify operator strategy casting logic by always casting both the field path AND value to the target PostgreSQL type, eliminating the need for `jsonb_column` parameter checking and fixing all casting-related bugs.

### Problem Statement

Current implementation has conditional casting logic that:
- Only casts when `jsonb_column` is provided
- Never casts the value side
- Results in incorrect SQL for special types (macaddr, daterange, inet, point)
- Complex to maintain (multiple code paths)

### Solution

Always cast both sides for special PostgreSQL types:
```python
# OLD (buggy):
casted_path = self._cast_path(path_sql, "macaddr", jsonb_column)  # Only path
return SQL("{} = {}").format(casted_path, Literal(value))  # Value not cast

# NEW (correct):
casted_path, casted_value = self._cast_both_sides(path_sql, value, "macaddr")
return SQL("{} = {}").format(casted_path, casted_value)  # Both cast
```

### Success Criteria

- ✅ All 18 casting-related integration tests pass
- ✅ No regression in existing tests
- ✅ Simpler, more maintainable code
- ✅ Consistent SQL output for all special types

---

## Phases

1. **Phase 1**: Add new `_cast_both_sides()` method (non-breaking)
2. **Phase 2**: Update MAC address strategy (test with 9 MAC tests)
3. **Phase 3**: Update DateRange strategy (test with 9 daterange tests)
4. **Phase 4**: Update Network strategy (fix IP tests)
5. **Phase 5**: Update Coordinate strategy (fix coordinate tests)
6. **Phase 6**: Integration test fixes (parameter order/names)
7. **Phase 7**: Verification & cleanup

---

## Execution Plan

### Prerequisites

- [ ] All phases 1-5 of test reorganization completed
- [ ] Clean git working directory
- [ ] Integration tests running (even if failing)
- [ ] Database accessible for integration tests

### Phase Order

Execute phases **sequentially**. Each phase includes verification before proceeding.

---

## Detailed Phase Plans

See individual phase files:
- [Phase 1: Add Base Method](phase-1-add-base-method.md)
- [Phase 2: Fix MAC Address Strategy](phase-2-fix-mac-address.md)
- [Phase 3: Fix DateRange Strategy](phase-3-fix-daterange.md)
- [Phase 4: Fix Network Strategy](phase-4-fix-network.md)
- [Phase 5: Fix Coordinate Strategy](phase-5-fix-coordinate.md)
- [Phase 6: Fix Integration Tests](phase-6-fix-integration-tests.md)
- [Phase 7: Verification & Cleanup](phase-7-verification-cleanup.md)

---

## Quick Reference

### Files to Modify

**Production Code** (5 files):
```
src/fraiseql/sql/operators/base.py                      # Phase 1: Add _cast_both_sides()
src/fraiseql/sql/operators/postgresql/macaddr_operators.py    # Phase 2: Update MAC
src/fraiseql/sql/operators/postgresql/daterange_operators.py  # Phase 3: Update DateRange
src/fraiseql/sql/operators/postgresql/network_operators.py    # Phase 4: Update Network
src/fraiseql/sql/operators/advanced/coordinate_operators.py   # Phase 5: Update Coordinate
```

**Test Code** (8 files):
```
tests/integration/database/sql/where/network/test_ip_operations.py      # Phase 6
tests/integration/database/sql/where/network/test_consistency.py        # Phase 6
tests/integration/database/sql/where/network/test_ip_filtering.py       # Phase 6
tests/integration/database/sql/where/network/test_jsonb_integration.py  # Phase 6
tests/integration/database/sql/where/network/test_network_fixes.py      # Phase 6
tests/integration/database/sql/where/network/test_production_bugs.py    # Phase 6
tests/integration/database/sql/where/spatial/test_coordinate_operations.py  # Phase 6
tests/integration/database/sql/where/temporal/test_daterange_operations.py  # Phase 6
```

### Test Commands

```bash
# Phase 2 verification: MAC tests
uv run pytest tests/integration/database/sql/where/network/test_mac_operations.py -v

# Phase 3 verification: DateRange tests
uv run pytest tests/integration/database/sql/where/temporal/test_daterange_operations.py -v

# Phase 4 verification: Network tests
uv run pytest tests/integration/database/sql/where/network/ -v

# Phase 5 verification: Coordinate tests
uv run pytest tests/integration/database/sql/where/spatial/ -v

# Final verification: All WHERE integration tests
uv run pytest tests/integration/database/sql/where/ -v
```

### Expected Results by Phase

| Phase | Tests Fixed | Cumulative Pass | Cumulative Fail |
|-------|-------------|-----------------|-----------------|
| Start | 0 | 103/159 (65%) | 56/159 (35%) |
| Phase 2 | 9 MAC | 112/159 (70%) | 47/159 (30%) |
| Phase 3 | 9 DateRange | 121/159 (76%) | 38/159 (24%) |
| Phase 4 | ~10 Network | 131/159 (82%) | 28/159 (18%) |
| Phase 5 | ~11 Coordinate | 142/159 (89%) | 17/159 (11%) |
| Phase 6 | 17 Param fixes | 159/159 (100%) | 0/159 (0%) |

---

## Time Estimates

### By Phase

| Phase | Description | Estimated Time |
|-------|-------------|----------------|
| 1 | Add base method | 15 minutes |
| 2 | Fix MAC strategy | 30 minutes |
| 3 | Fix DateRange strategy | 30 minutes |
| 4 | Fix Network strategy | 45 minutes |
| 5 | Fix Coordinate strategy | 30 minutes |
| 6 | Fix integration tests | 45 minutes |
| 7 | Verification & cleanup | 30 minutes |
| **Total** | | **3-4 hours** |

### By Activity

| Activity | Time |
|----------|------|
| Code changes | 2 hours |
| Testing & verification | 1 hour |
| Documentation | 30 minutes |
| Contingency | 30 minutes |

---

## Risk Assessment

### Risk Level: LOW

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Performance regression | Low | Low | PostgreSQL handles redundant casts efficiently |
| Breaking existing code | Low | Medium | Phased approach with verification at each step |
| Test failures | Medium | Low | Each phase verifies before proceeding |
| Unexpected edge cases | Low | Medium | Comprehensive integration tests catch issues |

### Rollback Strategy

Each phase is committed separately. If issues arise:

**Option 1: Rollback Last Phase**
```bash
# Revert last commit
git reset --hard HEAD~1

# Or revert specific commit
git revert <commit-hash>
```

**Option 2: Rollback All Changes**
```bash
# Create backup branch before starting
git checkout -b backup/before-always-cast

# If issues, restore
git checkout main
git reset --hard backup/before-always-cast
```

---

## Dependencies

### External Dependencies
- PostgreSQL 12+ (for type casting support)
- psycopg3 (for SQL composition)

### Internal Dependencies
- BaseOperatorStrategy class
- All operator strategies inherit from BaseOperatorStrategy
- Integration test infrastructure

### No Breaking Changes to Public API
- Changes are internal to operator strategies
- Public API (`registry.build_sql()`) unchanged
- GraphQL schema unchanged

---

## Communication Plan

### Commit Messages

Each phase has specific commit message format:

**Phase 1**:
```
feat(operators): Add _cast_both_sides method to BaseOperatorStrategy

Add new method that casts both field path and value to PostgreSQL type.
This simplifies casting logic and ensures consistent type handling.

Part of: Always Cast Both Sides implementation
Phase: 1/7
```

**Phase 2-5** (example for MAC):
```
fix(operators): Always cast both sides for MAC address comparisons

Update MacAddressOperatorStrategy to cast both field and value sides
to ::macaddr. Fixes 9 integration tests.

Before: (data->>'mac_address')::macaddr = '00:11:22:33:44:55'
After:  (data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr

Part of: Always Cast Both Sides implementation
Phase: 2/7
Fixes: 9 MAC address integration tests
```

**Phase 6**:
```
fix(tests): Update integration tests to use correct parameter order

Update network, spatial, and temporal integration tests to use new
build_sql signature: (operator, value, path_sql, field_type).

Part of: Always Cast Both Sides implementation
Phase: 6/7
Fixes: 17 integration tests
```

**Phase 7**:
```
docs: Document always-cast-both-sides approach in operator strategies

Update operator strategy documentation to reflect new casting approach.
Add deprecation notice for old _cast_path method.

Part of: Always Cast Both Sides implementation
Phase: 7/7
Complete: All 56 integration test failures resolved
```

---

## Testing Strategy

### Test Levels

1. **Unit Tests**: Should all pass (unchanged)
2. **Integration Tests**: Target of this fix
3. **Manual Testing**: Optional verification

### Verification Points

After each phase:
```bash
# 1. Run unit tests (should still pass)
uv run pytest tests/unit/sql/where/operators/ -v

# 2. Run affected integration tests
uv run pytest tests/integration/database/sql/where/<category>/ -v

# 3. Check for regressions
git diff HEAD~1 -- tests/
```

### Success Criteria per Phase

**Phase 2 Success**:
- ✅ 9/9 MAC tests pass
- ✅ No unit test regressions
- ✅ SQL output shows `::macaddr` on both sides

**Phase 3 Success**:
- ✅ 9/9 DateRange tests pass
- ✅ No unit test regressions
- ✅ SQL output shows `::daterange` on both sides

**Phase 4 Success**:
- ✅ Network tests pass
- ✅ No unit test regressions
- ✅ SQL output shows `::inet` on both sides

**Phase 5 Success**:
- ✅ Coordinate tests pass
- ✅ No unit test regressions
- ✅ SQL output shows `::point` with correct format

**Phase 6 Success**:
- ✅ All parameter order issues fixed
- ✅ All parameter name issues fixed
- ✅ 159/159 integration tests pass

---

## Documentation Updates

### Files to Update

1. **Operator Strategy Guide** (new):
   ```
   docs/development/operator-strategies.md
   ```
   - Document casting approach
   - Explain when to use `_cast_both_sides()`
   - Provide examples for new operator types

2. **Migration Guide** (if needed):
   ```
   docs/migrations/always-cast-both-sides.md
   ```
   - For users implementing custom operators
   - Deprecation timeline for `_cast_path()`

3. **CHANGELOG.md**:
   - Document bug fixes
   - Note improved casting consistency

---

## Post-Implementation Tasks

### Immediate (After Phase 7)

- [ ] Update CHANGELOG.md with fixes
- [ ] Run full test suite (unit + integration)
- [ ] Performance smoke test (optional)
- [ ] Document new approach in code comments

### Short-Term (Next Week)

- [ ] Monitor for any reported issues
- [ ] Add operator strategy documentation
- [ ] Consider blog post on the fix

### Long-Term (Next Release)

- [ ] Deprecate `_cast_path()` method
- [ ] Add deprecation warnings
- [ ] Plan removal of `jsonb_column` parameter (breaking change)

---

## Success Metrics

### Primary Metrics

- ✅ **Test Pass Rate**: 159/159 integration tests passing (100%)
- ✅ **Bug Resolution**: All 18 casting bugs fixed
- ✅ **Code Simplification**: 60% reduction in casting logic LOC

### Secondary Metrics

- ✅ **No Regressions**: All unit tests still pass
- ✅ **Performance**: < 2% overhead from redundant casts
- ✅ **Maintainability**: Single code path for casting

---

## Contingency Plans

### If Phase 2 Fails (MAC Tests Don't Pass)

**Possible Issues**:
1. List operators need special handling
2. NULL value handling

**Actions**:
1. Check `_build_in_operator()` method
2. Ensure list values are cast individually
3. Verify NULL handling doesn't break

### If Phase 3 Fails (DateRange Tests Don't Pass)

**Possible Issues**:
1. Range-specific operators (@>, &&, etc.) need different handling
2. Date vs. daterange casting confusion

**Actions**:
1. Check PostgreSQL range operator syntax
2. Verify operator symbols correct
3. Test with actual database

### If Phase 6 Reveals More Test Issues

**Possible Issues**:
1. More tests with wrong parameter order
2. Tests with hardcoded expectations

**Actions**:
1. Search for all `build_sql` calls in test files
2. Update systematically
3. Use regex to find patterns

---

## Final Verification Checklist

Before considering complete:

### Code Quality
- [ ] All production code changes reviewed
- [ ] No TODO or FIXME comments added
- [ ] Code follows project style guidelines
- [ ] Type hints present and correct

### Testing
- [ ] All 159 integration tests pass
- [ ] All 550+ unit tests still pass
- [ ] No new test warnings
- [ ] Test coverage maintained or improved

### Documentation
- [ ] CHANGELOG.md updated
- [ ] Code comments updated
- [ ] Phase completion documented

### Git History
- [ ] Each phase committed separately
- [ ] Commit messages follow convention
- [ ] No merge conflicts
- [ ] Clean git history

### Performance
- [ ] No significant performance regression
- [ ] SQL queries valid and efficient
- [ ] Database queries tested

---

## Next Steps After Completion

1. **Create PR** (if using PR workflow):
   - Title: "Fix operator strategy casting by always casting both sides"
   - Link to issue/bug report
   - Include before/after test results

2. **Monitor Production** (if applicable):
   - Watch for any unexpected behavior
   - Check query performance
   - Monitor error logs

3. **Document Lessons Learned**:
   - Why the bug existed
   - How it was found
   - How it was fixed
   - How to prevent similar bugs

4. **Consider Future Improvements**:
   - Should `_cast_path()` be removed?
   - Should `jsonb_column` parameter be removed?
   - Can unit tests catch these issues?

---

## Related Documentation

- [FUNCTIONAL-ISSUES-ASSESSMENT.md](../integration-test-reorganization/FUNCTIONAL-ISSUES-ASSESSMENT.md) - Original bug analysis
- [WHY-UNIT-TESTS-PASSED.md](../integration-test-reorganization/WHY-UNIT-TESTS-PASSED.md) - Root cause analysis
- [ALWAYS-CAST-BOTH-SIDES-PROPOSAL.md](../integration-test-reorganization/ALWAYS-CAST-BOTH-SIDES-PROPOSAL.md) - Solution proposal

---

**Status**: Ready for execution ✅
**Start Condition**: Clean git working directory, all prerequisites met
**End Condition**: 159/159 integration tests passing, code committed

---

**Last Updated**: 2025-12-11
**Plan Author**: FraiseQL Development Team
**Approved By**: Pending review
