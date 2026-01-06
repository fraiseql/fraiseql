# Phase 5: Verification & Release

**Timeline:** Immediate (part of v1.8.0-beta.1)
**Risk Level:** LOW (verification only)
**Dependencies:** Phases 1-4
**Deliverable:** FraiseQL v1.8.0-beta.1 (includes CASCADE + validation-as-error)

**Note:** Since v1.8.0-beta.1 has not been released yet, we incorporate this directly.
No need for a separate beta release - this becomes part of the existing v1.8.0 beta plan.

---

## Objective

1. Final verification of all changes
2. Performance benchmarking
3. Beta release
4. Gather feedback
5. Final release
6. Coordinate downstream migration (PrintOptim)

---

## Verification Steps

### Step 5.1: Comprehensive Test Suite

**Run full test suite:**

```bash
# Unit tests
uv run pytest tests/unit/ -v

# Integration tests
uv run pytest tests/integration/ -v

# Mutation-specific tests
uv run pytest tests/integration/graphql/mutations/ -v

# CASCADE tests
uv run pytest tests/integration/test_graphql_cascade.py -v

# Full suite
uv run pytest tests/ -v --cov=fraiseql --cov-report=html
```

**Expected Results:**
- All tests pass ✅
- Code coverage ≥ 90%
- No regressions from v1.7.x

---

### Step 5.2: Type Checking

**Run mypy:**

```bash
uv run mypy src/fraiseql --strict
```

**Expected Results:**
- No type errors
- Success/Error types validate correctly
- Union types recognized

---

### Step 5.3: Rust Tests

**Run Rust test suite:**

```bash
cd fraiseql_rs
cargo test --all-features
cargo test --release
```

**Expected Results:**
- All tests pass
- Response builder works correctly
- Status classification accurate

---

### Step 5.4: Performance Benchmarking

**Run benchmarks:**

```bash
cd fraiseql_rs
cargo bench --bench mutation_benchmark

# Compare with v1.7.x baseline
cargo bench --bench mutation_benchmark -- --baseline v1.7.x
```

**Acceptance Criteria:**
- No performance regression (< 5% slower)
- Memory usage unchanged
- Response time within ±10ms

**Key Metrics:**
- Response builder throughput
- Error type allocation overhead
- Union type resolution speed

---

### Step 5.5: Schema Validation

**Validate GraphQL schema:**

```python
from fraiseql.schema import validate_schema

# Validate generated schema conforms to GraphQL spec
errors = validate_schema()
assert len(errors) == 0, f"Schema validation errors: {errors}"
```

**Check introspection:**

```graphql
query IntrospectMutations {
  __schema {
    mutationType {
      fields {
        name
        type {
          kind
          ofType {
            kind
            name
            possibleTypes {
              name
            }
          }
        }
      }
    }
  }
}
```

**Expected:**
- All mutations return union types
- Union types include Success and Error
- Introspection works correctly

---

### Step 5.6: Integration Testing

**Test with real database:**

```bash
# Start test database
docker-compose up -d postgres

# Run integration tests
uv run pytest tests/integration/ --db-url=postgresql://localhost/fraiseql_test

# Cleanup
docker-compose down
```

**Test Scenarios:**
- Validation failures → Error type (422)
- Not found → Error type (404)
- Conflicts → Error type (409)
- Success → Success type with entity
- CASCADE with errors
- CASCADE with success

---

## Beta Release Process

### Step 5.7: Pre-Release Checklist

**Code:**
- [ ] All phases (1-4) complete
- [ ] All tests passing
- [ ] No type errors
- [ ] Performance benchmarks acceptable
- [ ] Schema validates

**Documentation:**
- [ ] Migration guide complete
- [ ] API reference updated
- [ ] Status strings doc updated
- [ ] Changelog prepared
- [ ] Code examples added

**Quality:**
- [ ] Code review complete
- [ ] Security review (if needed)
- [ ] Backward compatibility checked
- [ ] Breaking changes documented

---

### Step 5.8: Version Bump

**No separate version bump needed** - we're incorporating this into the existing v1.8.0-beta.1 plan.

**Current status:**
```bash
# Check current version
grep 'version =' pyproject.toml
# Should show: version = "1.8.0-alpha.5" (CASCADE feature)

# This work will be incorporated before releasing v1.8.0-beta.1
# v1.8.0-beta.1 will include BOTH:
# - CASCADE selection filtering (alpha.1-5)
# - Validation as Error type (this plan)
```

**When ready to release v1.8.0-beta.1 (after implementing all 5 phases):**
```bash
# Update to beta.1
sed -i 's/version = "1.8.0-alpha.5"/version = "1.8.0-beta.1"/' pyproject.toml
sed -i 's/version = "1.8.0-alpha.5"/version = "1.8.0-beta.1"/' fraiseql_rs/Cargo.toml
echo '__version__ = "1.8.0-beta.1"' > src/fraiseql/__version__.py
```

---

### Step 5.9: Build & Test Beta

**Build packages:**

```bash
# Build Python package
uv build

# Build Rust library
cd fraiseql_rs
cargo build --release

# Verify wheel
ls -lh dist/
```

**Test beta installation:**

```bash
# Create fresh venv
python -m venv test-venv
source test-venv/bin/activate

# Install beta
pip install dist/fraiseql-1.8.0b1-*.whl

# Run smoke tests
python -c "import fraiseql; print(fraiseql.__version__)"
```

---

### Step 5.10: Beta Release

**Publish to PyPI (Test):**

```bash
# Upload to Test PyPI
uv publish --repository testpypi

# Test installation
pip install --index-url https://test.pypi.org/simple/ fraiseql==1.8.0b1
```

**Publish to PyPI (Production):**

```bash
# Tag release
git tag v1.8.0-beta.1
git push origin v1.8.0-beta.1

# Publish to PyPI
uv publish
```

**GitHub Release:**

```markdown
# FraiseQL v1.8.0-beta.1

🚨 **BREAKING CHANGES** - Major mutation error handling improvements

## Summary

This beta combines TWO major features:

- Validation failures now return **Error type** (not Success with null entity)
- Error type includes **REST-like `code` field** (422, 404, 409, 500)
- Success type entity is **always non-null**
- All mutations return **union types**

## Migration Required

**Before upgrading:**
1. Read [Migration Guide](https://fraiseql.io/docs/migrations/v1.8.0)
2. Update Success types (remove nullable entities)
3. Update Error types (add `code` field)
4. Update test assertions
5. Update GraphQL fragments (handle unions)

## What Changed

### Before (v1.7.x)
```json
{
  "__typename": "CreateMachineSuccess",
  "machine": null,
  "cascade": {"status": "noop:invalid_contract_id"}
}
```

### After (v1.8.0)
```json
{
  "__typename": "CreateMachineError",
  "code": 422,
  "status": "noop:invalid_contract_id",
  "message": "Contract not found"
}
```

## Beta Testing

This is a **beta release** for testing and feedback:

- Beta period: 1 week (2024-12-XX to 2024-12-XX)
- Please report issues: [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)
- Feedback: [Discussions](https://github.com/fraiseql/fraiseql/discussions)

## Installation

```bash
pip install fraiseql==1.8.0b1
```

## Documentation

- [Migration Guide](https://fraiseql.io/docs/migrations/v1.8.0)
- [API Reference](https://fraiseql.io/docs/api/v1.8.0)
- [Status Strings](https://fraiseql.io/docs/mutations/status-strings)

## Changelog

### Breaking Changes
- Validation failures (`noop:*`) now return Error type, not Success
- Success types must have non-null entity
- Error types require `code: int` field
- All mutations return union types

### Added
- REST-like `code` field in Error type (422, 404, 409, 500)
- Schema validation for mutation types
- Migration guide with examples

### Changed
- Moved `noop:*` from `error_as_data_prefixes` to `error_prefixes`
- Updated response builder to return Error for all non-success statuses
- Updated schema generation to create union types

### Deprecated
- `error_as_data_prefixes` (use `error_prefixes` instead)
- `always_return_as_data` flag
- `STRICT_STATUS_CONFIG` (use `DEFAULT_ERROR_CONFIG`)

### Fixed
- Type safety: Success type can no longer have null entity
- Semantic clarity: Validation errors properly categorized

## Full Changelog

See [CHANGELOG.md](https://github.com/fraiseql/fraiseql/blob/main/CHANGELOG.md)
```

---

## Beta Feedback Period

### Step 5.11: Beta Testing (1 Week)

**Announce beta:**
- [ ] GitHub Discussions post
- [ ] Discord/Slack notification
- [ ] Email to known users
- [ ] Social media announcement

**Track feedback:**
- [ ] Create GitHub project for v1.8.0
- [ ] Monitor issues for beta-related problems
- [ ] Engage with users testing beta
- [ ] Document common issues

**Key Questions:**
1. Do migrations work smoothly?
2. Are there edge cases we missed?
3. Is documentation clear?
4. Any performance issues?
5. Client library compatibility?

---

### Step 5.12: Address Beta Feedback

**Fix critical issues:**
- [ ] P0: Blocking issues (prevent migration)
- [ ] P1: Major issues (significant pain points)
- [ ] P2: Minor issues (nice-to-haves)

**Update documentation:**
- [ ] Add FAQs based on feedback
- [ ] Clarify confusing sections
- [ ] Add more examples if needed

**Release beta.2 if needed:**

```bash
# If critical fixes required
git tag v1.8.0-beta.2
uv publish
```

---

## Final Release

### Step 5.13: Final Release Checklist

**Prerequisites:**
- [ ] Beta period complete (1+ week)
- [ ] All P0/P1 issues resolved
- [ ] No regressions reported
- [ ] Documentation finalized
- [ ] PrintOptim team ready for migration

**Final Verification:**
- [ ] All tests pass on main branch
- [ ] Performance benchmarks acceptable
- [ ] Schema validates
- [ ] Docs site updated
- [ ] CHANGELOG complete

---

### Step 5.14: Release v1.8.0 GA

**Version bump to GA:**

```bash
# Update version to final
sed -i 's/version = "1.8.0-beta.1"/version = "1.8.0"/' pyproject.toml
sed -i 's/version = "1.8.0-beta.1"/version = "1.8.0"/' fraiseql_rs/Cargo.toml

# Commit
git commit -am "chore: release v1.8.0"
git tag v1.8.0
git push origin main --tags
```

**Build and publish:**

```bash
# Build
uv build
cd fraiseql_rs && cargo build --release && cd ..

# Publish
uv publish

# Verify
pip install fraiseql==1.8.0
```

**GitHub Release:**

```markdown
# FraiseQL v1.8.0

🎉 **Major Release** - Validation as Error Type

## Summary

FraiseQL v1.8.0 implements a major architectural improvement to mutation error handling, following recommendations from Tim Berners-Lee's architectural review.

**Key Improvements:**
- ✅ Type safety: Success type always has non-null entity
- ✅ REST-like codes: Error type includes `code` field (422, 404, 409, 500)
- ✅ Semantic clarity: Validation failures properly return Error type
- ✅ GraphQL compliant: HTTP 200 OK, errors in type system

## Breaking Changes

⚠️ **Migration Required** - This release includes breaking changes.

See [Migration Guide](https://fraiseql.io/docs/migrations/v1.8.0) for complete upgrade instructions.

### Quick Summary

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
pip install --upgrade fraiseql==1.8.0
```

## Documentation

- [Migration Guide](https://fraiseql.io/docs/migrations/v1.8.0)
- [API Reference](https://fraiseql.io/docs/api/v1.8.0)
- [What's New](https://fraiseql.io/blog/v1.8.0-release)

## Changelog

[Full Changelog](https://github.com/fraiseql/fraiseql/blob/main/CHANGELOG.md)

## Support

Need help migrating?
- [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- [Discord](https://discord.gg/fraiseql)
- [Migration Examples](https://github.com/fraiseql/fraiseql/tree/main/examples/v1.8.0-migration)

## Acknowledgments

Special thanks to Tim Berners-Lee for the architectural review that inspired this improvement, and to the FraiseQL community for beta testing.
```

---

## Post-Release Tasks

### Step 5.15: Documentation Site

**Update docs site:**
- [ ] Set v1.8.0 as default version
- [ ] Add v1.7.x to "Previous Versions"
- [ ] Publish migration guide
- [ ] Publish blog post
- [ ] Update homepage examples

**Write blog post:**

```markdown
# FraiseQL v1.8.0: Validation as Error Type

Today we're excited to announce FraiseQL v1.8.0, a major release that improves mutation error handling...

## The Problem

In v1.7.x, validation failures returned Success type with null entity...

## The Solution

Following architectural review from Tim Berners-Lee, v1.8.0 implements...

## Migration

Upgrading is straightforward...

## Thank You

Thanks to our community for beta testing...
```

---

### Step 5.16: Coordinate Downstream Migration

**PrintOptim Backend:**

**Communication:**
```markdown
Subject: FraiseQL v1.8.0 Released - Migration Plan

Hi PrintOptim Team,

FraiseQL v1.8.0 is now available. This release includes breaking changes
that require code updates.

## What You Need to Do

1. Review migration guide: https://fraiseql.io/docs/migrations/v1.8.0
2. Update ~30-50 test assertions (validation errors now return Error type)
3. Update GraphQL fragments to handle union types
4. Update frontend error handling

## Timeline

- Week 4: Update PrintOptim to use FraiseQL v1.8.0
- Week 5: Testing on staging
- Week 6: Production deployment

## Support

I'm available for:
- Migration questions
- Pair programming sessions
- Code review
- Troubleshooting

Let me know when you're ready to start!
```

**Migration Support:**
- [ ] Schedule kickoff meeting
- [ ] Provide migration examples
- [ ] Offer pair programming
- [ ] Review their PRs
- [ ] Test on staging
- [ ] Monitor production deployment

---

## Success Criteria

### Release Metrics
- [ ] v1.8.0 published to PyPI
- [ ] GitHub release created
- [ ] Docs site updated
- [ ] Blog post published
- [ ] Zero regressions reported
- [ ] Performance within acceptable range

### Adoption Metrics (Track)
- Downloads from PyPI
- GitHub stars/watchers
- Issue reports (bugs vs questions)
- Migration completion rate

### PrintOptim Migration
- [ ] PrintOptim upgraded to v1.8.0
- [ ] All tests passing
- [ ] Deployed to staging
- [ ] Deployed to production
- [ ] No regressions

---

## Rollback Plan

If critical issues discovered post-release:

**Option 1: Hotfix**
```bash
# Create hotfix branch
git checkout -b hotfix/v1.9.1 v1.8.0

# Fix critical issue
git commit -am "fix: critical issue"

# Release v1.9.1
git tag v1.9.1
uv publish
```

**Option 2: Yank Release**
```bash
# Yank from PyPI (if severely broken)
pip install twine
twine yank fraiseql 1.8.0

# Communicate to users
echo "v1.8.0 yanked due to critical issue. Use v1.7.x until v1.9.1."
```

**Option 3: Deprecation**
```bash
# If fundamentally flawed, deprecate and plan v2.0
echo "v1.8.0 deprecated. v2.0.0 in development with revised approach."
```

---

## Timeline Summary

| Week | Phase | Deliverable |
|------|-------|-------------|
| Week 1 | Phases 1-2 | Rust + Python core changes |
| Week 2 | Phases 3-4 | Schema + Testing + Docs |
| Week 3 | Phase 5 | Beta release + feedback |
| Week 4 | Phase 5 | GA release + PrintOptim start |
| Week 5 | Post-release | PrintOptim testing |
| Week 6 | Post-release | PrintOptim production |

---

## Final Checklist

### Pre-Release
- [ ] All phases (1-4) complete
- [ ] All tests passing
- [ ] Documentation complete
- [ ] Performance acceptable
- [ ] Beta tested (1+ week)

### Release
- [ ] Version bumped to 1.8.0
- [ ] Published to PyPI
- [ ] GitHub release created
- [ ] Docs site updated
- [ ] Blog post published

### Post-Release
- [ ] Monitor for issues
- [ ] Support early adopters
- [ ] Coordinate PrintOptim migration
- [ ] Track adoption metrics

---

**Ready to Release!** 🚀

Once all checkboxes are complete, FraiseQL v1.8.0 is ready for GA release.
