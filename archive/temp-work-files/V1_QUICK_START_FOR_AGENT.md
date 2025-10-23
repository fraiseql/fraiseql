# V1.0 Release - Quick Start for Agent

**Read this first!** This is your TL;DR guide to execute the v1.0 release.

---

## 📋 What You Need to Know

### Current Status
- ✅ 3,551 tests passing
- ✅ 0 tests skipped
- ❌ 5 tests failing (need fixing)
- 📁 25+ planning docs cluttering root (need archiving)

### Your Mission
Fix 5 tests → Clean docs → Release v1.0.0

### Time Required
4-5 days total

---

## 🚀 Quick Start (3 Steps)

### 1️⃣ Read the Plan (10 minutes)
```bash
cd /home/lionel/code/fraiseql
cat V1_RELEASE_PREPARATION_PLAN.md
```

This 1,000+ line document has **everything** you need:
- Detailed bug fix steps
- Complete documentation cleanup
- Full release process
- Pre-written commit messages

### 2️⃣ Start Phase 1 (Fix Bugs)
```bash
# Run failing tests to understand them
uv run pytest tests/integration/database/repository/test_dynamic_filter_construction.py::TestDynamicFilterConstruction::test_complex_nested_dict_filters -vv --tb=long

uv run pytest tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py -vv --tb=long
```

Follow **Phase 1A** and **Phase 1B** in the plan.

### 3️⃣ Follow the Plan Sequentially
- Phase 1: Fix bugs (2-3 days)
- Phase 2: Clean docs (1 day)
- Phase 3: Clean code (4 hours)
- Phase 4: Validate (3 hours)
- Phase 5: Release (2 hours)

---

## 🎯 The 5 Failing Tests

### Group 1: PostgreSQL Placeholder Format (2 tests)
**Error**: `only '%s', '%b', '%t' are allowed as placeholders, got '%m'`

**Location**:
- `test_complex_nested_dict_filters`
- `test_production_mixed_filtering_comprehensive`

**Likely Cause**: String formatting bug in SQL generation

**Fix Approach**: Search for placeholder generation code, ensure only `%s` is used

**Files to Check**:
- `src/fraiseql/sql/where/core/sql_builder.py`
- `src/fraiseql/sql/graphql_where_generator.py`

---

### Group 2: Hybrid Table Nested Object Filtering (3 tests)
**Error**: `Expected 0 allocations for machine1, but got 3`

**Location**:
- `test_nested_object_filter_on_hybrid_table`
- `test_nested_object_filter_with_results`
- `test_multiple_nested_object_filters`

**Likely Cause**: Nested object filter not being applied on hybrid tables

**Fix Approach**: Check nested object detection logic in `_convert_dict_where_to_sql()`

**Files to Check**:
- `src/fraiseql/db.py` (lines 758-822)
- `src/fraiseql/sql/where/core/sql_builder.py`

---

## 📚 Key Documents

### Must Read (in order)
1. **V1_QUICK_START_FOR_AGENT.md** ← You are here
2. **V1_RELEASE_SUMMARY.md** - Executive overview
3. **V1_RELEASE_PREPARATION_PLAN.md** - Complete implementation guide

### Reference
- **CURRENT_TEST_STATUS_AND_NEXT_PHASES.md** - What was done in Phases 1-6
- **CHANGELOG.md** - Version history
- **pyproject.toml** - Current version (0.11.5)

---

## ✅ Success Checklist

Before you start, verify:
- [ ] You have the git repo: `/home/lionel/code/fraiseql`
- [ ] You can run tests: `uv run pytest --version`
- [ ] You understand the 5 failing tests
- [ ] You've read the V1_RELEASE_PREPARATION_PLAN.md

After Phase 1, you should have:
- [ ] All tests passing: `uv run pytest` → 3,556 passing, 0 failing
- [ ] Bugs fixed and committed
- [ ] No regressions introduced

After Phase 2, you should have:
- [ ] 25+ docs moved to `archive/session-docs/`
- [ ] VERSION_STATUS.md created
- [ ] CHANGELOG.md updated for v1.0.0
- [ ] README.md updated for v1.0.0

After Phase 5, you should have:
- [ ] Version bumped to 1.0.0
- [ ] Git tagged v1.0.0
- [ ] Published to PyPI
- [ ] GitHub release created

---

## 🚨 Common Pitfalls to Avoid

### When Fixing Bugs
❌ **Don't**: Make changes without understanding the root cause
✅ **Do**: Reproduce the bug, understand it, then fix it

❌ **Don't**: Skip regression testing
✅ **Do**: Run related tests after each fix

### When Cleaning Docs
❌ **Don't**: Delete files permanently
✅ **Do**: Archive to `archive/session-docs/`

❌ **Don't**: Break existing links
✅ **Do**: Update references when moving files

### When Releasing
❌ **Don't**: Skip validation steps
✅ **Do**: Run full test suite before releasing

❌ **Don't**: Publish to PyPI without testing
✅ **Do**: Test with fresh install first

---

## 🆘 If You Get Stuck

### Phase 1 (Bugs)
1. Read the test file to understand expectations
2. Add debug logging to trace SQL generation
3. Run test with `-vv --tb=long -s` for full output
4. Check git history: `git log --oneline --grep="filter"`

### Phase 2 (Docs)
1. Don't overthink - just archive to `archive/session-docs/`
2. Use the exact file moves specified in the plan
3. Commit after each section (2A, 2B, 2C, etc.)

### Phase 5 (Release)
1. Follow the plan step-by-step
2. Don't skip steps (especially validation)
3. Use Test PyPI before production PyPI
4. Verify installation in fresh environment

---

## 💡 Pro Tips

### Speed Up Phase 1
```bash
# Run only failing tests while debugging
uv run pytest tests/integration/database/repository/test_dynamic_filter_construction.py tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py tests/regression/where_clause/test_industrial_where_clause_generation.py -v

# Add breakpoint for debugging
import pdb; pdb.set_trace()
```

### Speed Up Phase 2
```bash
# Batch move files with shell script
#!/bin/bash
mkdir -p archive/session-docs/2025-10-22-v1-prep
mv AGENT_*.md COMPREHENSIVE_*.md CURRENT_*.md archive/session-docs/2025-10-22-v1-prep/
# ... etc
```

### Verify Progress
```bash
# After each phase, check status
git status
git log --oneline -5
uv run pytest --tb=short  # Should have fewer failures
```

---

## 📊 Progress Tracking

Use git tags to mark milestones:
```bash
# After Phase 1
git tag phase-1-bugs-fixed
git push origin phase-1-bugs-fixed

# After Phase 2
git tag phase-2-docs-cleaned
git push origin phase-2-docs-cleaned

# After Phase 3
git tag phase-3-code-cleaned
git push origin phase-3-code-cleaned

# After Phase 4
git tag phase-4-validated
git push origin phase-4-validated

# Final release
git tag v1.0.0
git push origin v1.0.0
```

---

## 🎯 Your Daily Goals

### Day 1
- [ ] Read all documentation
- [ ] Understand the 5 failing tests
- [ ] Start Phase 1A (PostgreSQL bug)

### Day 2
- [ ] Complete Phase 1A
- [ ] Start Phase 1B (Hybrid tables)

### Day 3
- [ ] Complete Phase 1B
- [ ] Start Phase 2 (Documentation)

### Day 4
- [ ] Complete Phase 2
- [ ] Complete Phase 3 (Code cleanup)
- [ ] Start Phase 4 (Validation)

### Day 5
- [ ] Complete Phase 4
- [ ] Complete Phase 5 (Release)
- [ ] 🎉 Celebrate v1.0.0!

---

## 🚀 Ready to Start?

1. ✅ Read this quick start (you just did!)
2. ✅ Open `V1_RELEASE_PREPARATION_PLAN.md`
3. ✅ Start with **Phase 1A: PostgreSQL Placeholder Format Bug Fix**
4. ✅ Follow the detailed steps in the plan
5. ✅ Commit and tag after each phase

---

## 📞 Questions?

Everything you need is in **V1_RELEASE_PREPARATION_PLAN.md**.

It has:
- Exact commands to run
- Expected outputs
- Decision trees for bugs
- Pre-written commit messages
- Success criteria for each step

**Trust the plan. Follow it sequentially. You've got this! 🚀**

---

**Good luck with v1.0.0! 🎊**
