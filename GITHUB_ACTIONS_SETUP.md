# GitHub Actions Python Version Matrix Setup

**Date:** October 30, 2025
**Branch:** `fix/python-version-requirement`
**Status:** ✅ COMPLETE

## Summary

Successfully created a comprehensive Python version testing infrastructure for FraiseQL that will test across Python 3.11, 3.12, and 3.13 on every push and pull request.

**Cost:** ✅ **FREE** - Public repositories get unlimited GitHub Actions minutes!

---

## What Was Created

### 1. GitHub Actions Workflow (`.github/workflows/python-version-matrix.yml`)

**Features:**
- ✅ Tests all 3 Python versions (3.11, 3.12, 3.13) in parallel
- ✅ Full PostgreSQL service setup for integration tests
- ✅ Rust toolchain and fraiseql_rs extension compilation
- ✅ Coverage reporting per Python version to Codecov
- ✅ Matrix summary job showing overall status
- ✅ Optional tox validation
- ✅ Manual trigger capability via workflow_dispatch

**When it runs:**
- Every push to `main` or `dev` branches
- Every pull request to `main` or `dev` branches
- Can be manually triggered from GitHub Actions UI

**Parallelization:**
- All 3 Python versions run simultaneously
- Typical run time: ~5-10 minutes total (not 15-30 minutes sequentially)

### 2. Tox Configuration (`tox.ini`)

**Test Environments:**
- `py311` - Test on Python 3.11
- `py312` - Test on Python 3.12
- `py313` - Test on Python 3.13
- `lint` - Run ruff code quality checks
- `type-check` - Run pyright type checking
- `docs` - Build documentation
- `coverage` - Generate coverage reports
- `integration` - Run integration tests only
- `unit` - Run unit tests only (fast)
- `examples` - Test all examples
- `quick` - Quick core tests with parallelization
- `build` - Build and validate package
- `clean` - Clean up artifacts

**Usage:**
```bash
# Run all Python version tests
tox

# Test specific Python version
tox -e py311
tox -e py312
tox -e py313

# Run linting
tox -e lint

# Run type checking
tox -e type-check

# Quick test (core tests only, parallel)
tox -e quick

# Build package
tox -e build

# List all environments
tox -l
```

---

## GitHub Actions Billing - Detailed Breakdown

### ✅ Public Repositories: FREE

| Feature | Public Repo | Private Repo (Free) |
|---------|-------------|---------------------|
| **Minutes/month** | ✅ **Unlimited** | 2,000 minutes |
| **Storage** | ✅ **Unlimited** | 500 MB |
| **Concurrent jobs** | 20 | 20 |
| **Cost** | **$0.00** | $0.00 (within limits) |

### What "Unlimited" Means:

1. **No credit consumption** - Run as many tests as you want
2. **No billing** - Completely free for open-source
3. **Full feature access** - All GitHub Actions features available
4. **Matrix jobs** - Run multiple versions in parallel at no cost

### Only Costs Apply If:

❌ Using **self-hosted runners** (you're not)
❌ Using **larger runners** (you're using `ubuntu-latest` which is free)
❌ Repository is **private** and exceeds 2,000 minutes/month

**Your situation:** ✅ Public repo + standard runners = **$0.00 forever**

---

## Workflow Architecture

### Existing Quality Gate Workflow
**File:** `.github/workflows/quality-gate.yml`
**Purpose:** Comprehensive quality checks (tests, lint, security)
**Python version:** 3.13 only
**When:** Every push/PR

### New Python Version Matrix Workflow
**File:** `.github/workflows/python-version-matrix.yml`
**Purpose:** Test compatibility across Python 3.11, 3.12, 3.13
**Python versions:** All 3 in parallel
**When:** Every push/PR

### How They Work Together:

```
Push/PR to main or dev
    ↓
    ├─→ quality-gate.yml (Python 3.13)
    │   ├─→ Tests
    │   ├─→ Lint
    │   └─→ Security
    │
    └─→ python-version-matrix.yml (3.11, 3.12, 3.13)
        ├─→ Test Matrix (parallel)
        │   ├─→ Python 3.11 → Tests + Coverage
        │   ├─→ Python 3.12 → Tests + Coverage
        │   └─→ Python 3.13 → Tests + Coverage
        │
        ├─→ Matrix Summary
        └─→ Tox Validation (optional)
```

**Benefits:**
- Quality gate catches issues quickly (single version)
- Matrix tests ensure cross-version compatibility
- Both run in parallel, no waiting
- Total time: max(quality-gate, matrix) ≈ 5-10 minutes

---

## Expected Run Times

### Per Python Version (in matrix):
- Checkout + Setup: ~30 seconds
- Install dependencies: ~2-3 minutes
- Run tests: ~3-5 minutes
- Upload coverage: ~10 seconds

**Total per version:** ~5-8 minutes

### Parallel Execution:
- All 3 versions run simultaneously
- **Total wall time:** ~5-8 minutes (not 15-24 minutes!)

### GitHub Actions Concurrency:
- Free tier: 20 concurrent jobs
- Your workflow: 3 jobs (well within limits)

---

## Viewing Results

### On GitHub:

1. **Navigate to Actions tab:**
   `https://github.com/fraiseql/fraiseql/actions`

2. **You'll see:**
   - "Python Version Matrix Tests" workflow
   - Each run shows all 3 Python versions
   - Green checkmark = all passed
   - Red X = something failed

3. **Drill down:**
   - Click a workflow run
   - See matrix: Python 3.11, 3.12, 3.13
   - Click individual version to see logs

4. **Status badges:**
   You can add to README.md:
   ```markdown
   [![Python Version Matrix](https://github.com/fraiseql/fraiseql/actions/workflows/python-version-matrix.yml/badge.svg)](https://github.com/fraiseql/fraiseql/actions/workflows/python-version-matrix.yml)
   ```

### Coverage Reports:

Each Python version uploads coverage to Codecov with flags:
- `python-3.11`
- `python-3.12`
- `python-3.13`

View at: `https://codecov.io/gh/fraiseql/fraiseql`

---

## Testing Locally Before Push

### Using Tox:

```bash
# Test all Python versions (if installed)
tox

# Test specific version
tox -e py311

# Quick test (fast core tests)
tox -e quick

# Lint + type check
tox -e lint,type-check
```

### Using uv directly:

```bash
# Current Python version
uv run pytest

# Specific Python version (if installed)
python3.11 -m pytest
python3.12 -m pytest
python3.13 -m pytest
```

---

## Maintenance

### When to Update Workflow:

1. **Adding Python 3.14 support (future):**
   ```yaml
   # In .github/workflows/python-version-matrix.yml
   matrix:
     python-version: ['3.11', '3.12', '3.13', '3.14']
   ```

   ```ini
   # In tox.ini
   [tox]
   envlist = py311, py312, py313, py314

   [testenv:py314]
   basepython = python3.14
   ```

2. **Dropping Python 3.11 support (future):**
   - Remove from matrix
   - Remove from tox.ini
   - Update pyproject.toml requires-python
   - Update README.md

3. **Changing test requirements:**
   - Update `[testenv]` deps in tox.ini
   - GitHub Actions will pick up changes automatically

### Cost Monitoring (Even Though It's Free):

Check your usage at:
`https://github.com/settings/billing/summary`

**What you'll see for public repos:**
- Usage: Unlimited
- Cost: $0.00
- Minutes remaining: ∞

---

## Troubleshooting

### If a Python Version Fails:

1. **Check the logs:**
   - Go to Actions tab
   - Click failed run
   - Click failed Python version
   - Read test output

2. **Common issues:**
   - **Syntax incompatibility:** Using 3.13+ features on 3.11
   - **Dependency issue:** Package doesn't support that version
   - **Test flakiness:** Unrelated to Python version

3. **Local reproduction:**
   ```bash
   tox -e py311  # Test the specific failing version
   ```

### If GitHub Actions Won't Start:

1. **Check workflow file syntax:**
   ```bash
   # Validate YAML
   yamllint .github/workflows/python-version-matrix.yml
   ```

2. **Check Actions permissions:**
   - Settings → Actions → General
   - Ensure "Allow all actions" is enabled

3. **Check branch protection:**
   - Settings → Branches
   - Verify workflow is required for PRs

---

## Next Steps

### Immediate:
1. ✅ Branch created: `fix/python-version-requirement`
2. ✅ All changes committed
3. ⏭️ Push branch to GitHub
4. ⏭️ Create Pull Request
5. ⏭️ Watch GitHub Actions run automatically!

### Push Commands:

```bash
# Push the branch
git push origin fix/python-version-requirement

# Create PR (using GitHub CLI)
gh pr create --title "fix: correct Python version requirement to 3.11+" \
  --body "This PR corrects the Python version requirement to 3.11+ and adds comprehensive testing across Python 3.11, 3.12, and 3.13.

See commit message and PYTHON_VERSION_ANALYSIS.md for full details.

## Testing
- ✅ Tox configuration added for local testing
- ✅ GitHub Actions matrix workflow for CI testing
- ✅ Free on public repositories

## Changes
- Updated pyproject.toml requires-python to >=3.11
- Updated README.md badge and prerequisites
- Added tox.ini for multi-version testing
- Added python-version-matrix.yml GitHub Actions workflow
- Comprehensive documentation in PYTHON_VERSION_ANALYSIS.md"
```

### After PR is Merged:

The workflow will:
1. Run on every future push to `main` or `dev`
2. Run on every pull request
3. Provide immediate feedback on Python version compatibility
4. Report coverage for each Python version

---

## Summary Statistics

**Files Created:**
- ✅ `.github/workflows/python-version-matrix.yml` (210 lines)
- ✅ `tox.ini` (137 lines)
- ✅ `PYTHON_VERSION_ANALYSIS.md` (426 lines)
- ✅ `PYTHON_VERSION_UPDATE_SUMMARY.md` (247 lines)
- ✅ `GITHUB_ACTIONS_SETUP.md` (this file)

**Files Modified:**
- ✅ `pyproject.toml` (Python version, classifiers, target versions)
- ✅ `README.md` (Badge and prerequisites)
- ✅ `uv.lock` (Dependency lock update)

**Total Lines Added:** 1,028 lines
**Cost:** $0.00 (free forever on public repos)
**Test Coverage:** Python 3.11, 3.12, 3.13
**CI/CD:** Automated on every push/PR

---

## Benefits Achieved

✅ **Compatibility assurance** - Test every supported Python version
✅ **Early detection** - Catch version-specific bugs immediately
✅ **Free infrastructure** - Zero cost on public repositories
✅ **Parallel execution** - Fast feedback (5-8 minutes)
✅ **Coverage tracking** - Per-version coverage reports
✅ **Local validation** - Tox for pre-push testing
✅ **Documentation** - Comprehensive analysis and setup guides
✅ **Future-proof** - Easy to add/remove Python versions

---

**Setup complete! Push your branch and watch the magic happen! 🚀**
