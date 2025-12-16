# FraiseQL Development Guide

**Last Updated**: December 16, 2025
**Framework**: Modern 2025 Python GraphQL with Rust pipeline

---

## 🎯 Quick Overview

FraiseQL is a high-performance GraphQL framework with:
- **Exclusive Rust pipeline** for JSON transformation (7-10x faster)
- **Type-safe Python API layer** with comprehensive validation
- **PostgreSQL integration** with native JSONB views
- **Enterprise features**: Security, monitoring, caching, authentication

---

## 📦 Release & Version Management

### One-Command Releases

FraiseQL uses an **automated 5-phase release workflow** adapted from PrintOptim:

```bash
# Create feature branch
git checkout -b chore/prepare-vX.Y.Z-release

# One command does everything (automated!)
make pr-ship

# Releases automatically when CI passes
```

### How It Works

**5 Phases** (fully automated):
1. **Phase 0** - Sync with `origin/dev`
2. **Phase 1** - Run full test suite (5991+ tests must pass)
3. **Phase 2** - Bump version (8 files atomically)
4. **Phase 3** - Create commit and git tag
5. **Phase 4** - Push to GitHub
6. **Phase 5** - Create PR with auto-merge enabled

### Version Files Managed

All updates atomic in single commit:
- `src/fraiseql/__init__.py` - Python package version
- `pyproject.toml` - Package metadata
- `Cargo.toml` - Rust workspace
- `fraiseql_rs/Cargo.toml` - Rust extension
- `README.md` - Version references
- `docs/strategic/version-status.md` - Documentation
- `CHANGELOG.md` - Release notes

### Make Commands

**Version Management:**
```bash
make version-show              # Display current version
make version-patch             # Bump patch (1.8.3 → 1.8.4)
make version-minor             # Bump minor (1.8.3 → 1.9.0)
make version-major             # Bump major (1.8.3 → 2.0.0)
make version-dry-run           # Preview all version bumps
```

**PR Shipping:**
```bash
make pr-ship                   # Default patch release
make pr-ship-patch             # Explicit patch
make pr-ship-minor             # Minor version
make pr-ship-major             # Major version (use with caution!)
make pr-status                 # Check current PR status
```

**Release Workflows (test + ship):**
```bash
make release-patch             # Full patch release workflow
make release-minor             # Full minor release workflow
make release-major             # Full major release workflow
```

### Documentation

Complete guide: **`docs/RELEASE_WORKFLOW.md`** (350+ lines)

Covers:
- Quick start
- Workflow overview
- Manual commands
- Safety features
- Troubleshooting
- FAQ
- Performance expectations
- CI/CD integration

---

## 🏗️ Architecture

### Unified Rust Pipeline

All queries execute through the **exclusive Rust pipeline** (`fraiseql_rs/`) for optimal performance:

```
PostgreSQL
    ↓
Rust Pipeline (fraiseql_rs)
    ↓
Python Framework (validation, type-safety)
    ↓
HTTP Response
```

**Key Components:**
- **Rust Pipeline**: Exclusive JSON transformation (7-10x faster than Python)
- **Python Framework**: Type-safe GraphQL API, validation, caching
- **PostgreSQL**: Native JSONB views, advanced functions
- **Enterprise**: Security, monitoring, observability

### Directory Structure

```
fraiseql/
├── src/fraiseql/              # Python framework
│   ├── __init__.py            # Version string
│   ├── db.py                  # Database connectivity
│   ├── where_normalization.py # WHERE clause processing
│   └── gql/                   # GraphQL builders
├── fraiseql_rs/               # Rust pipeline extension
│   ├── src/                   # Rust source
│   └── Cargo.toml             # Rust version
├── scripts/                   # Development utilities
│   ├── version_manager.py     # Atomic version bumping
│   └── pr_ship.py             # Automated release workflow
├── docs/                      # Documentation
│   └── RELEASE_WORKFLOW.md    # Complete release guide
├── tests/                     # Comprehensive test suite (5991+ tests)
└── Makefile                   # Development commands
```

---

## 🧪 Testing

### Test Suite

**Total**: 5991+ tests
- **Unit Tests**: Core functionality
- **Integration Tests**: End-to-end workflows
- **Regression Tests**: Specific issue fixes
- **WHERE Clause Tests**: 20+ tests (Issue #124 fix verification)

### Running Tests

```bash
# Full test suite
make test

# Fast subset
make test-fast

# Specific test file
make test-one TEST=tests/test_where_clause.py

# With verbose output
make test-verbose

# Show test collection
make test-collect
```

### Test Quality

- ✅ 5991+ tests pass (100% success rate)
- ✅ Zero regressions
- ✅ Fast execution (~5 minutes)
- ✅ Comprehensive coverage
- ✅ Issue #124 specific regression tests

---

## 🐛 Bug Fixes & Issue Resolution

### Issue #124: WHERE Clause Filtering on Hybrid Tables

**Status**: ✅ Fixed in v1.8.3

**Problem**: WHERE clause filters silently ignored on hybrid tables (tables with both SQL columns and JSONB data)

**Root Causes**:
1. Type re-registration cleared `table_columns` metadata
2. FK detection used truthiness check (failed on empty sets)

**Solution** (3 files, 44 LOC):
- `src/fraiseql/db.py` - Added metadata fallback
- `src/fraiseql/gql/builders/registry.py` - Preserve metadata during re-registration
- `src/fraiseql/where_normalization.py` - Fixed empty set checks

**Testing**:
- ✅ 4/4 regression tests pass
- ✅ 20+ WHERE clause tests pass
- ✅ 5991+ full test suite passes
- ✅ Zero regressions

---

## 🚀 Pre-commit with prek (Rust)

FraiseQL uses **prek** - a Rust-based replacement for pre-commit that's **7-10x faster** with zero Python dependencies.

### Installation

```bash
# macOS (via Homebrew)
brew install j178/tap/prek

# Linux/macOS (via Rust)
cargo install prek

# Verify installation
prek --version
```

### Setup

```bash
# Install git hooks
prek install

# Run hooks on all files
prek run --all

# Run hooks on staged files (default)
prek run
```

### Why prek?

✅ **7-10x faster** than pre-commit (Rust vs Python)
✅ **Single binary** - no Python dependencies
✅ **Drop-in compatible** - same `.pre-commit-config.yaml` format
✅ **Built-in hooks** in Rust (faster than Python equivalents)
✅ **Monorepo support** - multiple `.pre-commit-config.yaml` files

### Available Hooks

FraiseQL's prek configuration includes:
- **File checks**: trailing whitespace, file endings, YAML/JSON/TOML validation
- **Large files**: prevents committing large binaries
- **Merge conflicts**: detects unresolved merge conflicts
- **Debug statements**: finds leftover print() and debugging code
- **Linting**: ruff check with auto-fix
- **Formatting**: ruff format for consistent code style
- **Kubernetes validation**: yamllint for K8s manifests
- **Custom hooks**: pre-push pytest validation

### Commands

```bash
# Run all prek hooks
prek run --all

# Run specific hook
prek run ruff

# Update hook versions
prek update

# Get hook list
prek list
```

### Integration with Development

```bash
# Format + lint code
make format && make lint

# Or use prek directly
prek run --all

# The Makefile automatically uses prek for all quality checks
```

---

## 📝 Code Standards

### Python

**Version**: 3.10+ with modern type hints

```python
# ✅ CORRECT (3.10+ style)
def get_user(user_id: int) -> User | None:
    ...

def process(items: list[str] | None = None) -> dict[str, int]:
    ...

# ❌ AVOID (pre-3.10 style)
from typing import Optional, List, Dict
def get_user(user_id: int) -> Optional[User]:
    ...
```

### Tools

- **Package Manager**: `uv` (fast, modern)
- **Linter**: `ruff` with strict mode
- **Formatter**: `ruff format`
- **Type Checking**: Built-in `ruff` checker

### Quality Checks

```bash
# Format code
make format

# Lint checks
make lint

# Run all checks
make check

# Full QA pipeline
make qa
```

### Git Conventions

**Commit Messages**:
- `fix: WHERE clause filtering (Issue #124)` - Bug fix
- `chore(release): bump version to v1.8.3` - Release
- `feat: add new feature` - New feature
- `refactor: improve performance` - Code improvement

**Branch Naming**:
- Feature: `feature/description`
- Bugfix: `fix/issue-number`
- Release: `chore/prepare-vX.Y.Z-release`

---

## 🚀 Development Workflow

### For Feature Development

1. **Create feature branch**:
   ```bash
   git checkout -b feature/your-feature
   ```

2. **Make changes** with tests

3. **Run quality checks**:
   ```bash
   make qa    # Tests + linting + formatting
   ```

4. **Commit with descriptive message**:
   ```bash
   git add .
   git commit -m "feat: description of feature"
   ```

5. **Push and create PR**:
   ```bash
   git push -u origin feature/your-feature
   gh pr create --base dev
   ```

### For Bug Fixes

Same as features, but:
- Branch name: `fix/issue-number`
- Commit message: `fix: description (Issue #XXX)`
- Include regression tests

### For Releases

**One command release** (fully automated):
```bash
git checkout -b chore/prepare-vX.Y.Z-release
make pr-ship    # Everything automated!
```

---

## 🔒 Safety Features

### Protected Branches

✅ **Cannot ship from**: `dev`, `main`, `staging`, `production`

Forces use of feature branches.

### Quality Gates

✅ **Full test suite (5991+)** runs before version bump

Release stops if tests fail.

### Atomic Operations

✅ All version files updated together
✅ Single commit contains all changes
✅ Easy rollback: `git revert <commit>`

### Git Tags

✅ Automatic tag creation
✅ Tags pushed for traceability
✅ Enables PyPI releases

### Auto-Merge

✅ Requires CI checks to pass
✅ Prevents premature merge
✅ Squash merge (single clean commit)

---

## 📚 Project-Specific Documentation

### Release Workflow
See: **`docs/RELEASE_WORKFLOW.md`** (350+ lines)

Comprehensive guide covering:
- Quick start
- Complete 5-phase workflow
- Manual commands
- Safety features
- Troubleshooting
- FAQ

### Version Status
See: **`docs/strategic/version-status.md`**

Current stable version and roadmap.

### Getting Started
See: **`docs/getting-started/`**

Installation, quickstart, and tutorials.

---

## 🔧 Common Tasks

### Update Code

```bash
# Edit files, run tests
make test

# Format code
make format

# Lint checks
make lint

# Commit
git add .
git commit -m "fix: description"
```

### Check Versions

```bash
# Show current version
make version-show

# Preview version bump
make version-dry-run
```

### Create Release

```bash
# Create feature branch
git checkout -b chore/prepare-vX.Y.Z-release

# One command (fully automated!)
make pr-ship

# Or specify version type:
make pr-ship-minor    # 1.8.3 → 1.9.0
make pr-ship-major    # 1.8.3 → 2.0.0
```

### Check PR Status

```bash
# Current branch's PR
make pr-status

# Detailed PR info
gh pr view
```

---

## 📊 Performance

### Test Suite
- **Total Tests**: 5991+
- **Execution Time**: ~5 minutes
- **Success Rate**: 100%
- **Regressions**: 0

### Release Workflow
- **Total Time**: ~5 minutes 13 seconds
  - Phase 0 (Sync): ~5 sec
  - Phase 1 (Tests): ~5 min
  - Phases 2-5: ~8 sec

### Development Speed
- **Build**: < 2 seconds
- **Format**: < 10 seconds
- **Lint**: < 10 seconds
- **Type Check**: < 5 seconds

---

## 🎯 Key Files

| File | Purpose | Last Updated |
|------|---------|--------------|
| `scripts/version_manager.py` | Atomic version bumping | 2025-12-16 |
| `scripts/pr_ship.py` | 5-phase release workflow | 2025-12-16 |
| `Makefile` | Development commands | 2025-12-16 |
| `docs/RELEASE_WORKFLOW.md` | Complete release guide | 2025-12-16 |
| `src/fraiseql/__init__.py` | Package version | 2025-12-16 |
| `pyproject.toml` | Python metadata | 2025-12-16 |
| `Cargo.toml` | Rust workspace | 2025-12-16 |

---

## 🚨 Troubleshooting

### Tests Failing

```bash
# Run tests with verbose output
make test-verbose

# Run specific test file
make test-one TEST=tests/test_file.py

# Run test suite and see failures
make test-fast
```

### Cannot Ship from Branch

```
❌ Cannot ship from protected branch: dev
💡 Create a feature branch first:
   git checkout -b chore/prepare-vX.Y.Z-release
```

### Merge Conflicts During Release

```bash
# Resolve conflicts manually
git merge origin/dev

# Then retry shipping
make pr-ship
```

### Version Bump Preview

```bash
# See what would change (no changes made)
make version-dry-run
```

---

## 📞 Getting Help

### Documentation
- **Release Guide**: `docs/RELEASE_WORKFLOW.md`
- **Version Status**: `docs/strategic/version-status.md`
- **Installation**: `docs/getting-started/installation.md`

### Commands
```bash
# Show all available commands
make help

# Show command groups
make help | grep -A 5 "VERSION MANAGEMENT"
```

### GitHub Issues
Create issues for bugs or feature requests on GitHub.

---

## 🔄 Workflow Summary

```
Feature Branch Created
         ↓
make test       (Verify changes)
         ↓
make format     (Format code)
         ↓
make lint       (Check code)
         ↓
git add . && git commit -m "feat: ..."
         ↓
git push -u origin feature/...
         ↓
gh pr create --base dev
         ↓
Code Review & CI Checks
         ↓
Merge to dev
```

**For Releases:**
```
git checkout -b chore/prepare-vX.Y.Z-release
         ↓
make pr-ship    (Fully Automated!)
         ↓
Auto-merge when CI passes
         ↓
Version released!
```

---

## 📋 Checklist for New Contributors

- [ ] Read `docs/RELEASE_WORKFLOW.md`
- [ ] Understand the 5-phase release workflow
- [ ] Know how to run `make test`
- [ ] Know how to run `make pr-ship`
- [ ] Understand the 8 version files
- [ ] Review this CLAUDE.md file
- [ ] Set up pre-commit hooks: `pre-commit install`

---

## 🎉 Summary

FraiseQL uses a **modern, production-ready development workflow** with:

✅ **Automated releases** (one command: `make pr-ship`)
✅ **Comprehensive testing** (5991+ tests)
✅ **Atomic version management** (8 files)
✅ **GitHub native integration** (auto-merge)
✅ **Safety features** (protected branches, quality gates)
✅ **Excellent documentation** (350+ lines)

**Ready to contribute!** Just run `make help` to see all commands.

---

*Last Updated: December 16, 2025*
*Framework: FraiseQL v1.8.3*
*Release System: Modern 2025 GitHub-native auto-merge*
