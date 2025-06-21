# FraiseQL Release Process

This document outlines the release process for FraiseQL following the Simple GitHub Flow.

## Release Types

### 1. Regular Releases (vX.Y.Z)
For stable releases following semantic versioning:
- **Major (vX.0.0)**: Breaking changes
- **Minor (v0.Y.0)**: New features, backward compatible
- **Patch (v0.0.Z)**: Bug fixes, backward compatible

### 2. Pre-releases
- **Alpha**: `vX.Y.Z-alpha.N` (early testing)
- **Beta**: `vX.Y.Z-beta.N` (feature complete, testing)
- **Release Candidate**: `vX.Y.Z-rc.N` (final testing)

## Release Checklist

### 1. Pre-release Preparation

- [ ] All tests passing on `main`
- [ ] Update `CHANGELOG.md` with all changes since last release
- [ ] Update version in `pyproject.toml`
- [ ] Update documentation if needed
- [ ] Run `make test` locally
- [ ] Run `make typecheck` locally
- [ ] Run `make build` to test package building

### 2. Create Release PR

```bash
# Create release branch
git checkout -b release/v0.1.0

# Update version
# Edit pyproject.toml: version = "0.1.0"

# Update CHANGELOG
# Add release date to CHANGELOG.md

# Commit changes
git add pyproject.toml CHANGELOG.md
git commit -m "chore: prepare release v0.1.0"

# Push and create PR
git push origin release/v0.1.0
```

### 3. Release PR Review

The release PR should include:
- Version bump in `pyproject.toml`
- Updated `CHANGELOG.md` with release date
- Any last-minute documentation updates

### 4. Tag and Release

After PR is merged to `main`:

```bash
# Pull latest main
git checkout main
git pull origin main

# Create annotated tag
git tag -a v0.1.0 -m "Release version 0.1.0

<summary of major changes from CHANGELOG>"

# Push tag
git push origin v0.1.0
```

### 5. GitHub Release

1. Go to GitHub Releases page
2. Click "Create release from tag"
3. Select the tag you just created
4. Title: "v0.1.0"
5. Copy the CHANGELOG section for this release
6. Check "Set as the latest release" (or pre-release if applicable)
7. Publish release

### 6. PyPI Publication (Automated)

The GitHub Actions workflow will automatically:
1. Build the package
2. Run final tests
3. Publish to PyPI

Monitor the Actions tab for the release workflow.

### 7. Post-release

- [ ] Verify package on PyPI: `pip install fraiseql==0.1.0`
- [ ] Test installation in clean environment
- [ ] Update any dependent projects
- [ ] Announce release (if applicable)

## Hotfix Process

For critical fixes to released versions:

```bash
# Create hotfix branch from tag
git checkout -b hotfix/v0.1.1 v0.1.0

# Make fixes
# Update version to 0.1.1
# Update CHANGELOG

# Create PR to main
# After merge, tag as v0.1.1
```

## Pre-release Process

For alpha/beta/rc releases:

```bash
# Version in pyproject.toml
version = "0.2.0-beta.1"

# Tag
git tag -a v0.2.0-beta.1 -m "Pre-release v0.2.0-beta.1"

# On GitHub, mark as pre-release
```

## Version Management

### Version Locations
1. `pyproject.toml` - `version` field
2. Git tags - `vX.Y.Z`
3. GitHub Releases

### Automation Support

The release workflow (`.github/workflows/publish.yml`) is triggered by:
- Tags matching `v*` pattern
- Publishes to PyPI automatically
- Creates GitHub Release draft

## Emergency Procedures

### Yanking a Release

If a critical issue is found:

1. Yank from PyPI (within 24 hours if possible):
   ```bash
   python -m twine yank fraiseql==0.1.0
   ```

2. Delete the GitHub Release (keep tag for history)

3. Fix the issue and release a patch version

### Failed Release

If the automated release fails:

1. Check GitHub Actions logs
2. Fix any issues
3. Delete the tag locally and remotely:
   ```bash
   git tag -d v0.1.0
   git push origin :v0.1.0
   ```
4. Start the release process again
