# Documentation Updates Required for v1.8.0-alpha.5

**Phase:** CASCADE Fix Release
**Status:** Ready for Updates
**Priority:** Required before PyPI release

---

## Overview

After implementing the CASCADE fix, the following documentation needs to be updated before releasing v1.8.0-alpha.5 to PyPI.

---

## Critical Updates (Required for Release)

### 1. Version Numbers

#### 1.1 Rust Package Version

**File:** `fraiseql_rs/Cargo.toml`

**Current:**
```toml
[package]
name = "fraiseql-rs"
version = "1.8.0-alpha.4"
```

**Update to:**
```toml
[package]
name = "fraiseql-rs"
version = "1.8.0-alpha.5"
```

**Line:** ~3-4

---

#### 1.2 Python Package Version

**File:** `pyproject.toml`

**Current:**
```toml
[project]
name = "fraiseql"
version = "1.8.0a4"
```

**Update to:**
```toml
[project]
name = "fraiseql"
version = "1.8.0a5"
```

**Line:** ~2-3

---

### 2. CHANGELOG.md

**File:** `CHANGELOG.md`

**Add at the top:**

```markdown
## [1.8.0-alpha.5] - 2025-12-06

### Fixed
- **CASCADE nesting bug**: CASCADE data now appears at success wrapper level instead of nested inside entity objects
  - Added support for PrintOptim's 8-field `mutation_response` composite type
  - CASCADE field extracted from Position 7 (explicit field in composite type)
  - Maintains backward compatibility with simple format responses
  - Zero breaking changes

### Added
- New `postgres_composite` module for parsing 8-field mutation_response composite types
  - File: `fraiseql_rs/src/mutation/postgres_composite.rs` (172 lines)
  - Supports PrintOptim's migration to FraiseQL v1.8.0+ standard
  - Comprehensive unit tests for composite type parsing

### Changed
- Updated mutation parser to try 8-field composite format first, then fallback to simple format
- Enhanced test coverage for mutation response handling

### Technical Details
- **Files Modified:** 29 files (885 insertions, 446 deletions)
- **Breaking Changes:** None (backward compatible)
- **Migration Required:** None (automatic)
- **PrintOptim Compatibility:** Requires fraiseql>=1.8.0a5 for CASCADE fix

### Migration Guide
No migration needed. The fix is automatic:
- PrintOptim users: Upgrade to fraiseql>=1.8.0a5 to get CASCADE at correct location
- Other users: Simple format continues working unchanged

### References
- Bug Report: CASCADE appeared in entity object instead of success wrapper
- Design Document: `/tmp/fraiseql_mutation_pipeline_design.md`
- Phase Documentation: `.phases/phase_cascade_fix_v1.8.0a5/`

---
```

**Location:** Top of file, before `## [1.8.0-alpha.4]` entry

---

## Important Updates (Should Have)

### 3. README.md

**File:** `README.md`

**Section to Update:** Installation / Changelog reference

**Add note about CASCADE fix:**

```markdown
## Recent Changes

### v1.8.0-alpha.5 (2025-12-06)
- **Fixed:** CASCADE nesting bug - CASCADE now appears at success wrapper level
- **Added:** Support for 8-field mutation_response composite type
- **Improved:** Backward compatibility with fallback parsing

See [CHANGELOG.md](./CHANGELOG.md) for complete release history.
```

**Location:** Near top of README, after initial description

---

### 4. Migration Documentation (if exists)

**Files to Check:**
- `docs/MIGRATION.md`
- `docs/UPGRADING.md`
- Any migration guides

**Add section:**

```markdown
## Upgrading to v1.8.0-alpha.5

### CASCADE Fix

**What Changed:**
- CASCADE data now appears at GraphQL success wrapper level
- Previously appeared inside entity object (bug)

**Action Required:**
- **PrintOptim users:** Upgrade to `fraiseql>=1.8.0a5`
- **Other users:** No action needed (backward compatible)

**Example:**

Before (Bug):
```json
{
  "createAllocation": {
    "allocation": {
      "cascade": { ... }  // Wrong location
    }
  }
}
```

After (Fixed):
```json
{
  "createAllocation": {
    "allocation": { ... },  // No cascade
    "cascade": { ... }      // Correct location
  }
}
```

**GraphQL Queries:**
No changes needed to GraphQL queries. CASCADE is now accessible at the correct level:

```graphql
mutation {
  createAllocation(input: $input) {
    allocation { id }
    cascade {           # ✅ Now works correctly
      updated { ... }
      invalidations { ... }
    }
  }
}
```
```

---

### 5. API Documentation

**Files to Update:**
- `docs/API.md` (if exists)
- `docs/mutations.md` (if exists)
- Any GraphQL schema documentation

**Update mutation response examples to show CASCADE at correct level**

Example:
```markdown
## Mutation Response Structure

All mutations return a response with this structure:

```graphql
type CreateAllocationSuccess {
  allocation: Allocation!
  cascade: Cascade          # ✅ CASCADE at wrapper level
  message: String!
}
```

**Note:** Prior to v1.8.0-alpha.5, CASCADE incorrectly appeared inside the entity object. This has been fixed.
```

---

## Optional Updates (Nice to Have)

### 6. Code Examples

**Files to Review:**
- `examples/` directory
- Documentation code snippets
- Tutorial files

**Update any examples showing mutation responses to have CASCADE at correct level**

---

### 7. Test Documentation

**File:** `tests/README.md` (if exists)

**Add note:**
```markdown
## Testing CASCADE Location

To verify CASCADE appears at correct location:

```python
def test_cascade_location():
    result = execute_mutation(...)

    # CASCADE at success level ✅
    assert "cascade" in result["createAllocation"]

    # CASCADE NOT in entity ✅
    assert "cascade" not in result["createAllocation"]["allocation"]
```
```

---

### 8. Developer Documentation

**Files:**
- `CONTRIBUTING.md`
- `docs/DEVELOPMENT.md`
- Developer guides

**Add note about composite type parsing:**

```markdown
## Mutation Response Handling

FraiseQL supports two mutation response formats:

1. **8-field composite type** (PrintOptim format):
   - Parsed by `postgres_composite::PostgresMutationResponse`
   - CASCADE at Position 7
   - Full metadata support

2. **Simple format** (backward compatibility):
   - Parsed by `MutationResult::from_json`
   - Entity-only responses
   - Automatic fallback

See `fraiseql_rs/src/mutation/postgres_composite.rs` for implementation details.
```

---

## Documentation Checklist

### Pre-Release (Critical)
- [ ] Update `fraiseql_rs/Cargo.toml` version to 1.8.0-alpha.5
- [ ] Update `pyproject.toml` version to 1.8.0a5
- [ ] Add v1.8.0-alpha.5 entry to CHANGELOG.md
- [ ] Verify all version references are consistent

### Post-Release (Important)
- [ ] Update README.md with recent changes section
- [ ] Update migration documentation (if exists)
- [ ] Update API documentation examples
- [ ] Review and update code examples

### Future Updates (Nice to Have)
- [ ] Add developer documentation about composite parsing
- [ ] Update test documentation
- [ ] Create troubleshooting guide for CASCADE issues
- [ ] Add diagram showing CASCADE flow

---

## Version Reference Locations

Search for version strings in these locations:

```bash
# Find all version references
grep -r "1.8.0-alpha.4" .
grep -r "1.8.0a4" .

# Key files that should be updated
fraiseql_rs/Cargo.toml
pyproject.toml
CHANGELOG.md
```

---

## Validation

After updating documentation:

```bash
# 1. Check version consistency
grep -r "version.*1.8.0" fraiseql_rs/Cargo.toml pyproject.toml

# 2. Verify CHANGELOG format
head -50 CHANGELOG.md

# 3. Check for broken links
# (use markdown link checker if available)

# 4. Build documentation
# (if using mdbook or sphinx)
```

---

## Documentation Style Guide

### Version Format
- Rust: `1.8.0-alpha.5` (with hyphens)
- Python: `1.8.0a5` (no hyphens, 'a' for alpha)

### Date Format
- Use ISO 8601: `2025-12-06`

### Changelog Categories
Use conventional commit categories:
- **Fixed** - Bug fixes
- **Added** - New features
- **Changed** - Changes to existing functionality
- **Deprecated** - Soon-to-be removed features
- **Removed** - Removed features
- **Security** - Security fixes

### Code Examples
Always show:
- Before (if bug fix)
- After (current behavior)
- Expected usage

---

## Related Files

### Design Documents
- Main design: `/tmp/fraiseql_mutation_pipeline_design.md`
- Test report: `/tmp/fraiseql_v1.8.0a4_test_report.md`
- Test output: `/tmp/cascade_v1.8.0a4_test_output.txt`

### Phase Documents
- `.phases/phase_cascade_fix_v1.8.0a5/INDEX.md`
- `.phases/phase_cascade_fix_v1.8.0a5/README.md`
- `.phases/phase_cascade_fix_v1.8.0a5/00_OVERVIEW.md`
- `.phases/phase_cascade_fix_v1.8.0a5/01_IMPLEMENTATION_PLAN.md`
- `.phases/phase_cascade_fix_v1.8.0a5/02_TESTING_STRATEGY.md`
- `.phases/phase_cascade_fix_v1.8.0a5/03_QUICK_START.md`

---

## External Documentation

### PrintOptim Updates

After FraiseQL release, PrintOptim needs:

**File:** `printoptim_backend_manual_migration/pyproject.toml`

```toml
[project.dependencies]
fraiseql = ">=1.8.0a5"  # Update from 1.8.0a4
```

### GraphQL CASCADE Spec

No updates needed to spec (already correct).
Reference: `~/code/graphql-cascade/`

---

## Timeline

### Before PyPI Publish
1. Update Cargo.toml version (5 min)
2. Update pyproject.toml version (5 min)
3. Update CHANGELOG.md (15 min)
4. Verify all changes (10 min)

**Total:** ~35 minutes

### After PyPI Publish
1. Update README.md (10 min)
2. Update migration docs (15 min)
3. Review examples (20 min)

**Total:** ~45 minutes

---

## Review Checklist

Before committing documentation updates:

- [ ] All version numbers match (1.8.0-alpha.5 / 1.8.0a5)
- [ ] CHANGELOG entry is complete and accurate
- [ ] Date is correct (2025-12-06)
- [ ] No broken references or links
- [ ] Code examples are syntactically correct
- [ ] Markdown formatting is valid
- [ ] Spelling and grammar checked

---

## Commit Message Template

```bash
git add fraiseql_rs/Cargo.toml pyproject.toml CHANGELOG.md
git commit -m "docs: update version to 1.8.0-alpha.5 and document CASCADE fix

- Bump version to 1.8.0-alpha.5 in Cargo.toml
- Bump version to 1.8.0a5 in pyproject.toml
- Add comprehensive CHANGELOG entry for CASCADE fix
- Document migration path for PrintOptim users

Part of CASCADE nesting bug fix implementation.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Next Steps

1. Review this checklist
2. Update critical documentation (versions, CHANGELOG)
3. Commit documentation updates
4. Build and test package
5. Publish to PyPI
6. Update PrintOptim dependency
7. Update remaining documentation

---

**Last Updated:** 2025-12-06
**Status:** Ready for Documentation Updates
**Estimated Time:** 1-2 hours total
