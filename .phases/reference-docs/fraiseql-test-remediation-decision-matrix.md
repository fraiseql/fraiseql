# FraiseQL Test Remediation - Decision Matrix

**Quick Reference Guide for Test Suite Remediation**

---

## Phase Priority Matrix

| Phase | Tests Fixed | Effort | Risk | Start Date | Priority |
|-------|-------------|--------|------|------------|----------|
| **Phase 1** | 16 | 2-4 hours | LOW | Today | **CRITICAL** |
| **Phase 2** | ~150 | 16-20 hours | MEDIUM | Week 2 | **HIGH** |
| **Phase 3** | 0-20 | 10-20 hours | MEDIUM | Week 3 | **HIGH** |
| **Phase 4** | 92+10+2 | 4-6 hours | LOW | Week 4 | **MEDIUM** |

---

## Critical Decision: Root Cause Analysis

### The Key Finding

**Question**: Are the ~150 SQL validation test failures due to bugs in SQL generation or bugs in the tests themselves?

**Answer**: **BUGS IN THE TESTS** (95% confidence)

### Evidence

1. **Pattern**: All failures show `Composed([SQL(...), Literal(...)])` in assertion errors
2. **Root Cause**: Tests call `str(composed_object)` which returns `repr()`, not SQL
3. **Solution**: Tests need `composed_object.as_string(connection)` or mock rendering
4. **Impact**: Phase 2 (SQL rendering fix) should eliminate ~90% of failures

### What This Means

- ✅ **Good News**: SQL generation likely works correctly
- ✅ **Good News**: Fix is systematic (one pattern to update)
- ✅ **Good News**: Can use local AI model for bulk migration
- ⚠️ **Watch Out**: Phase 2 may reveal 5-10% real SQL bugs
- ⚠️ **Watch Out**: Need Phase 3 budget for actual bugs

---

## Decision Tree: Which Phase First?

### Option A: Sequential (Recommended) ✅

```
Phase 1 (Week 1) → Phase 2 (Week 2) → Phase 3 (Week 3) → Phase 4 (Week 4)
```

**Advantages**:
- ✅ Quick wins in Week 1 (16 tests fixed)
- ✅ Clear progress visibility (214 → 198 → 48 → 0)
- ✅ Phase 2 reveals real bugs for Phase 3
- ✅ Lower cognitive load (one thing at a time)

**Disadvantages**:
- ❌ Takes 4 weeks total
- ❌ Can't parallelize work

**Verdict**: **RECOMMENDED** - Clear, low-risk, systematic

### Option B: Parallel Phase 1 + Phase 2 (Aggressive)

```
Week 1: Phase 1 + Start Phase 2
Week 2: Finish Phase 2 + Start Phase 3
Week 3: Finish Phase 3 + Phase 4
```

**Advantages**:
- ✅ Faster completion (3 weeks vs 4)
- ✅ Higher developer engagement

**Disadvantages**:
- ❌ Higher cognitive load
- ❌ Risk of context switching
- ❌ Phase 2 completion needed to identify Phase 3 work

**Verdict**: **ACCEPTABLE** if time-constrained

### Option C: Phase 2 First (Not Recommended)

```
Phase 2 → Phase 1 → Phase 3 → Phase 4
```

**Advantages**:
- ✅ Fixes most tests fastest (150 vs 16)

**Disadvantages**:
- ❌ No quick wins in Week 1
- ❌ Harder to validate Phase 2 correctness
- ❌ Phase 1 failures may confuse Phase 2 debugging

**Verdict**: **NOT RECOMMENDED** - lose quick wins

---

## Decision: Use Local AI Model?

### For Phase 1 (16 tests)
**Decision**: ❌ **NO** - Use Claude

**Reasoning**:
- Requires understanding v1.8.1 semantics
- Need to verify field names are correct
- Small number of tests (not worth delegation overhead)
- High value in Claude reviewing changes

### For Phase 2.1 (SQL Rendering Utility)
**Decision**: ❌ **NO** - Use Claude

**Reasoning**:
- Architecture decision
- Need to handle edge cases correctly
- Need to write comprehensive docstrings
- This is a reusable utility (quality critical)

### For Phase 2.2 (Bulk SQL Test Migration)
**Decision**: ✅ **YES** - Use Local AI Model

**Reasoning**:
- 150 tests with identical pattern
- Clear transformation: `str(obj)` → `render_sql_for_testing(obj)`
- Repetitive and well-defined
- Claude reviews batch results
- Perfect use case for Ministral-3-8B-Instruct

**Prompt Template**:
```
Task: Update SQL assertion pattern

Replace:
  sql_str = str(composed_object)
  assert "::inet" in sql_str

With:
  sql_str = render_sql_for_testing(composed_object)
  assert "::inet" in sql_str

Add import at top:
  from tests.helpers.sql_rendering import render_sql_for_testing

File: [paste content]

Output: Complete updated file
```

### For Phase 3 (Bug Fixes)
**Decision**: ❌ **NO** - Use Claude

**Reasoning**:
- Complex debugging required
- Multi-file reasoning needed
- Operator strategy architecture knowledge required
- Unknown scope until Phase 2 complete

### For Phase 4 (Cleanup)
**Decision**: ⚖️ **MIXED** - Case by case

**Reasoning**:
- Shellcheck test: Claude (architecture decision)
- Performance markers: Claude (test configuration)
- Deprecation warnings: Local AI (if pattern-based, else Claude)

---

## Decision: Commit Strategy

### Option A: One Commit Per Phase (Recommended) ✅

```bash
# Week 1
git commit -m "test(mutations): update tests for v1.8.1 field semantics [Phase 1]"

# Week 2
git commit -m "test(sql): add SQL rendering utilities and update validation tests [Phase 2]"

# Week 3
git commit -m "fix(sql): resolve network type and casting bugs [Phase 3]"

# Week 4
git commit -m "test(config): configure performance markers and fix warnings [Phase 4]"
```

**Advantages**:
- ✅ Clear phase boundaries
- ✅ Easy to review each phase
- ✅ Can rollback individual phases
- ✅ Git history tells the story

**Disadvantages**:
- ❌ Large commits (Phase 2: ~150 files)

**Verdict**: **RECOMMENDED**

### Option B: Commit Per Category

```bash
git commit -m "test(mutations): update auto-populate tests for v1.8.1"
git commit -m "test(decorators): update field order tests for v1.8.1"
git commit -m "test(integration): update error array tests for v1.8.1"
# ... etc (10-20 commits)
```

**Advantages**:
- ✅ Smaller, focused commits
- ✅ Easier to review individual changes

**Disadvantages**:
- ❌ More commits to manage
- ❌ Harder to see phase completion
- ❌ More context switching

**Verdict**: **ACCEPTABLE** for Phase 1 and 3, **NOT RECOMMENDED** for Phase 2

### Option C: Squash at End

```bash
# Work in feature branch with many small commits
git checkout -b test-suite-remediation

# Many commits during work...

# Squash into 4 phase commits before merge
git rebase -i main  # squash into 4 commits
```

**Advantages**:
- ✅ Best of both worlds (small commits during work, clean history after)
- ✅ Easier to work incrementally

**Disadvantages**:
- ❌ Requires rebase knowledge
- ❌ Can lose granular history

**Verdict**: **RECOMMENDED** if comfortable with git rebase

---

## Decision: Testing Strategy Per Phase

### Phase 1: After Each File
```bash
# Update test_auto_populate_schema.py
uv run pytest tests/unit/mutations/test_auto_populate_schema.py -v

# Update test_decorators.py
uv run pytest tests/unit/decorators/test_decorators.py -v

# Final verification
uv run pytest tests/unit/mutations/ tests/unit/decorators/ -v
```

**Reason**: Small number of files, fast feedback

### Phase 2: After Each Batch of 20-30 Files
```bash
# Batch 1: Update 30 files in test_complete_sql_validation.py
uv run pytest tests/regression/where_clause/test_complete_sql_validation.py -v

# Batch 2: Update 30 files in test_industrial_where_clause_generation.py
uv run pytest tests/regression/where_clause/test_industrial_where_clause_generation.py -v

# ... etc

# Final verification
uv run pytest tests/regression/where_clause/ -v
```

**Reason**: Catch migration errors early, avoid debugging 150 files at once

### Phase 3: After Each Bug Fix
```bash
# Fix network strategy selection bug
uv run pytest tests/core/test_special_types_tier1_core.py::TestTier1StrategySelection -v

# Fix daterange casting bug
uv run pytest tests/core/test_special_types_tier1_core.py::TestTier1DateRangeTypes -v

# Final verification
uv run pytest tests/core/ tests/regression/ -v
```

**Reason**: Unknown number of bugs, validate each fix individually

### Phase 4: After Each Configuration Change
```bash
# Add performance markers
uv run pytest -m "performance" -v  # Should run performance tests
uv run pytest -m "not performance" -v  # Should skip performance tests

# Fix deprecation warnings
uv run pytest -W error -v  # Warnings become errors (should pass)
```

**Reason**: Configuration changes need immediate validation

---

## Decision: Branch Strategy

### Option A: Single Feature Branch (Recommended) ✅

```bash
git checkout -b test-suite-100-percent
# Work on all 4 phases in this branch
git push origin test-suite-100-percent
# Create PR when all phases complete
```

**Advantages**:
- ✅ Simple and clean
- ✅ One PR to review
- ✅ Atomic deployment (all or nothing)

**Disadvantages**:
- ❌ Long-lived branch (4 weeks)
- ❌ Large PR (hard to review)

**Verdict**: **RECOMMENDED** for solo work

### Option B: Phase Branches (Team Work)

```bash
git checkout -b test-suite-phase1
# Complete Phase 1, merge to main

git checkout -b test-suite-phase2
# Complete Phase 2, merge to main

# ... etc
```

**Advantages**:
- ✅ Incremental deployment
- ✅ Smaller PRs
- ✅ Team can review each phase

**Disadvantages**:
- ❌ Phase dependencies (Phase 3 needs Phase 2 merged)
- ❌ More branch management

**Verdict**: **RECOMMENDED** for team work or incremental deployment

---

## Final Recommendation

### For Solo Developer (Lionel)

**Strategy**: Sequential Phases + Single Branch + Squashed Commits

```bash
# Setup
git checkout -b test-suite-100-percent

# Week 1: Phase 1 (Claude)
# - Update 16 tests for v1.8.1 semantics
# - Commit: "test(mutations): update for v1.8.1 [Phase 1]"

# Week 2: Phase 2 (Claude + Local AI)
# - Create SQL rendering utility (Claude)
# - Bulk migrate 150 tests (Local AI with Claude review)
# - Commit: "test(sql): add rendering utilities [Phase 2]"

# Week 3: Phase 3 (Claude)
# - Fix revealed SQL bugs
# - Commit: "fix(sql): resolve casting and strategy bugs [Phase 3]"

# Week 4: Phase 4 (Claude)
# - Configure test markers
# - Fix warnings
# - Commit: "test(config): finalize test suite [Phase 4]"

# Merge
git rebase -i main  # Optional: squash to 4 clean commits
git push origin test-suite-100-percent
# Create PR or merge directly
```

**Timeline**: 4 weeks, ~30 hours total effort

**Outcome**: 100% passing test suite (5,315/5,315)

---

**Ready to Execute: Start with Phase 1 today** ✅
