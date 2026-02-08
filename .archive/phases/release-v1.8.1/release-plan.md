# FraiseQL v1.8.1 Release Plan

**Status**: Ready for Execution
**Created**: 2025-12-13
**Target Release Date**: 2025-12-13

---

## Overview

Release v1.8.1 with @error decorator rename, custom scalar WHERE filtering support, and WHERE clause improvements.

**Important**: Despite the CHANGELOG showing v1.8.1 dated 2025-12-12, this version was **never actually tagged/released**. This is the official v1.8.1 release.

**Version Progression**:
- v1.8.0 (released, tagged) - Base version
- **v1.8.1 (this release, NOT YET TAGGED)** - @error decorator rename, custom scalar WHERE support, WHERE improvements

---

## Release Highlights

### 🎯 Major Features

#### 1. Custom Scalar WHERE Clause Support
All 54 custom scalar types now support WHERE filtering with standard operators.

**Impact**: Makes custom scalars fully functional across the entire FraiseQL pipeline.

**Example**:
```python
from fraiseql.types.scalars import EmailScalar, CIDRScalar

@fraise_type(sql_source="users")
class User:
    email: EmailScalar
    ip_address: CIDRScalar

# WHERE filtering now works:
query {
    users(where: {email: {contains: "@company.com"}}) {
        email
    }
}
```

**Supported Operators**: eq, ne, in, notIn, contains, startsWith, endsWith

**Known Limitation**: JSON/dict-valued scalars (JSONScalar) cannot use standard WHERE operators due to parser conflicts. Documented in code.

#### 2. Automatic Field Name Conversion in WHERE Clauses
WHERE clauses automatically convert GraphQL camelCase to database snake_case.

**Impact**: Eliminates manual field name conversion in WHERE clauses.

**Example**:
```python
# Before: Required snake_case
where = {"ip_address": {"eq": "192.168.1.1"}}

# After: camelCase works automatically
where = {"ipAddress": {"eq": "192.168.1.1"}}  # Converts to ip_address
```

#### 3. Deep Nested WHERE Clause Support
WHERE clauses now support arbitrary nesting depth.

**Impact**: Fixes "Invalid operator" errors for deeply nested queries.

---

## Pre-Release Checklist

### Code & Tests
- [x] All tests passing (167/167 scalar tests)
- [x] No regressions in existing functionality
- [x] Custom scalar WHERE support implemented
- [x] Known limitations documented

### Documentation
- [ ] Update CHANGELOG.md with v1.8.1 entry
- [ ] Move Unreleased section to v1.8.1
- [ ] Add custom scalar WHERE support to changelog
- [ ] Update version numbers (pyproject.toml, __init__.py)

### Version Management
- [ ] Create release branch `release/v1.8.1` from current HEAD
- [ ] Update version to 1.8.1
- [ ] Tag release as `v1.8.1`
- [ ] Merge to main/dev branch

---

## Release Steps

### Step 1: Create Release Branch
```bash
# Ensure we're on the right branch
git checkout feature/rename-failure-to-error

# Create release branch
git checkout -b release/v1.8.1

# Verify commits since v1.8.0 (v1.8.1 doesn't exist yet)
git log v1.8.0..HEAD --oneline
```

**Expected commits to include**:
- 8681321e feat(where): enable WHERE clause filtering for custom scalar types
- a4d87cde refactor(where): clean up custom scalar filter generation
- 23a38f93 test(where): add tests for custom scalar WHERE filters
- c0f4c555 feat(graphql): support custom scalars as field types
- c05cb25d feat(graphql): support custom scalars as field types in queries and types
- fcd69eb4 fix(schema): ensure PageInfo type consistency in registry cache
- e88f2c65 test(connection): unskip integration tests
- cc5b9513 refactor(decorators): preserve type annotations on @connection wrapper

---

### Step 2: Update Version Numbers

#### File 1: pyproject.toml
```toml
# Change from:
version = "1.8.0"

# To:
version = "1.8.1"
```

#### File 2: src/fraiseql/__init__.py
```python
# Change from:
__version__ = "1.8.0"

# To:
__version__ = "1.8.1"
```

#### File 3: README.md
```markdown
# Change from:
**📍 You are here: Main FraiseQL Framework (v1.8.0-beta.5) - Beta Release**

**Current Version**: v1.8.0b5 | **Status**: Beta | **Python**: 3.13+ | **PostgreSQL**: 13+

# To:
**📍 You are here: Main FraiseQL Framework (v1.8.1) - Stable Release**

**Current Version**: v1.8.1 | **Status**: Stable | **Python**: 3.13+ | **PostgreSQL**: 13+
```

---

### Step 3: Update CHANGELOG.md

Move Unreleased section to v1.8.1 and add custom scalar features:

```markdown
## [Unreleased]

(Empty - ready for next development)

## [1.8.1] - 2025-12-13

### Features

#### Custom Scalar WHERE Clause Filtering
- All 54 custom scalar types now support WHERE clause filtering
- Standard operators work: eq, ne, in, notIn, contains, startsWith, endsWith
- Completes custom scalar integration across the entire pipeline
- Fully tested with 167/167 tests passing

**Example**:
```python
from fraiseql.types.scalars import EmailScalar, CIDRScalar, PhoneNumberScalar

@fraise_type(sql_source="users")
class User:
    email: EmailScalar
    phone: PhoneNumberScalar
    ip_address: CIDRScalar

# All WHERE operators work:
query {
    users(where: {email: {contains: "@company.com"}}) { email }
    users(where: {phone: {startsWith: "+1"}}) { phone }
    users(where: {ipAddress: {eq: "192.168.1.0/24"}}) { ipAddress }
}
```

**Known Limitation**: JSON/dict-valued scalars (JSONScalar) cannot use standard WHERE operators because the parser interprets dict keys as filter operators. Use specialized JSONB operators or filter on JSON paths instead. See `src/fraiseql/where_clause.py` for details.

#### Automatic Field Name Conversion in WHERE Clauses
- WHERE clauses now automatically convert GraphQL camelCase field names to database snake_case
- Supports arbitrary nesting levels (e.g., `machine.network.ipAddress`)
- Backward compatible - existing snake_case field names work unchanged
- Applies to both dict-based and WhereInput-based WHERE clauses

**Examples**:
```python
# GraphQL camelCase (now works automatically)
where = {"ipAddress": {"eq": "192.168.1.1"}}
# Converts to: {"ip_address": {"eq": "192.168.1.1"}}

# Deep nesting
where = {"machine": {"network": {"ipAddress": {"eq": "192.168.1.1"}}}}
# Converts all levels: machine → machine, network → network, ipAddress → ip_address
```

### Fixes

#### Deep Nested WHERE Clause Support
- Fixed WHERE clause processing to handle arbitrary levels of nesting
- Previously only supported 1 level of nesting, now supports unlimited depth
- Resolves "Invalid operator" errors for deeply nested GraphQL queries

#### PageInfo Type Consistency
- Fixed PageInfo type caching to ensure single instance across schema
- Prevents "duplicate type" errors in complex schemas with multiple connections

#### Connection Decorator Type Annotations
- Fixed @connection decorator to preserve original function type annotations
- Resolves type checker warnings and improves IDE autocomplete

## [1.8.1] - 2025-12-12

(existing content remains unchanged)
```

---

### Step 4: Commit Version Updates
```bash
git add pyproject.toml src/fraiseql/__init__.py README.md CHANGELOG.md
git commit -m "chore(release): prepare v1.8.1 release

- Update version to 1.8.1 in all files
- Update README to reflect stable release (remove beta)
- Move Unreleased features to v1.8.1 in CHANGELOG
- Add custom scalar WHERE filtering documentation
"
```

---

### Step 5: Create and Push Tag
```bash
# Create annotated tag
git tag -a v1.8.1 -m "Release v1.8.1: Custom Scalar WHERE Support

Major Features:
- Custom scalar WHERE clause filtering (all 54 scalars)
- Automatic camelCase → snake_case conversion in WHERE
- Deep nested WHERE clause support

Fixes:
- PageInfo type consistency
- Connection decorator type annotations
"

# Verify tag
git show v1.8.1

# Push tag to remote
git push origin v1.8.1
```

---

### Step 6: Merge to Main Branch
```bash
# Switch to main/dev branch (whichever is primary)
git checkout dev  # or main

# Merge release branch
git merge release/v1.8.1 --no-ff -m "Merge release/v1.8.1 into dev

Release v1.8.1 with custom scalar WHERE support and WHERE improvements.
"

# Push to remote
git push origin dev
```

---

### Step 7: Clean Up (Optional)
```bash
# Delete release branch locally
git branch -d release/v1.8.1

# Delete release branch remotely (if pushed)
git push origin --delete release/v1.8.1
```

---

## Verification

### Before Release
```bash
# Verify all tests pass
uv run pytest tests/integration/meta/test_all_scalars.py -v
# Expected: 167 passed, 1 skipped

# Verify no uncommitted changes
git status

# Verify version numbers updated
grep "version.*1\.8\.1" pyproject.toml
grep "__version__.*1\.8\.1" src/fraiseql/__init__.py
```

### After Release
```bash
# Verify tag exists
git tag -l | grep v1.8.1

# Verify tag points to correct commit
git show v1.8.1

# Verify remote has tag
git ls-remote --tags origin | grep v1.8.1
```

---

## Communication

### Release Notes (GitHub/GitLab)
```markdown
# FraiseQL v1.8.1

## 🎉 Custom Scalar WHERE Support

All 54 custom scalar types now support WHERE clause filtering! This completes the custom scalar integration, making them fully functional across the entire FraiseQL pipeline.

### What's New

✅ **WHERE Filtering for Custom Scalars**
- Filter on EmailScalar, CIDRScalar, PhoneNumberScalar, and 51 other scalars
- Standard operators: eq, ne, in, notIn, contains, startsWith, endsWith
- 167/167 tests passing

✅ **Automatic Field Name Conversion**
- Write WHERE clauses in camelCase, auto-converts to snake_case
- Works at any nesting level

✅ **Deep Nested WHERE Support**
- Fixed unlimited nesting depth in WHERE clauses

### Examples

```python
from fraiseql.types.scalars import EmailScalar

@fraise_type
class User:
    email: EmailScalar

# Filter by custom scalar
query {
    users(where: {email: {contains: "@company.com"}}) {
        email
    }
}
```

### Known Limitations

JSONScalar cannot use standard WHERE operators due to parser conflicts. Use JSONB-specific operators instead. See documentation for details.

### Full Changelog

See CHANGELOG.md for complete list of changes.
```

---

## Rollback Plan

If issues are discovered after release:

### Option 1: Hotfix Release (v1.8.2)
```bash
# Create hotfix branch from v1.8.1
git checkout -b hotfix/v1.8.2 v1.8.1

# Apply fixes
# ... make changes ...

# Create v1.8.2 release
# Follow same release process
```

### Option 2: Revert Tag (Nuclear Option)
```bash
# Delete tag locally
git tag -d v1.8.1

# Delete tag remotely
git push origin :refs/tags/v1.8.1

# Revert merge commit on main
git revert <merge-commit-sha>
```

**Recommendation**: Use Option 1 (hotfix) unless there's a critical security issue.

---

## Post-Release Tasks

- [ ] Update documentation site (if applicable)
- [ ] Announce release on social media/blog
- [ ] Update examples/tutorials using custom scalars
- [ ] Monitor for issues in the first 24-48 hours
- [ ] Create GitHub release from tag with release notes

---

## Success Criteria

- [x] All tests passing (167/167)
- [ ] Version updated to 1.8.1 in all files
- [ ] CHANGELOG updated with v1.8.1 entry
- [ ] Tag v1.8.1 created and pushed
- [ ] Release branch merged to main/dev
- [ ] No regressions reported

---

## Timeline

**Duration**: ~30 minutes - 1 hour

| Task | Time | Status |
|------|------|--------|
| Create release branch | 5 min | ⏳ Pending |
| Update versions | 5 min | ⏳ Pending |
| Update CHANGELOG | 10 min | ⏳ Pending |
| Commit changes | 2 min | ⏳ Pending |
| Create tag | 3 min | ⏳ Pending |
| Merge to main | 5 min | ⏳ Pending |
| Push tag & branch | 2 min | ⏳ Pending |
| Verification | 5 min | ⏳ Pending |
| Write release notes | 10 min | ⏳ Pending |

**Total**: ~47 minutes

---

**Ready to execute**: This plan can be followed step-by-step to release v1.8.1 safely and completely.
