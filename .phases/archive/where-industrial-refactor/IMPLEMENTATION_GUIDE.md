# WHERE Industrial Refactor - Implementation Guide

## Quick Start

This refactor transforms FraiseQL's WHERE processing from a fragile multi-path system to an industrial-grade single-path architecture.

## Phases at a Glance

| Phase | Type | Duration | Risk | Description |
|-------|------|----------|------|-------------|
| 1 | RED | 1-2 days | Low | Define canonical `WhereClause` representation |
| 2 | GREEN | 2-3 days | Medium | Implement dict normalization |
| 3 | GREEN | 2-3 days | Medium | Implement WhereInput normalization (fixes bug) |
| 4 | REFACTOR | 2-3 days | High | Refactor SQL generation to use `WhereClause` |
| 5 | GREEN | 1-2 days | Low | Add explicit FK metadata |
| 6 | REFACTOR | 1 day | Low | Remove old code paths |
| 7 | REFACTOR | 1-2 days | Low | Performance optimization & caching |
| 8 | QA | 1-2 days | Low | Documentation & migration guide |

**Total:** 2-3 weeks for full industrial-grade implementation
**MVP:** Phases 1-4 (1-2 weeks for bug fix + core architecture)

## Recommended Execution Strategy

### Option A: Full Sequential (Safest)

Execute phases 1-8 sequentially, one phase at a time.

**Pros:**
- Safest approach
- Easy to track progress
- Clear rollback points

**Cons:**
- Slower (2-3 weeks)
- Can't ship bug fix early

**Timeline:**
- Week 1: Phases 1-3 (canonical repr + normalization)
- Week 2: Phases 4-6 (SQL refactor + cleanup)
- Week 3: Phases 7-8 (optimization + docs)

### Option B: Early Bug Fix (Recommended)

Ship phases 1-3 as v1.8.1 (bug fix), then continue with refactor.

**Phases 1-3 (Bug Fix Release):**
- Define `WhereClause`
- Implement normalization
- **Ship v1.8.1** with bug fix
- Feature flag: Use normalization for WhereInput only

**Phases 4-8 (Full Refactor):**
- Complete architecture refactor
- **Ship v1.9.0** with full refactor

**Pros:**
- Bug fix ships in 1 week
- Lower risk for bug fix
- Full refactor has more time

**Cons:**
- Two releases to manage
- Need feature flag

**Timeline:**
- Week 1: Phases 1-3 → **v1.8.1 released**
- Week 2-3: Phases 4-8 → **v1.9.0 released**

### Option C: Parallel Development (Fastest)

Run some phases in parallel (requires multiple developers).

**Parallel Groups:**
1. Phases 1-2 (one developer)
2. Phase 3 (another developer, depends on 1-2)
3. Phases 4-5 (after 3 complete)
4. Phases 6-8 (after 4-5 complete)

**Pros:**
- Fastest (1-2 weeks)
- Efficient use of team

**Cons:**
- Requires coordination
- Merge conflicts possible

**Timeline:**
- Week 1: Phases 1-5 (parallel)
- Week 2: Phases 6-8 (cleanup)

## Execution Checklist

### Before Starting

- [ ] Read all phase documents
- [ ] Review architecture diagram in README.md
- [ ] Set up local development environment
- [ ] Create tracking epic/issue
- [ ] Decide on execution strategy (A, B, or C)
- [ ] Create feature branch: `feature/where-industrial-refactor`

### For Each Phase

- [ ] Read phase document completely
- [ ] Understand objective and context
- [ ] Create phase branch (if using git-flow)
- [ ] Implement changes from Implementation Steps
- [ ] Run Verification Commands
- [ ] Check all Acceptance Criteria
- [ ] Review DO NOT list
- [ ] Run full test suite
- [ ] Code review (if team)
- [ ] Merge to feature branch
- [ ] Update progress tracker

### After All Phases

- [ ] Run full test suite
- [ ] Run performance benchmarks
- [ ] Update CHANGELOG
- [ ] Update version number
- [ ] Create pull request
- [ ] Team review
- [ ] Merge to main
- [ ] Tag release
- [ ] Deploy to staging
- [ ] Smoke tests on staging
- [ ] Deploy to production
- [ ] Monitor for issues
- [ ] Celebrate! 🎉

## Testing Strategy

### Per-Phase Testing

Each phase has specific tests in Verification Commands.

### Integration Testing

After phases 1-4 complete:

```bash
# Run full regression suite
uv run pytest tests/regression/ -v

# Run with different PostgreSQL versions
uv run pytest tests/ --postgresql-version=15
uv run pytest tests/ --postgresql-version=16

# Run with different Python versions
uv run tox  # if using tox
```

### Performance Testing

After phase 7:

```bash
# Run performance benchmarks
uv run pytest tests/performance/ -v

# Compare with baseline
# Store results for regression tracking
```

### User Acceptance Testing

Before release:

1. Test with real PrintOptim queries
2. Test with FraiseQL examples
3. Test migration guide steps
4. Verify documentation accuracy

## Rollout Strategy

### v1.8.1 (Bug Fix) - If Using Option B

```python
# Feature flag approach
USE_NEW_NORMALIZATION = os.getenv("FRAISEQL_NEW_NORMALIZATION", "false").lower() == "true"

def _build_where_clause(self, view_name: str, **kwargs: Any):
    where_obj = kwargs.pop("where", None)

    if where_obj:
        # New path for WhereInput (bug fix)
        if USE_NEW_NORMALIZATION and hasattr(where_obj, "_to_whereinput_dict"):
            clause = self._normalize_where(where_obj, view_name, table_columns)
            sql, params = clause.to_sql()
            # ... use SQL ...
        else:
            # Old path (dict and legacy WhereInput)
            # ... existing code ...
```

**Rollout:**
1. Deploy with flag off
2. Enable for canary users
3. Monitor for issues
4. Enable for all users
5. Ship v1.9.0 with full refactor (flag removed)

### v1.9.0 (Full Refactor)

```python
# No feature flag, full refactor
def _build_where_clause(self, view_name: str, **kwargs: Any):
    where_obj = kwargs.pop("where", None)

    if where_obj:
        # Single code path
        clause = self._normalize_where(where_obj, view_name, table_columns)
        sql, params = clause.to_sql()
        # ... use SQL ...
```

**Rollout:**
1. Release as v1.9.0
2. Update documentation
3. Announce in release notes
4. Monitor GitHub issues
5. Respond to user feedback

## Risk Mitigation

### High-Risk Phases

**Phase 4 (SQL Generation Refactor)** is highest risk:
- Touches core query logic
- Could break existing queries
- Hard to test exhaustively

**Mitigations:**
1. Extensive test coverage before starting
2. Feature flag for gradual rollout
3. Comprehensive logging
4. Easy rollback plan
5. Canary deployment
6. Monitoring and alerts

### Rollback Plan

**If issues found in production:**

1. **Immediate:** Disable feature flag (if using)
2. **Short-term:** Revert to previous version
3. **Long-term:** Fix issue, re-deploy

**Rollback procedure:**
```bash
# Option 1: Feature flag (if available)
export FRAISEQL_NEW_WHERE=false

# Option 2: Downgrade version
pip install fraiseql==1.8.0

# Option 3: Git revert
git revert <commit-hash>
git push
```

## Communication Plan

### Internal Communication

**Before starting:**
- Share plan with team
- Get buy-in on approach
- Assign responsibilities

**During development:**
- Daily standup updates
- Phase completion notifications
- Blocker escalation

**After completion:**
- Demo to team
- Share metrics/results
- Retrospective

### External Communication

**v1.8.1 Release (Bug Fix):**
```markdown
## v1.8.1 - Bug Fix Release

### Fixed
- Nested object filters now work correctly with WhereInput objects
- No more "Unsupported operator: id" warnings

### Migration
- No changes required - fully backward compatible
- Existing code continues to work
```

**v1.9.0 Release (Full Refactor):**
```markdown
## v1.9.0 - WHERE Architecture Refactor

### Added
- Explicit FK metadata for better performance
- Comprehensive WHERE clause normalization

### Changed
- Internal refactor of WHERE processing (backward compatible)
- 50% reduction in WHERE-related code

### Migration
- Recommended: Add explicit fk_relationships to register_type_for_view()
- See migration guide: docs/where-migration-guide.md
```

## Success Metrics

### Technical Metrics

- [ ] All tests pass (100%)
- [ ] Code coverage maintained or improved (>85%)
- [ ] 500+ lines of code removed
- [ ] No regressions in functionality
- [ ] Performance within ±5% of baseline
- [ ] Normalization overhead <0.5ms
- [ ] Cache hit rate >90% for repeated queries

### Quality Metrics

- [ ] Zero "Unsupported operator" warnings in production
- [ ] FK optimization used for 100% of eligible queries
- [ ] No new GitHub issues related to WHERE processing
- [ ] Documentation completeness: 100%

### User Experience Metrics

- [ ] Zero breaking changes
- [ ] Zero required migrations
- [ ] Clear migration path for new features
- [ ] Positive user feedback

## Troubleshooting

### Common Issues

**"Tests failing in Phase 1"**
- Expected! Phase 1 is RED (tests should fail)
- Verify tests are skipped with `@pytest.mark.skip`
- Tests pass in Phase 2-3 (GREEN)

**"Can't merge Phase 3"**
- Ensure Phase 1-2 complete first
- Check for conflicts in db.py
- Rebase on latest main

**"Performance regression in Phase 4"**
- Check SQL generated is identical to before
- Profile to find hot spots
- Verify caching working (Phase 7)
- Compare database query plans

**"Documentation unclear"**
- Review examples in phase documents
- Check architecture diagram
- Ask questions in team chat

## Resources

### Documentation

- Phase plans: `.phases/where-industrial-refactor/phase-*.md`
- Architecture: `README.md` (this directory)
- Code examples: Each phase document
- Test examples: `tests/unit/test_where_*.py`

### Code References

- Current WHERE logic: `src/fraiseql/db.py:1406-2920`
- Current bug location: `src/fraiseql/db.py:2871-2920`
- Test suite: `tests/regression/test_nested_filter_id_field.py`

### External Resources

- TDD methodology: [Test Driven Development](https://testdriven.io/)
- Refactoring patterns: [Refactoring.guru](https://refactoring.guru/)
- PostgreSQL JSONB: [PostgreSQL JSON docs](https://www.postgresql.org/docs/current/datatype-json.html)

## Questions?

- **GitHub Issues:** Create issue with `refactor` label
- **Team Chat:** #fraiseql-refactor channel
- **Email:** maintainer@fraiseql.dev

---

**Good luck! This refactor will make FraiseQL's WHERE processing truly industrial-grade. 🚀**
