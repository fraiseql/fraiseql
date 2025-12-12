# Always Cast Both Sides - Quick Start Guide

**TL;DR**: Execute 7 phases sequentially to fix all 56 integration test failures by always casting both sides of PostgreSQL type comparisons.

---

## Prerequisites

- Clean git working directory
- All phases of test reorganization completed
- Database accessible for integration tests

---

## Execution Order

Execute phases **in order**. Each phase builds on previous phases.

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
  (15min)   (30min)   (30min)   (45min)   (30min)   (45min)   (30min)

  Total: 3-4 hours
```

---

## Quick Commands

### Phase 1: Add Base Method
```bash
# 1. Add _cast_both_sides() to BaseOperatorStrategy
# 2. Test syntax
python3 -m py_compile src/fraiseql/sql/operators/base.py

# 3. Verify
uv run pytest tests/unit/sql/where/operators/ -v

# 4. Commit
git add src/fraiseql/sql/operators/base.py
git commit -m "feat(operators): Add _cast_both_sides method..."
```

### Phase 2: Fix MAC Address
```bash
# 1. Update MacAddressOperatorStrategy
# 2. Test
uv run pytest tests/integration/database/sql/where/network/test_mac_operations.py -v

# Expected: 9/9 tests pass ✅

# 3. Commit
git add src/fraiseql/sql/operators/postgresql/macaddr_operators.py
git commit -m "fix(operators): Always cast both sides for MAC..."
```

### Phase 3: Fix DateRange
```bash
# 1. Update DateRangeOperatorStrategy
# 2. Test
uv run pytest tests/integration/database/sql/where/temporal/test_daterange_operations.py -v

# Expected: 5-6 more tests pass

# 3. Commit
git add src/fraiseql/sql/operators/postgresql/daterange_operators.py
git commit -m "fix(operators): Always cast both sides for DateRange..."
```

### Phase 4: Fix Network
```bash
# 1. Update NetworkOperatorStrategy
# 2. Test
uv run pytest tests/integration/database/sql/where/network/ -v

# Expected: Some more tests pass

# 3. Commit
git add src/fraiseql/sql/operators/postgresql/network_operators.py
git commit -m "fix(operators): Always cast both sides for IP addresses..."
```

### Phase 5: Fix Coordinate
```bash
# 1. Update CoordinateOperatorStrategy
# 2. Test
uv run pytest tests/integration/database/sql/where/spatial/ -v

# Expected: 11/11 coordinate tests pass ✅

# 3. Commit
git add src/fraiseql/sql/operators/advanced/coordinate_operators.py
git commit -m "fix(operators): Always cast coordinates to ::point..."
```

### Phase 6: Fix Integration Tests
```bash
# 1. Fix parameter order in test files (manual)
# 2. Fix parameter names (can use sed)

sed -i 's/op="/operator="/g' tests/integration/database/sql/where/temporal/test_daterange_operations.py
sed -i 's/val="/value="/g' tests/integration/database/sql/where/temporal/test_daterange_operations.py

# 3. Test ALL
uv run pytest tests/integration/database/sql/where/ -v

# Expected: 159/159 tests pass ✅

# 4. Commit
git add tests/integration/database/sql/where/
git commit -m "fix(tests): Update integration tests to use correct build_sql signature..."
```

### Phase 7: Verification
```bash
# 1. Run full test suite
uv run pytest tests/unit/ -v
uv run pytest tests/integration/ -v

# 2. Update CHANGELOG.md
# 3. Create documentation
# 4. Final commit
git add CHANGELOG.md docs/ .phases/
git commit -m "docs: Document always-cast-both-sides approach..."
```

---

## Verification Points

After each phase, verify:
```bash
# Current phase tests pass
uv run pytest <phase-specific-tests> -v

# No unit test regression
uv run pytest tests/unit/sql/where/operators/ -v

# Git status clean
git status
```

---

## Expected Progress

| After Phase | Tests Passing | Cumulative Fixes |
|-------------|---------------|------------------|
| Start | 103/159 (65%) | 0 |
| Phase 1 | 103/159 (65%) | 0 (setup only) |
| Phase 2 | 112/159 (70%) | +9 MAC tests |
| Phase 3 | 117/159 (74%) | +5 DateRange tests |
| Phase 4 | ~127/159 (80%) | +10 Network tests |
| Phase 5 | ~142/159 (89%) | +11 Coordinate tests |
| Phase 6 | 159/159 (100%) ✅ | +17 Param fixes |
| Phase 7 | 159/159 (100%) ✅ | Verified |

---

## Troubleshooting

### Tests Still Failing After Phase?

**Check**:
1. Did previous phase commit successfully?
2. Are you using the correct test command?
3. Is the database accessible?

**Debug**:
```bash
# Test SQL generation directly
python3 << 'PYEOF'
from fraiseql.sql.operators import get_default_registry
from fraiseql.types import MacAddress
from psycopg.sql import SQL

registry = get_default_registry()
sql = registry.build_sql("eq", "00:11:22:33:44:55", SQL("data->>'mac'"), field_type=MacAddress)
print(sql.as_string(None))
# Should show both sides cast: ::macaddr appears twice
PYEOF
```

### Merge Conflicts?

All changes are in different files, should not conflict. If conflicts arise:
```bash
# Show what conflicts
git status

# Resolve manually, then:
git add <resolved-files>
git commit
```

### Want to Rollback?

```bash
# Rollback last phase
git reset --hard HEAD~1

# Or rollback specific commits
git log --oneline | head -10  # Find commit hash
git revert <hash>
```

---

## Success Criteria

✅ **Phase 1**: New method added, syntax valid
✅ **Phase 2**: 9 MAC tests pass
✅ **Phase 3**: 5+ DateRange tests pass
✅ **Phase 4**: Network tests improve
✅ **Phase 5**: 11 Coordinate tests pass
✅ **Phase 6**: ALL 159 tests pass
✅ **Phase 7**: Documentation complete

---

## Time Estimates

**Fast track** (experienced, no issues): 2.5 hours
**Normal** (following guide carefully): 3-4 hours
**With issues** (debugging, retries): 4-5 hours

---

## Files Changed Summary

**Production code**: 5 files
- `base.py` - New methods
- `macaddr_operators.py` - Updated
- `daterange_operators.py` - Updated
- `network_operators.py` - Updated
- `coordinate_operators.py` - Updated

**Test code**: 8 files
- All files in `tests/integration/database/sql/where/network/`
- `tests/integration/database/sql/where/temporal/test_daterange_operations.py`
- `tests/integration/database/sql/where/spatial/test_coordinate_operations.py`

**Documentation**: 3+ files
- `CHANGELOG.md`
- `docs/development/operator-strategies.md`
- `.phases/always-cast-both-sides/*`

---

## Post-Implementation

After all phases complete:

1. ✅ All tests passing
2. ✅ Code simplified
3. ✅ Documentation updated
4. ✅ Ready for production

**Optional**: Create PR, deploy to staging, monitor performance

---

## Getting Help

If stuck:
1. Check phase-specific documentation (phase-N-*.md)
2. Review troubleshooting section
3. Check git log for what changed
4. Run verification commands to isolate issue
5. Review WHY-UNIT-TESTS-PASSED.md for context

---

## Final Verification

```bash
# The moment of truth
uv run pytest tests/integration/database/sql/where/ -v

# Should see:
# ======================== 159 passed ======================== ✅
```

---

**Ready to start? Begin with Phase 1!**

📂 See: `phase-1-add-base-method.md`
