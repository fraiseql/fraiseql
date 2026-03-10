# FraiseQL Release Guide

Complete guide for managing releases of FraiseQL across all platforms and registries.

## Table of Contents

1. [Overview](#overview)
2. [Release Process](#release-process)
3. [GitHub Actions Workflow](#github-actions-workflow)
4. [Manual Release (Emergency Only)](#manual-release-emergency-only)
5. [Troubleshooting](#troubleshooting)
6. [Rollback Procedures](#rollback-procedures)

---

## Overview

FraiseQL follows **semantic versioning** and publishes to multiple registries:

- **crates.io** - Rust crates (9 packages)
- **PyPI** - Python package (1 package)
- **GitHub Releases** - Binary artifacts and source

### Release Channels

- **Stable**: `v2.0.0`, `v2.1.0` (production releases)
- **Pre-release**: `v2.0.0-alpha.5`, `v2.0.0-beta.1` (testing releases)

### Current Registries

| Registry | Package | Manager | Required Secret |
|----------|---------|---------|-----------------|
| crates.io | fraiseql* (9 crates) | cargo | CARGO_TOKEN |
| PyPI | fraiseql | pip | PYPI_TOKEN |
| GitHub | Binaries (5 platforms) | Manual | GITHUB_TOKEN |

---

## Release Cadence

**Minor versions** (2.x.0): Minimum 6 weeks between releases. Each minor version should be
available on crates.io for at least 6 weeks before the next minor is cut. ("Deployed to
production" cannot be enforced for a library; crates.io availability is the observable
equivalent.)

**Patch versions** (2.x.y): As needed for security fixes or critical bugs. No minimum gap.

**Major versions** (x.0.0): Require an explicit deprecation period for breaking changes.
Announce breaking changes at least one minor version in advance.

**Exception**: If two minor versions are developed concurrently (e.g. during initial
launch), document the exception clearly in the CHANGELOG with a rationale. See the
v2.0.0 / v2.1.0 note in `roadmap.md` for an example.

---

## Release Process

### Step 1: Prepare Release

```bash
# 1. Update version in Cargo.toml (workspace)
# Current: 2.0.0-alpha.5
# Next: 2.0.0-alpha.6

# 2. Update CHANGELOG.md
# Add entry with all changes

# 3. Update README.md if needed
# Ensure version numbers match

# 4. Commit changes
git add -A
git commit -m "chore(release): Prepare v2.0.0-alpha.6"
git push origin dev
```

### Step 2: Create Release Tag

```bash
# Create annotated tag with release notes
git tag -a v2.0.0-alpha.6 -m "Release v2.0.0-alpha.6

## Changes

- Feature A
- Feature B
- Bug fix C

## Verification

✅ All tests pass
✅ All clippy checks pass
✅ Release notes updated
"

# Push to trigger workflow
git push origin v2.0.0-alpha.6
```

### Step 3: Monitor Workflow

```bash
# Watch workflow progress
gh run watch --workflow=release.yml

# Or check status
gh run list --workflow=release.yml --limit 1
```

### Step 4: Verify Release

```bash
# Check crates.io
curl https://crates.io/api/v1/crates/fraiseql | jq '.crate.newest_version'

# Check PyPI
pip index versions fraiseql

# Check GitHub
gh release view v2.0.0-alpha.6
```

---

## GitHub Actions Workflow

### Architecture

```
┌─────────────────────────────────────────┐
│ 1. VALIDATE-RELEASE (Fail-fast)         │
│ ├─ Check secrets exist                  │
│ ├─ Test crates.io token (dry-run)       │
│ └─ Validate Python package build        │
│ Duration: 30-60 seconds                 │
└──────────────────┬──────────────────────┘
                   ↓
┌─────────────────────────────────────────┐
│ 2. CREATE-GITHUB-RELEASE (Sequential)   │
│ ├─ Create GitHub release                │
│ └─ Post release notes                   │
│ Duration: 10 seconds                    │
└──────────────────┬──────────────────────┘
                   ↓
┌─────────────────────────────────────────┐
│ 3. PUBLISH-REGISTRIES (Parallel)        │
│ ├─ publish-crates (crates.io)           │
│ │  └─ Publishes 9 crates in order       │
│ │  Duration: 2-3 minutes                │
│ └─ publish-python (PyPI)                │
│    ├─ Build distributions               │
│    ├─ Upload to PyPI                    │
│    └─ Verify installation               │
│    Duration: 1-2 minutes                │
│ Total: 2-3 minutes (parallel)           │
└──────────────────┬──────────────────────┘
                   ↓
┌─────────────────────────────────────────┐
│ 4. BUILD-BINARIES (Parallel, 5 jobs)    │
│ ├─ Linux x86_64                         │
│ ├─ Linux ARM64                          │
│ ├─ macOS x86_64                         │
│ ├─ macOS ARM64                          │
│ └─ Windows x86_64                       │
│ Duration: 15-30 minutes (parallel)      │
└──────────────────┬──────────────────────┘
                   ↓
┌─────────────────────────────────────────┐
│ 5. VERIFY-RELEASE (Parallel)            │
│ ├─ Test Rust crate imports              │
│ ├─ Test Python package imports          │
│ └─ Check binary integrity               │
│ Duration: 2-5 minutes                   │
└──────────────────┬──────────────────────┘
                   ↓
┌─────────────────────────────────────────┐
│ 6. NOTIFY (Always)                      │
│ ├─ Success notification                 │
│ └─ Link to release                      │
│ Duration: 10 seconds                    │
└─────────────────────────────────────────┘

Total time: ~20-35 minutes (with parallelization)
```

### Workflow Jobs

#### 1. validate-release

**Purpose:** Fail-fast on configuration issues

**Checks:**

- ✅ CARGO_TOKEN secret exists
- ✅ PYPI_TOKEN secret exists
- ✅ crates.io token valid (dry-run test)
- ✅ Python package builds successfully

**If fails:** Stops workflow, saves 15+ minutes

#### 2. create-github-release

**Purpose:** Create GitHub release with proper metadata

**Actions:**

- Uses `gh release create` (official GitHub CLI)
- Sets prerelease flag for alpha/beta versions
- Posts CHANGELOG excerpt as release notes
- Uploads source code archives

**If fails:** Non-blocking (packages already published)

#### 3. publish-crates

**Purpose:** Publish all Rust crates to crates.io

**Order of publication:**

1. fraiseql-error (no dependencies)
2. fraiseql-wire (minimal deps)
3. fraiseql-core (depends on error)
4. fraiseql-arrow (depends on core, wire)
5. fraiseql-observers-macros (depends on core)
6. fraiseql-observers (depends on core, macros)
7. fraiseql-server (depends on core, error, observers, arrow)
8. fraiseql-cli (depends on core)
9. fraiseql (root crate, depends on all)

**Waits between publishes:** 30 seconds (for crates.io indexing)

#### 4. publish-python

**Purpose:** Publish Python package to PyPI

**Actions:**

- Build sdist and wheel
- Upload to PyPI
- Wait for indexing
- Verify package is importable
- Test version string

#### 5. build-binaries

**Purpose:** Build CLI binaries for distribution

**Platforms:**

- Linux x86_64 (AMD64)
- Linux ARM64 (aarch64)
- macOS x86_64 (Intel)
- macOS ARM64 (Apple Silicon)
- Windows x86_64 (MSVC)

**Actions:**

- Cross-compile using Rust targets
- Strip symbols (Unix)
- Upload to GitHub release using `softprops/action-gh-release`

**Phase 2 Enhancement:**

- Replaced manual `gh release upload` with `softprops/action-gh-release@v2`
- Automatic checksums for all binaries
- Better error handling and retry logic
- Idempotent uploads (can safely retry)
- Cleaner, more maintainable YAML

#### 6. verify-release

**Purpose:** Post-publish verification (NEW in Phase 2)

**Checks:**

- Verify fraiseql crate on crates.io
- Verify fraiseql package on PyPI
- Count binary assets on GitHub release
- List uploaded asset names

**Duration:** ~30 seconds

**Benefits:**

- Early detection of publishing failures
- Clear status in workflow summary
- Guides troubleshooting if issues found

#### 7. notify

**Purpose:** Send release notifications

**Notifications:**

- (Future) Slack message
- (Future) GitHub discussion post
- (Future) Email digest

---

## Manual Release (Emergency Only)

Use only if GitHub Actions is unavailable.

### Prerequisites

```bash
# Install tools
cargo install cargo-release
pip install twine build

# Set environment
export CARGO_TOKEN="your_token_here"
export PYPI_TOKEN="your_token_here"
export VERSION="2.0.0-alpha.6"
```

### Manual Steps

```bash
# 1. Validate environment
[ -z "$CARGO_TOKEN" ] && echo "ERROR: CARGO_TOKEN not set" && exit 1
[ -z "$PYPI_TOKEN" ] && echo "ERROR: PYPI_TOKEN not set" && exit 1

# 2. Create tag
git tag -a v$VERSION -m "Release v$VERSION"
git push origin v$VERSION

# 3. Publish Rust crates
for crate in fraiseql-error fraiseql-wire fraiseql-core fraiseql-arrow fraiseql-observers-macros fraiseql-observers fraiseql-server fraiseql-cli fraiseql; do
  echo "Publishing $crate..."
  cargo publish -p $crate --token $CARGO_TOKEN
  sleep 30  # Wait for indexing
done

# 4. Publish Python package
cd fraiseql-python
python -m build
twine upload dist/* -u __token__ -p $PYPI_TOKEN

# 5. Create GitHub release
cd ..
gh release create v$VERSION \
  --title "Release v$VERSION" \
  --notes "See CHANGELOG.md for details" \
  --prerelease

# 6. Build and upload binaries
cargo build --release --target x86_64-unknown-linux-gnu
# ... build for other targets ...
gh release upload v$VERSION target/release/fraiseql-cli*
```

---

## Troubleshooting

### Issue: "CARGO_TOKEN secret is missing"

**Solution:**

1. Go to: https://github.com/fraiseql/fraiseql/settings/secrets/actions
2. Click "New repository secret"
3. Name: `CARGO_TOKEN`
4. Value: Get from https://crates.io/me → API Tokens

```bash
# Or via CLI
gh secret set CARGO_TOKEN --repo fraiseql/fraiseql -b "YOUR_TOKEN"
```

### Issue: "PYPI_TOKEN secret is missing"

**Solution:**

1. Go to: https://pypi.org/manage/account/
2. Scroll to "API tokens"
3. Create token with "Entire account" scope
4. Copy token value

```bash
# Or via CLI
gh secret set PYPI_TOKEN --repo fraiseql/fraiseql -b "YOUR_TOKEN"
```

### Issue: "Resource not accessible by integration"

**Cause:** GitHub Actions doesn't have permission to create releases

**Solution:**

- Workflow now uses `gh release create` (fixes permission issues)
- If error persists, check repository permissions:
  1. Settings → Environments
  2. Check GITHUB_TOKEN permissions
  3. Ensure "contents: write" is set

### Issue: "Binary upload failed"

**Solution (softprops/action-gh-release):**

- The action now provides better error messages in the workflow log
- Check that binaries exist: `ls target/release/fraiseql-cli*`
- Check that the release was already created: `gh release view v2.0.0-alpha.6`
- Retry the failed build-binaries job directly from GitHub Actions

**Manual Upload (if needed):**

```bash
gh release upload v2.0.0-alpha.6 \
  target/x86_64-unknown-linux-gnu/release/fraiseql-cli \
  target/aarch64-unknown-linux-gnu/release/fraiseql-cli \
  target/x86_64-pc-windows-msvc/release/fraiseql-cli.exe \
  target/x86_64-apple-darwin/release/fraiseql-cli \
  target/aarch64-apple-darwin/release/fraiseql-cli
```

### Issue: "crates.io token expired"

**Solution:**

1. Get new token from https://crates.io/me
2. Update secret: `gh secret set CARGO_TOKEN --repo fraiseql/fraiseql -b "NEW_TOKEN"`
3. Retag and push: `git tag -a v2.0.0-alpha.6-retry ...`

### Issue: "PyPI upload failed but GitHub release succeeded"

**Solution (Non-blocking):**

- GitHub release is already created
- Manually fix and upload Python package:

  ```bash
  cd fraiseql-python
  python -m build
  twine upload dist/*
  ```

- Update release notes to note PyPI delay

### Issue: "Verification job shows missing packages"

**What's happening:**

- PyPI and crates.io have indexing delays (5-15 minutes)
- The verify-release job reports current status, not final status
- This is informational, not an error

**Solution:**

- Wait 10-15 minutes and check manually:

  ```bash
  # Check crates.io
  curl https://crates.io/api/v1/crates/fraiseql | jq '.crate.newest_version'

  # Check PyPI
  pip index versions fraiseql
  ```

- Re-run verification job if needed: `gh run rerun <run-id> --job verify-release`

---

## Rollback Procedures

### Scenario 1: Pre-Publishing Failure

**If validation fails:**

- Workflow stops automatically
- No packages published
- No GitHub release created
- Fix issue and retag

```bash
git tag -d v2.0.0-alpha.6
git push origin :refs/tags/v2.0.0-alpha.6
# Fix issue
git tag -a v2.0.0-alpha.6-retry ...
git push origin v2.0.0-alpha.6-retry
```

### Scenario 2: Partial Failure (Some Crates Published)

**If only some crates published to crates.io:**

- Document which crates succeeded
- Manually publish remaining crates
- Coordination with downstream users (if any)

```bash
# Check what published
curl https://crates.io/api/v1/crates/fraiseql-core | jq '.crate.newest_version'

# Manually publish missing
cargo publish -p fraiseql-server --token $CARGO_TOKEN
```

### Scenario 3: Post-Publishing Issue

**If issue discovered after full publish:**

1. **Document issue:** Create GitHub issue
2. **Yanked crate (crates.io):**

   ```bash
   cargo yank --vers 2.0.0-alpha.6 -p fraiseql
   ```

3. **Deprecate on PyPI:** Manually via https://pypi.org/manage/project/fraiseql/
4. **Create patch release:** v2.0.0-alpha.7 with fix

### Scenario 4: GitHub Release Issues

**If release created but binaries wrong:**

- Edit release on GitHub
- Re-upload corrected binaries
- Update release notes if needed

```bash
# Delete and recreate release (if needed)
gh release delete v2.0.0-alpha.6 -y
# ... fix binaries ...
gh release create v2.0.0-alpha.6 --title ... target/release/fraiseql-cli-*
```

---

## Best Practices

### Before Releasing

- ✅ Ensure `dev` branch is stable
- ✅ Run full test suite locally: `cargo test --all --all-features`
- ✅ Run linter: `cargo clippy --all-targets --all-features -- -D warnings`
- ✅ Update CHANGELOG.md with all changes
- ✅ Update version numbers consistently
- ✅ Create meaningful tag annotations

### During Release

- ✅ Monitor workflow in real-time
- ✅ Don't push new commits while workflow is running
- ✅ Keep GitHub Actions page open
- ✅ Have backup registries (PyPI, crates.io status)

### After Release

- ✅ Verify all packages published
- ✅ Test installation: `pip install fraiseql==2.0.0-alpha.6`
- ✅ Test cargo dependency: Add to test project
- ✅ Update documentation with new version
- ✅ Post release announcement (if major)

### Emergency Contacts

- **crates.io issues:** security@rust-lang.org
- **PyPI issues:** pypi-help@python.org
- **GitHub Actions issues:** GitHub Support

---

## Version Naming Conventions

### Stable Releases

- Format: `v2.0.0`, `v2.1.0`, `v2.1.1`
- Workflow: Full validation + binaries
- Support: Long-term

### Pre-releases

- Alpha: `v2.0.0-alpha.1` → `v2.0.0-alpha.5`
- Beta: `v2.1.0-beta.1` → `v2.1.0-beta.3`
- RC: `v2.1.0-rc.1` → `v2.1.0-rc.2`
- Workflow: Same as stable
- Support: Until next stable

### Release Candidates

- For final testing before stable
- Only bug fixes after RC
- No new features

---

## Future Enhancements

### Phase 2 (Complete ✅)

- [x] Binary upload with softprops/action-gh-release
- [x] Post-publish verification job
- [x] Workflow summaries with clear status
- [x] Better error tracking and reporting

### Phase 3 (Planned)

- [ ] Slack notifications on release status
- [ ] GitHub Discussions announcements
- [ ] Automated rollback capability
- [ ] Release notes auto-generation

### Phase 4 (Later)

- [ ] Docker image publishing
- [ ] Homebrew formula publishing
- [ ] Windows installer (.msi)
- [ ] Debian/RPM packages
- [ ] Release metrics dashboard

---

## Related Documents

- [CHANGELOG.md](CHANGELOG.md) - All version history
- [README.md](README.md) - Installation instructions
- [Contributing Guide](CONTRIBUTING.md) - Development setup
- [Security Policy](SECURITY.md) - Security reporting
