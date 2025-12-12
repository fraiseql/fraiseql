# @failure → @error Rename - Quick Reference

**Status**: ✅ Ready to execute (Phase 0 audit completed)

---

## 📁 Documents in This Directory

1. **[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)** - Main implementation plan with all 10 phases
2. **[PHASE_0_AUDIT.md](PHASE_0_AUDIT.md)** - Pre-implementation audit results (COMPLETED)
3. **[CHECKPOINTS.md](CHECKPOINTS.md)** - Verification scripts for each phase
4. **README.md** (this file) - Quick reference guide

---

## 🎯 Executive Summary

**Scope**: Rename `@failure` decorator to `@error` throughout FraiseQL codebase

**Audit Results**:
- **37 occurrences** across **19 files** (original estimate of 200+ was incorrect)
- Python source: 10 occurrences (5 files)
- Tests: 18 occurrences (9 files)
- Docs: 6 occurrences (3-4 files)
- Examples: 0 occurrences (skip Phase 3)
- Rust: 3 occurrences (comments only)

**Estimated Time**: 2-4 hours with local model delegation

---

## 🚀 Quick Start

### 1. Verify Phase 0 Audit Completed
```bash
cat .phases/failure-to-error-rename/PHASE_0_AUDIT.md
# Should show "Status: ✅ COMPLETED"
```

### 2. Create Feature Branch
```bash
git checkout -b feature/rename-failure-to-error
```

### 3. Run Checkpoint 0 (Pre-Implementation)
```bash
# See CHECKPOINTS.md > Checkpoint 0
# This establishes baseline tests
```

### 4. Execute Phases Sequentially

For each phase:
1. Read phase instructions in `IMPLEMENTATION_PLAN.md`
2. Execute changes (use local models for batch updates)
3. Run corresponding checkpoint from `CHECKPOINTS.md`
4. Commit if checkpoint passes
5. Move to next phase

---

## 📋 Phase Checklist

- [ ] **Phase 0**: Pre-Implementation Audit ✅ COMPLETED
- [ ] **Phase 1**: Core Python Implementation [RED]
  - Files: 5 core files
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 1`
  - Expected: Tests will fail (intentional RED phase)
- [ ] **Phase 2**: Update All Test Files [GREEN]
  - Files: 9 test files
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 2`
  - Expected: All tests pass
- [ ] **Phase 3**: Update Examples [GREEN]
  - Files: 0 (audit shows no occurrences)
  - Checkpoint: Skip or verify examples don't exist
- [ ] **Phase 4**: Update CLI & Introspection [GREEN]
  - Files: 3 files
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 4`
- [ ] **Phase 5**: Update Rust Code [GREEN]
  - Files: 2 files (3 comment changes only)
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 5`
- [ ] **Phase 6**: Update Documentation [REFACTOR]
  - Files: 3-4 doc files
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 6`
- [ ] **Phase 7**: Update Config & Misc [REFACTOR]
  - Files: Final sweep
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 7`
- [ ] **Phase 8**: Final Verification & QA [QA]
  - Comprehensive testing
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 8`
- [ ] **Phase 9**: Migration Guide [GREENFIELD]
  - Create migration docs
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 9`
- [ ] **Phase 10**: Archaeology Cleanup [EVERGREEN]
  - Remove historical comments
  - Checkpoint: `CHECKPOINTS.md > Checkpoint 10`

---

## ⚡ Critical Success Factors

1. **Run Phase 0 audit** - Already done ✅
2. **Run checkpoint after EACH phase** - Don't skip!
3. **Don't proceed if checkpoint fails** - Fix issues first
4. **Commit after each successful checkpoint** - Small commits
5. **Use local models for batch updates** - Save Claude API costs
6. **Run Phase 10 archaeology cleanup** - Keep code evergreen

---

## 🎓 TDD Phase Labels

| Label | Meaning | Phases |
|-------|---------|--------|
| **[RED]** | Break tests intentionally | Phase 1 |
| **[GREEN]** | Fix tests, make them pass | Phases 2-5 |
| **[REFACTOR]** | Clean up code/docs | Phases 6-7 |
| **[QA]** | Comprehensive verification | Phase 8 |
| **[GREENFIELD]** | New documentation | Phase 9 |
| **[EVERGREEN]** | Remove archaeology | Phase 10 |

---

## 🔄 Workflow Pattern

```
┌─────────────────────────────────────────────────────┐
│ For each phase:                                     │
│                                                      │
│  1. Read phase in IMPLEMENTATION_PLAN.md            │
│  2. Execute changes (delegate to local AI if batch) │
│  3. Run checkpoint from CHECKPOINTS.md              │
│  4. If PASS: commit and move to next phase          │
│  5. If FAIL: fix issues, re-run checkpoint          │
│                                                      │
│  Don't skip checkpoints!                            │
└─────────────────────────────────────────────────────┘
```

---

## 🛡️ Rollback Strategy

**If Phase 1 fails** → ROLLBACK (core is broken):
```bash
git reset --hard HEAD~1
```

**If Phase 2-10 fails** → FIX FORWARD (too much invested):
- Identify issue
- Fix it
- Re-run checkpoint
- Continue

---

## 📊 Scope Verification

Run these commands to verify audit findings:

```bash
# Python source (expected: 10)
grep -r "@failure\|import failure\|_failure_" src/ --include="*.py" 2>/dev/null | wc -l

# Tests (expected: 18)
grep -r "@failure\|import failure" tests/ --include="*.py" 2>/dev/null | wc -l

# Docs (expected: 6)
grep -r "@failure\|import failure" docs/ README.md --include="*.md" 2>/dev/null | wc -l

# Examples (expected: 0)
grep -r "@failure\|import failure" examples/ --include="*.py" 2>/dev/null | wc -l

# Rust (expected: 3)
grep -r "failure" fraiseql_rs/src/ --include="*.rs" -n 2>/dev/null | wc -l

# TOTAL: Should be 37
```

---

## 🎯 Key Files to Update

### Phase 1 (Core - 5 files)
1. `src/fraiseql/mutations/decorators.py` - Decorator definition
2. `src/fraiseql/mutations/__init__.py` - Export
3. `src/fraiseql/__init__.py` - Public API
4. `src/fraiseql/__init__.pyi` - Type stubs
5. `src/fraiseql/mutations/mutation_decorator.py` - Remove `failure:` field support

### Phase 2 (Tests - 9 files)
1. `tests/test_mutation_field_selection_integration.py`
2. `tests/mutations/test_canary.py`
3. `tests/integration/graphql/mutations/test_mutation_failure_alias.py`
4. `tests/integration/graphql/mutations/test_decorators.py`
5. `tests/integration/graphql/mutations/test_mutation_decorator.py`
6. `tests/unit/decorators/test_empty_string_to_null.py`
7. `tests/unit/decorators/test_decorators.py`
8. `tests/unit/decorators/test_mutation_decorator.py`
9. `tests/unit/mutations/test_auto_populate_schema.py`

### Phase 5 (Rust - 2 files, 3 comments)
1. `fraiseql_rs/src/mutation/response_builder.rs` - Lines 433, 453
2. `fraiseql_rs/src/mutation/test_status_only.rs` - Line 137

---

## 🤖 Local Model Usage

Use local models (Ministral-3-8B-Instruct) for:
- Batch sed replacements (Phase 2, 3, 6)
- Simple comment updates (Phase 5)
- Repetitive pattern application

**Keep using Claude for**:
- Architectural review (you're doing this!)
- Complex debugging
- Phase 1 core implementation (critical)
- Phase 8 QA verification

---

## ✅ Final Success Criteria

After Phase 10 completes:

```bash
# Should return 0
grep -r "@failure\|import failure\|_failure_" src/ tests/ --include="*.py" 2>/dev/null | wc -l

# Should pass
uv run pytest tests/ -v

# Should pass
uv run ruff check src/ tests/

# Should pass (if mypy available)
uv run mypy src/fraiseql/

# Should work
python3 -c "from fraiseql import error; print('✓')"

# Should fail
python3 -c "from fraiseql import failure" 2>&1 | grep ImportError
```

---

## 📞 Support

If you encounter issues:
1. Check the phase-specific troubleshooting in `IMPLEMENTATION_PLAN.md`
2. Review the checkpoint output for specific failures
3. Consult `PHASE_0_AUDIT.md` for file locations
4. Re-run audit commands to verify current state

---

## 🎉 When Complete

After all phases pass:
1. Run final checkpoint (Checkpoint 10)
2. Create PR: `feature/rename-failure-to-error` → `dev`
3. Include link to migration guide in PR description
4. Tag as breaking change for v2.0

**PR Title**: `feat!: rename @failure to @error decorator [BREAKING]`

**PR Description Template**:
```markdown
## Summary
Renames `@failure` decorator to `@error` to align with GraphQL ecosystem conventions.

## Breaking Changes
- All `@failure` → `@error`
- Migration guide: docs/migration/v2.0-failure-to-error.md
- Automated migration script provided

## Verification
- [x] All tests pass
- [x] No `@failure` references remain
- [x] Migration guide complete
- [x] Code archaeology removed

See: .phases/failure-to-error-rename/IMPLEMENTATION_PLAN.md
```

---

**Last Updated**: 2025-12-12 (Phase 0 audit completed)
