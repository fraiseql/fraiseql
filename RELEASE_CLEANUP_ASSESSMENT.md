# FraiseQL v2.0.0-alpha.2 Release Cleanup Assessment

**Date**: February 7, 2026
**Current Version**: 2.0.0-alpha.2 (released Feb 6, 2026)
**Current Branch**: dev
**Git Status**: Clean (no uncommitted changes)

---

## Executive Summary

FraiseQL v2.0.0-alpha.2 is functionally complete and production-ready, but **contains significant development artifacts** that violate the "Eternal Sunshine Principle" from the CLAUDE.md methodology. Before releasing to production (v2.0.0 GA), all phase documentation, phase-numbered files, and development markers must be removed.

**Estimated cleanup scope**: 150+ files and directories across the codebase.

---

## GitHub Issue Status

**Open Issues**: 8 (no blockers for release)

| Issue | Title | Type | Status |
|-------|-------|------|--------|
| #269 | JSONB field lookup fails with snake_case→camelCase | Bug | OPEN |
| #268 | fraiseql-cli compile drops jsonb_column | Bug | OPEN |
| #267 | Default jsonb_column to 'data' | Enhancement | OPEN |
| #266 | Add wire-backend feature | Feature | OPEN |
| #258 | Schema dependency graph and validation | Feature | OPEN |
| #247 | GraphQL Subscriptions Implementation | Question/Doc | OPEN |
| #226 | Rust-First Architecture v2.0 | Enhancement | OPEN |
| #225 | Security Testing & Enforcement | Enhancement | OPEN |

**Note**: Issues #269, #268, and #267 are JSONB-related and should be addressed in alpha.3. They don't block alpha.2 release but should be prioritized for next iteration.

---

## Development Artifacts to Remove

### 1. ⚠️ CRITICAL: Phase Documentation Directory

**Location**: `.phases/` (65 items)
**Action**: **DELETE ENTIRE DIRECTORY**

**Contains**:
- 26 subdirectories (00-planning through QA-PLANNING-*)
- Phase implementation guides
- Planning documents
- Architecture decisions
- Executive summaries
- Implementation roadmaps
- Chaos engineering and testing documentation

**Why**: Per CLAUDE.md finalization phase, all development archaeology must be removed.

```bash
# To remove
rm -rf .phases/
```

---

### 2. Phase-Named Documentation Files

**Location**: `crates/fraiseql-observers/` (13 files)
**Action**: **DELETE ALL PHASE_* PREFIXED FILES**

Files to remove:
- `PHASE_8_COMPLETION_SUMMARY.md`
- `PHASE_8_INDEX.md`
- `PHASE_9_1_ACTION_TRACING_GUIDE.md`
- `PHASE_9_1_COMPLETION_SUMMARY.md`
- `PHASE_9_1_DESIGN.md`
- `PHASE_9_1_IMPLEMENTATION_GUIDE.md`
- `PHASE_9_1_JAEGER_INTEGRATION_GUIDE.md`
- `PHASE_9_1_INDEX.md`
- `PHASE_9_2_A_METRICS_GUIDE.md`
- `PHASE_9_2_B_MACROS_GUIDE.md`
- `PHASE_9_2_C_LOGGING_GUIDE.md`
- `PHASE_9_2_DESIGN.md`
- `PHASE_9_ROADMAP.md`

**Location**: `fraisier/.claude/` (7+ files)
**Action**: **DELETE ALL PHASE_* PREFIXED FILES**

Files to remove:
- `PHASE_1_IMPLEMENTATION_PLAN.md`
- `PHASE_1_PROGRESS.md`
- `PHASE_3_10_MULTIDATABASE_PLAN.md`
- `PHASE_3_COMPLETION_SUMMARY.md`
- `PHASE_3_IMPLEMENTATION_PLAN.md`
- `PHASE_3_VERIFICATION_REPORT.md`
- `PHASE_10_COMPLETE_DOCUMENTATION_PLAN.md`

---

### 3. Cleanup Planning Artifacts

**Location**: Root directory and test directories
**Action**: **DELETE ALL FILES**

- `.cleanup-plan.md` (root)
- `tests/integration/.cleanup-inventory.json`
- `tests/integration/.cleanup-summary.txt`
- `tests/integration/.cleanup-complete.txt`

**Why**: These are markers from the cleanup process itself. They've served their purpose and should not ship.

---

### 4. Archived Test Files

**Location**: `tests/archived_tests/` (1 subdirectory with 1 file)
**Action**: **DELETE ENTIRE DIRECTORY** or move outside repo

- `tests/archived_tests/dual_mode_system/dual_mode_repository_unit.py.archived`

**Why**: Archived files shouldn't be in the distribution.

---

### 5. Development Backup Files

**Location**: Root directory
**Action**: **DELETE**

- `.pre-commit-config.yaml.backup`

**Why**: Backup files don't belong in version control or distributions.

---

### 6. Phase-Named Test Files

**Location**: Various test directories
**Action**: **RENAME** to remove phase indicators

**Current**: `test_phase0_*.py`, `test_phase2_*.py`, `test_phase3_*.py`, `test_phase4_*.py`
**Action**: Rename to descriptive names without phase numbers

Examples:
- `tests/chaos/phase1_validation.py` → `tests/chaos/cache_validation.py`
- `tests/chaos/test_phase0_verification.py` → `tests/chaos/baseline_verification.py`
- `tests/chaos/cache/test_phase3_validation.py` → `tests/chaos/cache/cache_validation.py`
- `tests/chaos/database/test_phase2_validation.py` → `tests/chaos/database/database_validation.py`
- `tests/chaos/resources/test_phase4_validation.py` → `tests/chaos/resources/resources_validation.py`

---

### 7. Phase References in Commit Messages

**Status**: Recent commits contain phase references (4 of last 15 commits)

**Examples**:
- `feat(clippy): Phase 3 - Fix remaining 73 secondary violations`
- `feat(clippy): Phase 2 - Remove all assert!(true) placeholder assertions`
- `chore(cleanup): Phase 0 - Remove development archaeology markers`
- `refactor(executor): Phase 2, Cycle 4 - Add JSONB strategy tests`

**Action**: **OPTIONAL** - If these commits need to stay on dev branch, they can remain. If squashing for main/production branch, these should be rewritten.

---

## Code Quality Issues to Address

### HIGH PRIORITY

1. **Placeholder Comments in Source Code**

**Location**: `crates/fraiseql-server/src/config/mod.rs:227`

```rust
// Placeholder structs for future phases (TODO: will be defined in later phases)
```

**Action**: Remove comment or replace with proper documentation

---

2. **Phase References in SDK Documentation**

**Files to Update**:
- `fraiseql-python/PYTHON_FEATURE_PARITY.md` - 15+ "Phase" references
- `fraiseql-php/PHP_FEATURE_PARITY.md` - 12+ "Phase" references
- `fraiseql-java/JAVA_FEATURE_PARITY.md` - 10+ "Phase" references
- `fraiseql-scala/SCALA_FEATURE_PARITY.md` - 8+ "Phase" references
- `fraiseql-go/IMPLEMENTATION_SUMMARY.md` - 6+ "Phase" references

**Action**: Rewrite documentation to remove phase numbering, focus on feature status instead

**Example**:
```markdown
# BEFORE
**Phase 7 - Python (100% ✅)**
- Phase 1 (TypeScript): ✅ Complete
- Phase 2 (Java): ✅ Complete
- Phase 7 (Python): ✅ Complete

# AFTER
**Python Implementation (100% ✅)**
- TypeScript SDK: ✅ Complete
- Java SDK: ✅ Complete
- Python SDK: ✅ Complete
```

---

3. **WP- Work Package References**

**Locations**: Found in 5+ files including:
- `tests/unit/test_connection_pool_config.py:4` - WP-027
- `docs/archive/journeys/backend-engineer.md:167` - WP-029
- Various SECURITY_VULNERABILITIES.md and compliance docs

**Action**: Remove WP-* references or replace with GitHub issue numbers

---

### MEDIUM PRIORITY

1. **Test File Naming Conventions**

Some SDK test files use phase-like naming. Review and normalize:
- SDK tests in `fraiseql-java/tests/`
- SDK tests in `fraiseql-typescript/tests/`
- SDK tests in other language bindings

---

2. **Legacy Documentation**

**Location**: `docs/archive/`

**Status**: Should remain but verify it's not referenced by active docs

---

## Production Readiness Checklist

### Code Quality
- [x] All tests passing (2,400+ tests)
- [x] Clippy warnings: 0 (as of Phase 3 complete)
- [x] Format check passing
- [x] Type checking passing
- [x] Security audit passing (with known vulnerability allowlist)

### Release Artifacts
- [ ] `.phases/` directory removed
- [ ] All PHASE_*.md files removed
- [ ] Cleanup planning files removed
- [ ] Backup files removed
- [ ] Archived test directories removed
- [ ] Phase-named test files renamed
- [ ] Placeholder comments removed
- [ ] Phase references removed from SDK documentation
- [ ] WP- references removed or replaced

### Verification
- [ ] `git grep -i "^phase " -- "*.md" -- "*.rs" -- "*.py"` returns 0 matches
- [ ] `git grep "PHASE_"` returns 0 matches
- [ ] `git grep "TODO.*future.*phase"` returns 0 matches
- [ ] No `.phases/` directory in git
- [ ] All tests still pass after cleanup
- [ ] Build passes in release mode: `cargo build --release`

---

## Recommended Cleanup Sequence

1. **Phase 1: Delete Directories** (immediate, no conflicts)
   - Delete `.phases/` directory
   - Delete `tests/archived_tests/` directory
   - Commit: "chore(cleanup): Remove development phase documentation"

2. **Phase 2: Delete Files** (check for references first)
   - Delete `.cleanup-plan.md` and related cleanup tracking files
   - Delete `.pre-commit-config.yaml.backup`
   - Delete PHASE_*.md files from crates/fraiseql-observers/ and fraisier/.claude/
   - Commit: "chore(cleanup): Remove development artifacts and backups"

3. **Phase 3: Rename Test Files** (update imports if needed)
   - Rename phase-numbered test files
   - Update any test runner configuration if needed
   - Commit: "test(refactor): Rename test files to remove phase indicators"

4. **Phase 4: Update Documentation**
   - Update SDK feature parity documentation
   - Remove WP- references
   - Remove phase references from architecture docs
   - Commit: "docs(refactor): Remove development phase references"

5. **Phase 5: Code Cleanup**
   - Remove placeholder comments from source code
   - Update any remaining TODO comments
   - Commit: "refactor(cleanup): Remove development markers from source"

6. **Phase 6: Verification**
   - Run full test suite
   - Run clippy checks
   - Verify git grep patterns
   - Commit: "chore(verify): Confirm all cleanup complete"

---

## Impact Analysis

| Category | Count | Impact | Priority |
|----------|-------|--------|----------|
| Files to delete | 20+ | Low - dev artifacts | HIGH |
| Directories to delete | 3 | Low - dev planning | HIGH |
| Files to rename | 12+ | Medium - test files | HIGH |
| Files to update | 10+ | Low - documentation | MEDIUM |
| Lines to remove | 500+ | Low - comments/markers | MEDIUM |

**Total Estimated Effort**: 2-4 hours for complete cleanup and verification

---

## Next Steps

1. ✅ **Current**: Assessment complete (this document)
2. ⏳ **Next**: Choose cleanup approach (automated script vs. manual)
3. ⏳ **Then**: Execute cleanup phases in sequence
4. ⏳ **Then**: Verify all tests pass
5. ⏳ **Then**: Tag v2.0.0-alpha.3 (or v2.0.0 GA if this is final alpha)

---

## Notes

- **Integration Test Cleanup**: Already complete (per `.cleanup-complete.txt`)
- **JSONB Issues**: #268, #269, #267 should be addressed in next release
- **Phase References in Commits**: Can stay on dev branch; will be cleaned when squashing for production
- **Backward Compatibility**: None of this cleanup affects the API or runtime behavior

---

**Status**: Ready to begin cleanup at user's direction.
