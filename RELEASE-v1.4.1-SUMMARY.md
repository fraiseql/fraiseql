# Release v1.4.1 Summary

## Overview
Released v1.4.1 on 2025-11-10 to fix critical mutation decorator issues that caused 7 test failures.

**Release URL**: https://github.com/fraiseql/fraiseql/releases/tag/v1.4.1
**PR**: https://github.com/fraiseql/fraiseql/pull/129

## Issues Fixed

### Original Problem
Seven tests were failing with:
```
TypeError: Use a FraiseUnion wrapper for result unions, not plain Union
```

Affected tests:
- `tests/integration/test_schema_initialization.py` (4 tests)
- `tests/regression/test_graphql_ip_address_scalar_mapping.py` (3 tests)

### Root Causes Identified

1. **Missing FraiseUnion wrapper** (mutation_decorator.py)
   - The `@mutation` decorator was creating plain `Union[Success, Error]` types
   - Required: `Annotated[Success | Error, FraiseUnion(name)]` wrapper

2. **Stripped Annotated metadata** (mutation_builder.py)
   - Called `get_type_hints()` without `include_extras=True`
   - This stripped the `Annotated` metadata containing `FraiseUnion` information

3. **Single result type pattern** (mutation_decorator.py)
   - Some mutations use the same type for both success and failure
   - Example: `success: RoleMutationResult, failure: RoleMutationResult`
   - Python simplifies `A | A` to just `A` (not a union)
   - Caused FraiseUnion validation to fail

4. **Test pollution** (decorators.py)
   - `clear_mutation_registries()` only cleared decorator registries
   - Did not clear `SchemaRegistry.mutations` dict
   - Caused test ordering failures in CI

## Changes Made

### Commit 1: Fix mutation decorator return types
**File**: `src/fraiseql/mutations/mutation_decorator.py`

```python
# Before:
return_type = Union[self.success_type, self.error_type]

# After:
return_type = Annotated[
    self.success_type | self.error_type,
    FraiseUnion(union_name),
]
```

### Commit 2: Preserve Annotated metadata
**File**: `src/fraiseql/gql/builders/mutation_builder.py`

```python
# Before:
hints = get_type_hints(fn)

# After:
hints = get_type_hints(fn, include_extras=True)
```

### Commit 3: Handle single result type pattern
**File**: `src/fraiseql/mutations/mutation_decorator.py`

```python
# Added check for identical success/error types:
if self.success_type is self.error_type:
    # Single result type used for both success and error - no union needed
    return_type = self.success_type
else:
    # Create union with FraiseUnion wrapper
    ...
```

### Commit 4: Fix test pollution
**File**: `src/fraiseql/mutations/decorators.py`

```python
def clear_mutation_registries() -> None:
    """Clear all mutation decorator registries and SchemaRegistry mutations."""
    _success_registry.clear()
    _failure_registry.clear()
    _union_registry.clear()

    # Also clear the SchemaRegistry mutations to prevent test pollution
    try:
        from fraiseql.gql.builders.registry import SchemaRegistry
        registry = SchemaRegistry.get_instance()
        registry.mutations.clear()
    except ImportError:
        pass
```

## Test Results

### Before Fix
- ❌ 7 tests failing
- ❌ CI failing on all workflows
- ❌ Quality gate blocked

### After Fix
- ✅ All 7 previously failing tests pass
- ✅ Full test suite passes (4,301 passed, 13 skipped)
- ✅ All CI workflows pass
- ✅ Quality gate open

## Release Process

1. Created branch: `fix/mutation-union-type-wrapping`
2. Made 4 commits with fixes
3. Created PR #129
4. All CI checks passed
5. Squashed and merged to `dev`
6. Created tag `v1.4.1`
7. Pushed tag to trigger publish workflow
8. Release published successfully

## Published Artifacts

- **macOS ARM64**: `fraiseql-1.4.0-cp313-cp313-macosx_11_0_arm64.whl`
- **macOS ARM64**: `fraiseql-1.4.0-cp314-cp314-macosx_11_0_arm64.whl`
- **Linux x86_64**: `fraiseql-1.4.0-cp313-cp313-manylinux_2_34_x86_64.whl`
- **Windows**: `fraiseql-1.4.0-cp313-cp313-win_amd64.whl`
- **Windows**: `fraiseql-1.4.0-cp314-cp314-win_amd64.whl`
- **Source**: `fraiseql-1.4.0.tar.gz`

**Note**: Wheel filenames show `1.4.0` because version wasn't bumped before tag creation.
This was corrected in post-release commit d513d9e.

## Version Consistency Fix

After release, updated version numbers to ensure consistency:
- `pyproject.toml`: 1.4.0 → 1.4.1
- `src/fraiseql/__init__.py`: 1.3.4 → 1.4.1

**Commit**: d513d9e - "chore: Bump version to 1.4.1"

## Impact

### Breaking Changes
- ❌ None - fully backward compatible

### New Features
- ❌ None - bug fix release only

### Bug Fixes
- ✅ Fixed FraiseUnion wrapping in mutation decorators
- ✅ Fixed test pollution from SchemaRegistry
- ✅ Fixed support for single result type mutations
- ✅ Fixed Annotated metadata preservation

### Improvements
- ✅ More robust mutation type handling
- ✅ Better test isolation
- ✅ Cleaner test registry management

## Future Considerations

1. **Automated version bumping**: Consider using tools like `bump2version` or `semantic-release`
2. **Pre-release version sync**: Add CI check to ensure versions are consistent before tagging
3. **Version source of truth**: Consider using `setuptools_scm` to derive version from git tags
4. **Release automation**: Improve release workflow to handle version bumping automatically

## Timeline

- **2025-11-10 00:14**: Initial CI failures detected
- **2025-11-10 00:21**: Root cause analysis completed
- **2025-11-10 00:25**: First fix committed
- **2025-11-10 00:30**: Second fix committed
- **2025-11-10 00:35**: Third fix committed
- **2025-11-10 00:43**: Fourth fix committed (test pollution)
- **2025-11-10 01:26**: PR #129 created
- **2025-11-10 01:33**: PR merged, tag v1.4.1 created
- **2025-11-10 01:40**: Release v1.4.1 published
- **2025-11-10 01:45**: Version consistency fix committed

## Contributors

- **Primary Developer**: Claude Code (AI Assistant)
- **User/Reviewer**: @evoludigit

---

**Generated**: 2025-11-10
**Release**: v1.4.1
**Type**: Bug Fix Release
**Status**: ✅ Complete
