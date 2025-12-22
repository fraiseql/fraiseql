# FraiseQL Release Workflow & PR Shipping

**Modern 2025 GitHub-native automated release strategy for FraiseQL**

This document describes FraiseQL's automated PR shipping and release workflow, adapted from PrintOptim's production-proven system. It handles version bumping across 8 files (Python, Rust, and documentation) with a completely automated GitHub native workflow.

---

## Quick Start

### Ship a patch release (1.8.3 → 1.8.4)

```bash
# Create feature branch
git checkout -b chore/prepare-v1.8.4-release

# Ship to dev with patch version bump
make pr-ship

# Or explicitly:
make pr-ship-patch
```

### Ship a minor release (1.8.3 → 1.9.0)

```bash
git checkout -b chore/prepare-v1.9.0-release
make pr-ship-minor
```

### Ship a major release (1.8.3 → 2.0.0)

```bash
git checkout -b chore/prepare-v2.0.0-release
make pr-ship-major
```

---

## Workflow Overview

The `pr-ship` workflow runs in 5 phases and is fully automated:

```
📋 Feature branch created
        ↓
🔄 PHASE 0: Sync with base branch (dev)
        ↓
🔍 PHASE 1: Run full test suite (5991+ tests)
        ↓
📦 PHASE 2: Bump version (8 files updated)
        ↓
💾 PHASE 3: Commit and create git tag
        ↓
📤 PHASE 4: Push to GitHub
        ↓
🚀 PHASE 5: Create PR + enable auto-merge
        ↓
✅ Auto-merge when all checks pass
```

### What Gets Updated

**Version Files (8 total):**

1. `src/fraiseql/__init__.py` - Python package version string
2. `pyproject.toml` - Package metadata
3. `Cargo.toml` - Rust workspace version
4. `fraiseql_rs/Cargo.toml` - Rust extension version
5. `README.md` - Version badges/references
6. `docs/strategic/version-status.md` - Current stable version
7. `CHANGELOG.md` - Release notes header
8. All documentation references updated

---

## Manual Commands

If you prefer to run steps individually:

### 1. Show Current Version

```bash
make version-show
```

Output:
```
📊 FraiseQL Version Information
============================================================
Current Version: v1.8.3
Version Parts: Major=1, Minor=8, Patch=3

Next versions:
  • Patch: v1.8.4
  • Minor: v1.9.0
  • Major: v2.0.0
```

### 2. Preview Changes (Dry-Run)

```bash
make version-dry-run
```

Preview patch bump:
```bash
uv run python scripts/version_manager.py patch --dry-run
```

### 3. Bump Version (Manual)

```bash
# Bump patch version
make version-patch

# Bump minor version
make version-minor

# Bump major version
make version-major
```

### 4. Run Tests

```bash
# Full test suite
make test

# Fast test subset
make test-fast

# Specific test file
make test-one TEST=tests/test_where_clause.py
```

### 5. Manual PR Creation

After committing version changes:

```bash
# Create branch
git checkout -b chore/prepare-vX.Y.Z-release

# Commit version changes
git add .
git commit -m "chore(release): bump version to vX.Y.Z"

# Create git tag
git tag -a vX.Y.Z -m "Release vX.Y.Z"

# Push branch and tag
git push -u origin chore/prepare-vX.Y.Z-release
git push origin vX.Y.Z

# Create PR
gh pr create --base dev --title "Release v1.8.X" --body "Automated release PR"

# Enable auto-merge
gh pr merge --auto --squash
```

---

## Available Make Commands

### Version Management

| Command | Purpose |
|---------|---------|
| `make version-show` | Display current version |
| `make version-patch` | Bump patch (1.8.3 → 1.8.4) |
| `make version-minor` | Bump minor (1.8.3 → 1.9.0) |
| `make version-major` | Bump major (1.8.3 → 2.0.0) |
| `make version-dry-run` | Preview all version bumps |

### Pull Request Shipping

| Command | Purpose |
|---------|---------|
| `make pr-ship` | Default patch release workflow |
| `make pr-ship-patch` | Explicit patch release |
| `make pr-ship-minor` | Minor version release |
| `make pr-ship-major` | Major version release |
| `make pr-status` | Check PR status |

### Release Workflows

| Command | Purpose |
|---------|---------|
| `make release-patch` | Full test + patch ship |
| `make release-minor` | Full test + minor ship |
| `make release-major` | Full test + major ship |

---

## How It Works: Technical Details

### Version Manager (`scripts/version_manager.py`)

Handles atomic version bumping across 8 files:

```python
# Show version
python scripts/version_manager.py show

# Bump patch/minor/major
python scripts/version_manager.py patch
python scripts/version_manager.py minor
python scripts/version_manager.py major

# Preview without changes
python scripts/version_manager.py patch --dry-run
```

**Features:**
- Parses semantic versioning (major.minor.patch)
- Updates 8 files consistently
- Validates patterns exist before updating
- Dry-run mode for preview
- Atomic operations (all-or-nothing)

### PR Ship (`scripts/pr_ship.py`)

Orchestrates the complete release workflow:

**Phase 0: Sync**
- Fetches latest `origin/dev`
- Merges base branch into current branch
- Ensures no out-of-date PRs

**Phase 1: Quality Checks**
- Runs full test suite (5991+ tests)
- Fails if tests don't pass
- Reports test summary

**Phase 2: Version Bump**
- Calls version_manager for atomic update
- Updates all 8 version files

**Phase 3: Git Operations**
- Creates atomic commit with version changes
- Creates git tag for release
- Pushes branch and tag to GitHub

**Phase 4: PR Creation**
- Creates PR targeting `dev` branch
- Auto-fills title from branch name
- Enables auto-merge (squash merge)

**Features:**
- Protected branch checks (prevents shipping from dev/main)
- Uncommitted changes detection
- Merge conflict handling
- Auto-merge with squash (single commit)
- Git tag creation and push

---

## Branch Naming Convention

Use descriptive branch names for releases:

```bash
# Patch releases
git checkout -b chore/prepare-v1.8.4-release

# Minor releases
git checkout -b chore/prepare-v1.9.0-release

# Major releases
git checkout -b chore/prepare-v2.0.0-release
```

These are automatically converted to PR titles like:
- "Prepare V1.8.4 Release"
- "Prepare V1.9.0 Release"
- "Prepare V2.0.0 Release"

---

## Example: Complete Release Workflow

### Patch Release (1.8.3 → 1.8.4)

```bash
# 1. Create feature branch
git checkout -b chore/prepare-v1.8.4-release

# 2. Ship (fully automated - 5 phases)
make pr-ship

# Output shows:
# 🚢 FRAISEQL PR SHIP WORKFLOW
# 🔄 PHASE 0: Syncing with Base Branch
# ✅ Fetched origin/dev
# 🔍 PHASE 1: Pre-flight Quality Checks
# ✅ All 5991+ tests passed
# 📦 PHASE 2: Preparing Changes
# 📈 Bumping patch version...
# ✅ Version bumped to 1.8.4
# 💾 PHASE 3: Committing Changes
# 📝 Committing: chore(release): bump version to v1.8.4
# ✅ Changes committed
# 🔧 PHASE 4: Git Operations
# 🏷️  Creating tag: v1.8.4
# ✅ Tag v1.8.4 created
# 📤 Pushing to GitHub...
# ✅ Pushed to GitHub
# 🚀 PHASE 5: Creating Pull Request
# 🚀 Creating PR with auto-merge...
# ✅ PR created: https://github.com/fraiseql/fraiseql/pull/XXX
# 🤖 Enabling auto-merge...
# ✅ Auto-merge enabled
# ✨ PR SHIP COMPLETED SUCCESSFULLY!

# 3. Verify (optional)
make pr-status

# 4. PR auto-merges when CI checks pass (automatic!)
```

### Minor Release (1.8.3 → 1.9.0)

```bash
# Identical to patch, just use minor:
git checkout -b chore/prepare-v1.9.0-release
make pr-ship-minor
```

### Major Release (1.8.3 → 2.0.0)

```bash
# Identical, use major:
git checkout -b chore/prepare-v2.0.0-release
make pr-ship-major
```

---

## What Happens After Merge

Once the PR auto-merges (automatic when CI passes):

1. ✅ Version commit merged to `dev`
2. ✅ Git tag `vX.Y.Z` pushed to GitHub
3. ✅ All version files updated
4. ✅ CHANGELOG.md updated

### Manual Post-Merge Steps (if needed)

Build and publish to PyPI:

```bash
# Build distribution
uv build

# Publish to PyPI
uvx twine upload dist/*
```

---

## Troubleshooting

### PR Ship Fails with "Cannot ship from protected branch"

```
❌ Cannot ship from protected branch: dev
💡 Create a feature branch first:
   git checkout -b feature/your-feature
```

**Solution:** Use a feature branch:
```bash
git checkout -b chore/prepare-v1.8.4-release
make pr-ship
```

### Tests Failed Before Version Bump

The workflow stops at Phase 1 if tests fail:

```
❌ Tests failed!
[test output...]
💡 Your changes are safe. Fix the issue and try again.
```

**Solution:** Fix test failures:
```bash
# Run tests locally to debug
make test-verbose

# Fix the issue
# Then run pr-ship again
make pr-ship
```

### Merge Conflicts During Sync

If base branch has diverged:

```
❌ Failed to sync with dev
💡 You may have merge conflicts. Please resolve them and try again.
```

**Solution:** Resolve manually:
```bash
# Resolve conflicts
git merge origin/dev

# Continue shipping (tests will run again)
make pr-ship
```

### Cannot Enable Auto-Merge

```
⚠️  Auto-merge will activate when checks pass
```

This is normal if CI checks haven't completed yet. Auto-merge will activate automatically once checks pass.

### Already Have Uncommitted Changes

```
⚠️  You have uncommitted changes
💡 These will be included in the version bump commit
```

The workflow will include these in the release commit. If you don't want them, commit or stash them first.

---

## Safety Features

### Protected Branch Check
- Cannot ship from `dev`, `main`, `staging`, `master`, or `production`
- Forces feature branch usage

### Quality Gates
- Full test suite runs before version bump
- All 5991+ tests must pass
- Pre-merge CI checks required

### Atomic Operations
- All 8 version files updated together
- Single commit contains all changes
- Rollback is as simple as reverting the commit

### Git Tags
- Automatic git tag creation
- Tags pushed with PR for traceability
- Easy rollback via tag revert

### Auto-Merge
- Requires CI checks to pass
- Prevents premature merge
- Squash merge (single commit to dev)

---

## Integration with CI/CD

The auto-merge feature integrates with GitHub Actions:

1. PR is created with auto-merge enabled
2. CI checks run automatically
3. When all checks pass, PR auto-merges
4. Deployment can be triggered on merge

### GitHub Actions Workflow

Add this to `.github/workflows/release.yml`:

```yaml
name: Auto-Deploy on Release

on:
  push:
    tags:
      - 'v*'

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Deploy to PyPI
        run: |
          uv build
          uvx twine upload dist/*
```

---

## Performance

### Expected Times

| Phase | Expected Time |
|-------|---|
| Phase 0 (Sync) | ~5 seconds |
| Phase 1 (Tests) | ~5 minutes |
| Phase 2 (Version bump) | ~2 seconds |
| Phase 3 (Commit + Tag) | ~1 second |
| Phase 4 (Push) | ~3 seconds |
| Phase 5 (PR Creation) | ~2 seconds |
| **Total** | **~5 minutes 15 seconds** |

The test suite is the main time consumer (5991+ tests). Everything else is very fast.

---

## FAQ

**Q: Can I ship without running tests?**
A: No, tests are mandatory. This prevents breaking releases.

**Q: Can I skip version bump?**
A: The current implementation requires a version bump. For bugfix-only releases, use patch.

**Q: How do I rollback after merge?**
A: `git revert` the release commit, then create a new patch release.

**Q: Can I ship multiple releases at once?**
A: No, ship them sequentially. Each creates a separate PR.

**Q: What if CI checks fail after merge?**
A: Auto-merge won't trigger until checks pass. Fix CI and update the PR.

**Q: Can I manually merge instead of auto-merge?**
A: Yes, the PR exists for 10+ seconds before auto-merge triggers.

---

## See Also

- [FraiseQL Version Status](../docs/strategic/version-status.md)
- [Release Automation Script](../scripts/pr_ship.py)
- [GitHub Actions Auto-Merge Documentation](https://docs.github.com/en/pull-requests/automating-your-workflow-with-github-apis/managing-pull-requests-with-the-rest-api)
