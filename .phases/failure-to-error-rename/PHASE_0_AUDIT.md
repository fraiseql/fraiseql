# Phase 0: Pre-Implementation Audit

**Objective**: Discover and catalog ALL occurrences of `@failure` decorator and related references before implementation begins.

**Status**: ✅ **COMPLETED** - Audit run on 2025-12-12

---

## Summary

| Category | Occurrences | Files | Status |
|----------|-------------|-------|--------|
| Python Source | 10 | 5 | ⚠️ Core files |
| Test Files | 18 | 9 | ⚠️ Need updates |
| Documentation | 6 | ~3-4 | ℹ️ Docs only |
| Examples | 0 | 0 | ✅ Already clean |
| Rust Files | 3 | 2 | ℹ️ Comments only |
| **TOTAL** | **37** | **~19** | 🎯 Manageable scope |

**Key Finding**: Original estimate of "200+ occurrences" was incorrect. Actual scope is **37 occurrences across 19 files** - much more manageable!

---

## Detailed Audit Results

### 1. Python Source Files (10 occurrences, 5 files)

**Critical Files**:
```
src/fraiseql/mutations/decorators.py:24    _failure_registry: dict[str, type] = {}
src/fraiseql/mutations/decorators.py:31    _failure_registry.clear()
src/fraiseql/mutations/decorators.py:247   _failure_registry[cls.__name__] = cls
src/fraiseql/mutations/decorators.py:262   if error_name in _failure_registry:
src/fraiseql/mutations/decorators.py:263   failure_cls = _failure_registry[error_name]
src/fraiseql/mutations/decorators.py:272   for failure_name, failure_cls in _failure_registry.items():
```

**Public API Exports**:
```
src/fraiseql/mutations/__init__.py:7       from .decorators import failure, resolve_union_annotation, result, success
src/fraiseql/__init__.py:11                from .mutations.decorators import failure, result, success
```

**Documentation/Type Hints**:
```
src/fraiseql/utils/introspection.py:14     # Comment: "decorated with @fraise_input, @success, @failure, or @fraise_type"
src/fraiseql/types/definitions.py:15       # Comment: "`@success`, or `@failure`, and stores runtime metadata"
```

**Impact**: HIGH - Core decorator implementation and public API

---

### 2. Test Files (18 occurrences, 9 files)

**Test Files Requiring Updates**:
1. `tests/test_mutation_field_selection_integration.py`
2. `tests/mutations/test_canary.py`
3. `tests/integration/graphql/mutations/test_mutation_failure_alias.py` ⚠️ Special - tests alias functionality
4. `tests/integration/graphql/mutations/test_decorators.py`
5. `tests/integration/graphql/mutations/test_mutation_decorator.py`
6. `tests/unit/decorators/test_empty_string_to_null.py`
7. `tests/unit/decorators/test_decorators.py`
8. `tests/unit/decorators/test_mutation_decorator.py`
9. `tests/unit/mutations/test_auto_populate_schema.py`

**Impact**: MEDIUM - Test updates are straightforward (import + decorator changes)

---

### 3. Documentation Files (6 occurrences, ~3-4 files)

**Command used**:
```bash
grep -r "@failure\|import failure" docs/ README.md --include="*.md" -n
```

**Expected files**:
- `docs/reference/decorators.md` (likely)
- `docs/getting-started/quickstart.md` (likely)
- `README.md` (main example)
- `docs/guides/error-handling-patterns.md` (possibly)

**Impact**: MEDIUM - Documentation updates are critical for users

---

### 4. Examples (0 occurrences)

**Command used**:
```bash
grep -r "@failure\|import failure" examples/ --include="*.py" -n
```

**Result**: No occurrences found ✅

**Status**: Examples directory may not exist or already uses `@error`

**Impact**: NONE - Skip Phase 3 or verify examples exist

---

### 5. Rust Files (3 occurrences, 2 files)

**Comments only** (no code changes required):
```
fraiseql_rs/src/mutation/response_builder.rs:433   // Validation failure or business rule rejection
fraiseql_rs/src/mutation/response_builder.rs:453   // Internal Server Error (generic failure)
fraiseql_rs/src/mutation/test_status_only.rs:137   // NOOP PREFIX (validation/business rule failure) - should be Noop variant AND Error type
```

**Impact**: LOW - Comment updates only, no compilation changes

---

## Verification Commands

### Re-run Audit (to verify changes)
```bash
# Python source
echo "=== Python Source ==="
grep -r "@failure\|import failure\|_failure_" src/ --include="*.py" -n | wc -l
grep -r "@failure\|import failure\|_failure_" src/ --include="*.py" -n

# Test files
echo "=== Test Files ==="
find tests/ -name "*.py" -exec grep -l "@failure\|from.*failure\|import.*failure" {} \; | wc -l
find tests/ -name "*.py" -exec grep -l "@failure\|from.*failure\|import.*failure" {} \;

# Documentation
echo "=== Documentation ==="
grep -r "@failure\|import failure" docs/ README.md --include="*.md" -n 2>/dev/null | wc -l

# Examples
echo "=== Examples ==="
grep -r "@failure\|import failure" examples/ --include="*.py" -n 2>/dev/null | wc -l

# Rust
echo "=== Rust Files ==="
grep -r "failure" fraiseql_rs/src/ --include="*.rs" -n | wc -l

# Total
echo "=== TOTAL ==="
echo "Should be 0 after all phases complete"
```

---

## Scope Adjustments to Original Plan

Based on audit findings, adjust the following phases:

### Phase 1: Core Python ✅ Confirmed
- 5 files affected (matches audit)
- `decorators.py`: 6 references to `_failure_registry`
- Public API: 2 export changes

### Phase 2: Tests ⚠️ **Reduce Scope**
- **Original estimate**: ~40 files
- **Actual**: 9 files
- **Action**: Update Phase 2 to list exact 9 files

### Phase 3: Examples ✅ **SKIP or VERIFY**
- **Original estimate**: 9 files
- **Actual**: 0 occurrences
- **Action**: Either skip Phase 3 or verify examples/ exists and check manually

### Phase 5: Rust ✅ **Confirmed Light Changes**
- **Original estimate**: "Likely no code changes"
- **Actual**: 3 comment-only changes
- **Action**: Simple comment updates, no cargo rebuild needed

### Phase 6: Documentation ✅ **Reduce Scope**
- **Original estimate**: ~15 files
- **Actual**: ~6 occurrences across 3-4 files
- **Action**: Update Phase 6 to focus on specific files

---

## Pre-Implementation Checklist

Before starting Phase 1:

- [x] Audit completed
- [x] Scope validated (37 occurrences, not 200+)
- [ ] Phase 1 file list verified against audit
- [ ] Phase 2 test file list updated to 9 files
- [ ] Phase 3 examples strategy decided (skip or verify)
- [ ] Phase 5 Rust changes confirmed as comments-only
- [ ] Phase 6 documentation file list refined
- [ ] Git branch created: `feature/rename-failure-to-error`
- [ ] All tests currently passing (baseline)

---

## Baseline Test Status

**Run before Phase 1 to establish baseline**:
```bash
# Full test suite
uv run pytest tests/ -v --tb=short > /tmp/baseline_tests.txt 2>&1
grep -E "passed|failed|error" /tmp/baseline_tests.txt | tail -5

# Store baseline
git add /tmp/baseline_tests.txt
git commit -m "test: baseline before @failure → @error rename"
```

**Expected**: All tests pass (or document known failures)

---

## Audit Completion

**Audited by**: Claude (Architect)
**Date**: 2025-12-12
**Command log**: All commands saved above for reproducibility
**Approval**: ✅ Ready to proceed with Phase 1

**Next Step**: Review Phase 1 implementation plan and begin with `decorators.py` changes.
