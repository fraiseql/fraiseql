# Publish Workflow Modernization Summary

## Changes Made (2025 Best Practices)

Following patterns from leading Rust+Python projects (Ruff, pydantic-core, Polars), the publish workflow has been modernized with these key improvements:

### ✅ 1. Added Source Distribution (sdist) Build

**New Job**: `build-sdist`
- Builds source distribution using `maturin sdist`
- Required for PyPI complete package metadata
- Allows users to install from source

```yaml
build-sdist:
  name: Build source distribution
  runs-on: ubuntu-latest
  steps:
    - uses: PyO3/maturin-action@v1
      with:
        maturin-version: "1.9.6"
        command: sdist
```

### ✅ 2. Added Artifact Validation Job

**New Job**: `validate`
- Validates all built artifacts before publishing
- Uses `twine check --strict` for PyPI compliance
- Verifies Rust extension `.so` files are present in wheels
- Lists all artifacts for transparency

### ✅ 3. Switched to `uv publish` (2025 Standard)

**Old**:
```yaml
- uses: pypa/gh-action-pypi-publish@release/v1
  with:
    password: ${{ secrets.PYPI_TOKEN }}
```

**New**:
```yaml
- name: Install uv
  uses: astral-sh/setup-uv@v7

- name: Publish to PyPI
  run: uv publish --trusted-publishing always
```

**Benefits**:
- Faster and more reliable publishing
- Built-in trusted publishing support (no tokens needed)
- Consistent with modern Python tooling
- Used by Ruff, pydantic-core in 2025

### ✅ 4. Pinned Maturin Version

**Changed**: `maturin-version: latest` → `maturin-version: "1.9.6"`

**Benefits**:
- Reproducible builds
- Easier debugging
- Controlled updates

### ✅ 5. Added GitHub Release Automation

**New Job**: `create-release`
- Automatically creates GitHub releases for tags
- Generates release notes from commits
- Attaches all wheels and sdist to release
- Provides download links for users

### ✅ 6. Updated Artifact Downloads

**Changed**: Now includes both wheels and sdist in all relevant jobs
```yaml
pattern: '{wheels-*,sdist}'
```

### ✅ 7. Removed PYPI_TOKEN Dependency

**Trusted Publishing**: Now uses `id-token: write` permission instead of API tokens
- More secure
- No token management required
- GitHub OIDC authentication

## Workflow Job Flow

```
test, lint, security (parallel)
        ↓
build-wheels, build-sdist (parallel)
        ↓
validate (verifies all artifacts)
        ↓
publish (to PyPI with uv)
        ↓
create-release (GitHub release)
```

## Configuration Requirements

### PyPI Trusted Publishing Setup

1. Go to https://pypi.org/manage/account/publishing/
2. Add publisher configuration:
   - **PyPI Project**: `fraiseql`
   - **Owner**: `fraiseql` (GitHub org/user)
   - **Repository**: `fraiseql`
   - **Workflow**: `publish.yml`
   - **Environment**: `release`

### GitHub Environment (Already Configured)

The `release` environment is used for publishing, providing:
- Protection rules for releases
- Audit trail
- Manual approval gates (optional)

## Testing the Workflow

**Dry Run** (test without publishing):
```bash
# Create a test tag locally
git tag v1.1.2-test

# Push to trigger workflow
git push origin v1.1.2-test

# Monitor the workflow
gh run watch

# Delete test tag after verification
git tag -d v1.1.2-test
git push origin :refs/tags/v1.1.2-test
```

**Production Release**:
```bash
# Create release tag
git tag v1.1.2

# Push to trigger workflow
git push origin v1.1.2

# Workflow will:
# 1. Run all tests
# 2. Build wheels for Linux, macOS, Windows
# 3. Build source distribution
# 4. Validate all artifacts
# 5. Publish to PyPI
# 6. Create GitHub release
```

## Comparison with Leading Projects

| Feature | FraiseQL (Before) | FraiseQL (After) | Ruff | pydantic-core |
|---------|-------------------|------------------|------|---------------|
| Build tool | maturin ✓ | maturin ✓ | cargo-dist | maturin |
| Publish tool | pypa/gh-action | **uv publish** ✓ | uv publish | uv publish |
| sdist build | ✗ | **✓** | ✓ | ✓ |
| Validation | ✗ | **✓** | ✓ | ✓ |
| Trusted publishing | ✓ | ✓ | ✓ | ✓ |
| GitHub releases | ✗ | **✓** | ✓ | ✓ |
| Pinned maturin | ✗ | **✓** | N/A | ✓ |

## Benefits

1. **Modern Tooling**: Follows 2025 best practices with `uv publish`
2. **Complete Distribution**: Both wheels and source distribution
3. **Quality Assurance**: Validation before publishing
4. **Transparency**: Clear artifact listing and verification
5. **Automation**: GitHub releases created automatically
6. **Security**: Trusted publishing, no token management
7. **Reliability**: Pinned versions, reproducible builds

## Files Modified

- `.github/workflows/publish.yml` - Complete rewrite
- `.github/workflows/publish.yml.backup` - Backup of original

## Documentation Updated

- `.github/CICD_REVIEW_REQUEST.md` - Already documented issues
- `.github/PUBLISH_WORKFLOW_CHANGES.md` - This file (changes summary)

## Next Steps

1. **Configure PyPI Trusted Publishing** (required before next release)
2. **Test with a pre-release tag** (recommended)
3. **Update CHANGELOG.md** with workflow improvements
4. **Consider adding build caching** for faster CI (future optimization)

## References

- Ruff: https://github.com/astral-sh/ruff/blob/main/.github/workflows/publish-pypi.yml
- pydantic-core: https://github.com/pydantic/pydantic-core/blob/main/.github/workflows/ci.yml
- Polars: https://github.com/pola-rs/polars/blob/main/.github/workflows/release-python.yml
- uv docs: https://docs.astral.sh/uv/
- Maturin: https://www.maturin.rs/
