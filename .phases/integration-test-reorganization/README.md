# Integration Test Reorganization

**Status:** READY FOR EXECUTION
**Type:** Test Infrastructure Refactoring
**Estimated Duration:** 1-2 hours
**Risk Level:** Low
**Context:** Post-operator refactoring cleanup

---

## Objective

Reorganize integration tests in `tests/integration/database/sql/` to match the unit test structure created during the operator strategies refactoring.

**Current Problem:**
- Flat structure with 15+ test files in single directory
- Inconsistent with organized unit test structure
- Hard to find related tests
- Unclear where new tests should go

**Desired Outcome:**
- Hierarchical structure matching unit tests
- Tests organized by operator type (network, specialized, temporal, spatial)
- Clear correspondence: `unit/.../network/` ↔ `integration/.../network/`
- Easier test discovery and maintenance

---

## Context

**Prerequisite Work:**
- ✅ Operator strategies refactored (Phases 1-8 complete)
- ✅ Unit tests reorganized into structured hierarchy
- ✅ All 126 ltree tests passing
- ✅ All operator unit tests organized

**Current Unit Test Structure:**
```
tests/unit/sql/where/
├── core/                    # Core WHERE functionality
├── operators/
    ├── network/            # IP, MAC, hostname, email, port
    ├── specialized/        # ltree, fulltext
    ├── temporal/           # date, datetime, daterange
    └── <root level>        # basic, array, jsonb, list, pattern
```

**Current Integration Test Structure:**
```
tests/integration/database/sql/
├── test_end_to_end_ltree_filtering.py
├── test_ltree_filter_operations.py
├── test_network_address_filtering.py
├── test_mac_address_filter_operations.py
└── ... (15+ files, flat)
```

**Target Integration Test Structure:**
```
tests/integration/database/sql/where/
├── network/                           # Network operator integration tests
├── specialized/                       # PostgreSQL-specific integration tests
├── temporal/                          # Time-related integration tests
├── spatial/                           # Spatial/coordinate tests
└── <root level>                       # Mixed-type and cross-cutting tests
```

---

## Implementation Phases

### Phase 1: Assessment & Planning (ANALYSIS)
**File:** `phase-1-assessment.md`
**Goal:** Inventory current tests, plan new structure, identify dependencies
**Duration:** 15-20 minutes
**Risk:** None (read-only analysis)

### Phase 2: Create Directory Structure (SETUP)
**File:** `phase-2-create-structure.md`
**Goal:** Create new directories, add __init__.py files, create READMEs
**Duration:** 10-15 minutes
**Risk:** Low (additive only)

### Phase 3: Move & Rename Files (MIGRATION)
**File:** `phase-3-move-files.md`
**Goal:** Move test files to new locations with better names
**Duration:** 15-20 minutes
**Risk:** Medium (tests will temporarily fail until imports updated)

### Phase 4: Update References (FIX)
**File:** `phase-4-update-references.md`
**Goal:** Fix imports, update CI/CD paths, update documentation
**Duration:** 10-15 minutes
**Risk:** Low (straightforward find/replace)

### Phase 5: Verification & QA (VALIDATION)
**File:** `phase-5-verification.md`
**Goal:** Run full test suite, verify no regressions, check test discovery
**Duration:** 10-15 minutes
**Risk:** None (validation only)

### Phase 6: Documentation & Cleanup (FINALIZATION)
**File:** `phase-6-documentation.md`
**Goal:** Update docs, add directory READMEs, clean up old references
**Duration:** 10-15 minutes
**Risk:** None (documentation only)

---

## Success Criteria

### Structure
- [ ] All integration tests organized into subdirectories
- [ ] Directory structure matches unit test organization
- [ ] Each directory has __init__.py and README.md
- [ ] No more than 5 files in root `where/` directory

### Testing
- [ ] All tests still pass (0 new failures)
- [ ] Test discovery works: `pytest tests/integration/database/sql/where/`
- [ ] Individual directories testable: `pytest tests/integration/database/sql/where/network/`
- [ ] CI/CD pipeline still works

### Maintainability
- [ ] Clear naming conventions established
- [ ] README in each directory explains test purpose
- [ ] Contributing guide updated with new structure
- [ ] Test counts: network (~8), specialized (~4), temporal (~3), spatial (~2)

---

## Benefits

### Immediate Benefits
1. **Consistency** - Matches unit test structure
2. **Discoverability** - Easy to find related tests
3. **Navigation** - Reduced cognitive load
4. **Organization** - Clear test categories

### Long-term Benefits
1. **Maintainability** - Clear where new tests go
2. **Onboarding** - Easier for new contributors
3. **Refactoring** - Easy to refactor test categories together
4. **Documentation** - Natural place for category-specific docs

---

## Risk Assessment

### Risk Matrix

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Tests fail after move | Low | Medium | Phase 3 has rollback, Phase 5 validates |
| CI/CD breaks | Low | High | Use parent paths, update in Phase 4 |
| Git history lost | Medium | Low | Use `git mv`, document moves |
| Merge conflicts | Medium | Medium | Do in quiet period, coordinate |
| Import errors | Low | Medium | Phase 4 systematically fixes |

### Overall Risk: **LOW**
- Small number of files (~15)
- Clear dependencies
- Straightforward migration
- Easy rollback at any phase

---

## Rollback Plan

### Per-Phase Rollback

**Phase 1-2 (Assessment, Structure):**
- No rollback needed (read-only or additive)
- Can delete new directories if needed

**Phase 3 (Move Files):**
```bash
# Rollback using git
git reset --hard HEAD
git clean -fd
```

**Phase 4-6 (Updates, Verification, Docs):**
```bash
# Rollback to before Phase 3
git reset --hard <commit-before-phase-3>
```

### Emergency Rollback
```bash
# Nuclear option - revert all changes
cd /home/lionel/code/fraiseql
git stash
git reset --hard origin/main
```

---

## Files to Move (Inventory)

### Network Tests (8 files)
- `test_end_to_end_ip_filtering_clean.py`
- `test_network_address_filtering.py`
- `test_network_filtering_fix.py`
- `test_production_cqrs_ip_filtering_bug.py`
- `test_network_operator_consistency_bug.py`
- `test_jsonb_network_filtering_bug.py`
- `test_mac_address_filter_operations.py`
- `test_end_to_end_mac_address_filtering.py`

### Specialized Tests (2 files)
- `test_end_to_end_ltree_filtering.py`
- `test_ltree_filter_operations.py`

### Temporal Tests (2 files)
- `test_daterange_filter_operations.py`
- `test_end_to_end_daterange_filtering.py`

### Spatial Tests (1 file)
- `test_coordinate_filter_operations.py`

### Mixed/Root Tests (2 files)
- `test_end_to_end_phase4_filtering.py`
- `test_end_to_end_phase5_filtering.py`

**Total:** 15 files to reorganize

---

## Timeline

### Sequential Execution
- **Total Time:** 70-110 minutes (1.2-1.8 hours)
- **Recommended:** Execute in single session
- **Best Time:** After operator refactoring, before next major work

### Parallel Execution (Not Recommended)
- Phases must be sequential
- Each phase depends on previous completion

---

## Dependencies

### Prerequisites
- [ ] All current tests passing
- [ ] Operator refactoring complete (Phases 1-8)
- [ ] Clean git working directory
- [ ] No pending PRs modifying test files

### Blocks
- Future test additions should follow new structure

### Enables
- Cleaner test organization
- Easier test maintenance
- Better developer experience

---

## Related Work

- **Operator Strategies Refactor:** `.phases/operator-strategies-refactor/` (COMPLETED)
- **Unit Test Reorganization:** Part of operator refactor Phase 7
- **WHERE Clause Refactor:** `.phases/archive/where-industrial-refactor/` (COMPLETED)

---

## Notes

### Execution Tips
1. Execute phases sequentially in one sitting
2. Commit after each phase completes
3. Use descriptive commit messages with [PHASE-N] tags
4. Run tests after each phase (Phases 3-6)
5. Document any deviations in commit messages

### Naming Conventions
- **Operations tests:** `test_<type>_operations.py` (tests operator SQL generation)
- **Filtering tests:** `test_<type>_filtering.py` (tests end-to-end filtering)
- **Bug tests:** `test_<type>_bugs.py` or `test_production_bugs.py` (regression tests)
- **Consistency tests:** `test_<type>_consistency.py` (cross-operator validation)

### Git Best Practices
```bash
# Use git mv to preserve history
git mv old/path/file.py new/path/file.py

# Check history after move
git log --follow new/path/file.py

# Use -C option for blame
git blame -C new/path/file.py
```

---

## Next Steps

After completing all phases:
1. Update team about new structure
2. Update onboarding documentation
3. Create test template examples for each category
4. Consider similar reorganization for other test categories (e.g., repository tests)

---

**Prepared by:** Claude (Sonnet 4.5)
**Date:** 2025-12-11
**Status:** Ready for Execution ✅
