# Version Adaptation: v1.9.0 → v1.8.0-beta.1

## Change Summary

The implementation plans have been adapted to incorporate "Validation as Error Type" into **v1.8.0-beta.1** instead of creating a new v1.9.0 release.

### Why This Change?

- v1.8.0-beta.1 has **not been released yet**
- v1.8.0 is already a breaking change (CASCADE feature)
- Combining both features into one beta release makes sense
- Avoids confusion with multiple major versions

---

## What Changed in the Plans

### 1. Directory Renamed
```bash
# Before
.phases/validation-as-error-v1.9.0/

# After
.phases/validation-as-error-v1.8.0/
```

### 2. Version References Updated

All references updated throughout the plan:
- `v1.9.0` → `v1.8.0`
- `1.9.0` → `1.8.0`
- `v1.8.x` → `v1.7.x` (previous version)
- `1.8.x` → `1.7.x` (previous version)

**Files affected:**
- `00_OVERVIEW.md`
- `01_PHASE_1_RUST_CORE.md`
- `02_PHASE_2_PYTHON_LAYER.md`
- `03_PHASE_3_SCHEMA_GENERATION.md`
- `04_PHASE_4_TESTING_DOCS.md`
- `05_PHASE_5_VERIFICATION_RELEASE.md`
- `README.md`
- `QUICK_REFERENCE.md`
- `PHASE_3_ENHANCEMENTS.md`

### 3. Key Plan Updates

#### 00_OVERVIEW.md
```markdown
# Before
**Breaking Change:** Yes (major version bump to v1.9.0)

# After
**Breaking Change:** Yes (part of v1.8.0 CASCADE feature)
**Status:** To be included in v1.8.0-beta.1 (not yet released)
```

#### Rollout Strategy
```markdown
# Before
**Deliverable:** v1.9.0-beta.1

# After
**Deliverable:** v1.8.0-beta.1 (includes CASCADE + validation-as-error)

**Note:** This is being incorporated into v1.8.0-beta.1, which already includes:
- CASCADE selection filtering (v1.8.0-alpha.1 through v1.8.0-alpha.5)
- Validation as Error type (this plan)
```

#### 05_PHASE_5_VERIFICATION_RELEASE.md
```markdown
# Before
**Deliverable:** FraiseQL v1.9.0-beta.1 → v1.9.0 GA

# After
**Deliverable:** FraiseQL v1.8.0-beta.1 (includes CASCADE + validation-as-error)

**Note:** Since v1.8.0-beta.1 has not been released yet, we incorporate this directly.
No need for a separate beta release - this becomes part of the existing v1.8.0 beta plan.
```

#### Version Bump Strategy
```markdown
# Before
sed -i 's/version = "1.8.0"/version = "1.9.0-beta.1"/' pyproject.toml

# After
**No separate version bump needed** - we're incorporating this into the existing v1.8.0-beta.1 plan.

# Current: version = "1.8.0-alpha.5" (CASCADE feature)
# Will become: version = "1.8.0-beta.1" (CASCADE + validation-as-error)
```

---

## v1.8.0-beta.1 Feature Set

The combined v1.8.0-beta.1 will include:

### Feature 1: CASCADE Selection Filtering
**Status:** Already implemented (alpha.1 - alpha.5)
- Efficient CASCADE field selection
- Schema-level filtering
- Performance optimizations

### Feature 2: Validation as Error Type
**Status:** To be implemented (this plan)
- Validation failures → Error type (not Success)
- Error type includes `code` field (422, 404, 409, 500)
- Success type always has non-null entity
- Union types for all mutations

---

## Implementation Timeline

### Current Status
```
v1.8.0-alpha.5 (CASCADE feature)
    ↓
[Implement Phases 1-5 of this plan]
    ↓
v1.8.0-beta.1 (CASCADE + validation-as-error)
    ↓
[Beta testing period]
    ↓
v1.8.0 GA
```

### No Changes to Implementation Steps
- All 5 phases remain the same
- Code changes remain identical
- Test updates remain identical
- Only version numbers in documentation changed

---

## Migration Guide Updates

### Code Examples
```python
# Before (v1.7.x) ← Changed from v1.8.x
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌ Nullable

# After (v1.8.0) ← Changed from v1.9.0
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Non-nullable
```

### GraphQL Examples
```graphql
# Before (v1.7.x) ← Changed from v1.8.x
type CreateMachineSuccess {
  machine: Machine    # Nullable
}

# After (v1.8.0) ← Changed from v1.9.0
union CreateMachineResult = CreateMachineSuccess | CreateMachineError

type CreateMachineSuccess {
  machine: Machine!   # Non-null
}

type CreateMachineError {
  code: Int!
  status: String!
  message: String!
}
```

---

## GitHub Release Notes Update

### v1.8.0-beta.1 Release Notes

```markdown
# FraiseQL v1.8.0-beta.1

🚨 **BREAKING CHANGES** - Major mutation error handling improvements

## Summary

This beta combines TWO major features:

### Feature 1: CASCADE Selection Filtering (alpha.1-5)
- ✅ Efficient field selection in CASCADE metadata
- ✅ Schema-level filtering
- ✅ Performance optimizations

### Feature 2: Validation as Error Type (NEW)
- ✅ Validation failures now return Error type (not Success)
- ✅ Error type includes REST-like `code` field (422, 404, 409, 500)
- ✅ Success type entity is always non-null
- ✅ Type-safe union types for all mutations

## Migration Required

**Before upgrading:**
1. Read [Migration Guide](https://fraiseql.io/docs/migrations/v1.8.0)
2. Update Success types (remove nullable entities)
3. Update Error types (add `code` field)
4. Update test assertions
5. Update GraphQL fragments (handle unions)

## Breaking Changes

### Validation Failures → Error Type
- ❌ BREAKING: `noop:*` statuses now return Error type
- ❌ BREAKING: Success types must have non-null entity
- ❌ BREAKING: All mutations return union types

### Code Examples

**Before (v1.7.x):**
```python
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌ Nullable
```

**After (v1.8.0):**
```python
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Non-nullable

@fraiseql.failure
class CreateMachineError:
    code: int  # ✅ NEW: REST-like code
    status: str
    message: str
```

## Installation

```bash
pip install --upgrade fraiseql==1.8.0b1
```

## Documentation

- [Migration Guide](https://fraiseql.io/docs/migrations/v1.8.0)
- [CASCADE Feature](https://fraiseql.io/docs/cascade)
- [API Reference](https://fraiseql.io/docs/api/v1.8.0)

## Changelog

See [CHANGELOG.md](https://github.com/fraiseql/fraiseql/blob/main/CHANGELOG.md)
```

---

## Affected PrintOptim Dependencies

### Before
```toml
[dependencies]
fraiseql = "^1.8.0"  # Would need update to 1.9.0
```

### After
```toml
[dependencies]
fraiseql = "^1.8.0"  # No change needed, just update to beta.1
```

**Benefit:** PrintOptim doesn't need to update dependency version constraints, just update to v1.8.0-beta.1 when ready.

---

## Summary of Changes

| Aspect | Before (v1.9.0 plan) | After (v1.8.0 plan) |
|--------|---------------------|---------------------|
| **Version** | v1.9.0-beta.1 | v1.8.0-beta.1 |
| **Previous version** | v1.8.x | v1.7.x |
| **Directory** | `validation-as-error-v1.9.0/` | `validation-as-error-v1.8.0/` |
| **Breaking change** | New major version | Part of existing v1.8.0 |
| **Dependencies** | Need to update to 1.9.0 | Stay on 1.8.0 |
| **Release strategy** | Separate beta release | Combined with CASCADE |
| **Implementation** | No change | No change |

---

## Next Steps

1. ✅ Version numbers updated in all plans
2. ✅ Directory renamed
3. ✅ Rollout strategy updated
4. ✅ Release notes updated
5. → **Ready to implement Phases 1-5**
6. → Release as v1.8.0-beta.1 (combined with CASCADE)

---

**The implementation plans are now correctly adapted for v1.8.0-beta.1.**
