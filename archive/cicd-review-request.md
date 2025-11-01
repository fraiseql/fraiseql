# CI/CD Expert Review Request - PR #104

## Context
We're modernizing our CI workflows for a mixed Python+Rust project (maturin-based) to use 2025 best practices with `uv` and `maturin develop`. The project is **FraiseQL** - a production-ready GraphQL framework for PostgreSQL with Rust-accelerated components.

## PR Overview
**PR #104**: "ci: modernize workflows to use maturin develop with uv (2025 best practices)"
- **URL**: https://github.com/fraiseql/fraiseql/pull/104
- **Branch**: `ci/modernize-maturin-uv-workflows`
- **Status**: 7/8 checks passing, 1 failing (Tox Validation)

## Current Status

### ✅ Passing Checks (7/8)
- **Tests** (2m38s): All Python tests pass with Rust extension working
- **Python 3.13** (2m19s): Cross-version testing successful
- **Lint** (31s): Code quality checks pass
- **Security** (1m3s): No security issues found
- **Quality Gate**: Overall gate passes
- **Python Version Matrix Summary**: Cross-version validation passes
- **pre-commit.ci**: All hooks pass

### ❌ Failing Check (1/8)
- **Tox Validation** (2m24s): Rust extension (`fraiseql_rs`) loads as `None` in tox environment
  - Error: `AttributeError: 'NoneType' object has no attribute 'build_graphql_response'`
  - All other tests pass, only Rust-dependent tests fail
  - Same tests pass perfectly in regular Python 3.13 matrix job

## Technical Architecture

### Project Structure
```
fraiseql/
├── src/fraiseql/           # Python package
│   ├── __init__.py         # Imports _fraiseql_rs
│   └── ...
├── fraiseql_rs/            # Rust source code
│   ├── src/
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── ...
├── Cargo.toml              # Root Cargo config (mixed project)
├── pyproject.toml          # PEP 517, maturin build backend
└── tox.ini                 # Tox configuration

```

### Build Configuration (`pyproject.toml`)
```toml
[build-system]
requires = ["maturin>=1.9,<2.0"]
build-backend = "maturin"

[tool.maturin]
python-source = "src"
python-packages = ["fraiseql"]
module-name = "fraiseql._fraiseql_rs"
include = ["src/fraiseql/py.typed", "fraiseql_rs/**/*"]  # ← Added in PR
features = ["pyo3/extension-module"]
```

### Root Cargo.toml
```toml
[lib]
name = "_fraiseql_rs"
crate-type = ["cdylib"]
path = "fraiseql_rs/src/lib.rs"
```

## Changes Made in PR #104

### 1. Workflow Modernization
**File**: `.github/workflows/quality-gate.yml`, `python-version-matrix.yml`

**Before**:
```yaml
- uv pip install -e ".[dev,all]"  # ❌ Doesn't build Rust extension
```

**After**:
```yaml
- uv tool install maturin
- uv venv
- uv pip install ".[dev,all]"
- uv run maturin develop --uv  # ✅ Builds Rust extension in debug mode
```

### 2. Tox Configuration Attempts
**File**: `tox.ini`

**Evolution of fixes**:
1. ✅ Added `fraiseql_rs/**/*` to maturin include (ensures Rust sources in sdist)
2. ✅ Added maturin as tox dependency
3. ✅ Added Cargo environment variables to passenv
4. ✅ Pre-build wheel with `maturin build --release` before tox
5. ✅ Use `tox --installpkg dist/fraiseql-*.whl` to install pre-built wheel
6. ✅ Disabled `isolated_build = false`

**Current state**: Despite all fixes, `fraiseql_rs` still loads as `None` in tox environment.

## The Tox Problem - Deep Dive

### Symptom
```python
# In tox environment:
from fraiseql import fraiseql_rs
print(fraiseql_rs)  # None

# In regular pytest (same wheel):
from fraiseql import fraiseql_rs
print(fraiseql_rs)  # <module '_fraiseql_rs' from '.../_fraiseql_rs.cpython-313-x86_64-linux-gnu.so'>
```

### What We Know
1. **The wheel is correct**:
   - Built with `maturin build --release`
   - Size: 771KB (contains compiled `.so` file)
   - Hash matches between runs
   - Successfully installs in tox environment

2. **The Python 3.13 matrix test works**:
   - Uses `uv run maturin develop --uv`
   - Rust extension loads perfectly
   - All tests pass (including Rust-dependent tests)

3. **Tox fails**:
   - Uses pre-built wheel with `--installpkg`
   - Wheel installs without errors
   - But `fraiseql_rs = None` at runtime
   - `commands_pre` verification shows: `✅ fraiseql_rs loaded: None`

### What We've Tried

#### Attempt 1: Include Rust sources in sdist
```toml
include = ["src/fraiseql/py.typed", "fraiseql_rs/**/*"]
```
**Result**: Rust sources now in sdist, but tox's isolated build still doesn't compile them.

#### Attempt 2: Add maturin to tox deps
```ini
deps =
    maturin>=1.9,<2.0
    ...
```
**Result**: Maturin available, but not invoked during wheel build from sdist.

#### Attempt 3: Pre-build wheel, install with --installpkg
```yaml
- maturin build --release --out dist/
- tox -e py313 --installpkg dist/fraiseql-*.whl
```
**Result**: Wheel installs, but `fraiseql_rs` still None. This is the most puzzling behavior.

#### Attempt 4: Disable isolated_build
```ini
isolated_build = false
```
**Result**: No change in behavior.

### Hypothesis
The issue may be related to how tox configures the Python environment or PYTHONPATH, preventing the `_fraiseql_rs.so` shared library from being found/loaded, even though it's present in the installed wheel.

## Questions for CI/CD Expert

### 1. **Tox + Maturin Best Practices**
- Is tox fundamentally incompatible with maturin-built packages?
- What's the recommended pattern for testing mixed Python+Rust packages in 2025?
- Should we abandon tox validation in favor of matrix testing?

### 2. **Wheel Build & Installation**
- Why would a wheel install successfully but the extension module not load?
- Could there be ABI compatibility issues between maturin build and tox's Python?
- Are there maturin build flags we should use for tox compatibility?

### 3. **Alternative Approaches**
- Should we use `maturin develop` inside tox instead of pre-built wheels?
- Is there a way to make tox's isolated build work with maturin?
- Should we use `nox` instead of `tox` for mixed Python+Rust projects?

### 4. **Current Workflow Assessment**
- Is our workflow structure sound for a production mixed Python+Rust package?
- Are we following 2025 best practices for `uv` + `maturin` integration?
- What would you change in our CI setup?

### 5. **Pragmatic Decision**
Given that:
- ✅ Regular pytest works perfectly (2m38s, all tests pass)
- ✅ Python 3.13 matrix test works perfectly (2m19s, Rust extension loads)
- ✅ Quality gate, lint, security all pass
- ❌ Only tox validation fails (but tests same functionality)

**Should we**:
- A) Remove tox validation from CI (it's duplicate coverage)?
- B) Keep investigating tox + maturin compatibility?
- C) Move to a different testing tool (nox, pytest-matrix)?

## Workflow Files to Review

### Primary Workflows
1. `.github/workflows/quality-gate.yml` - Main CI pipeline
2. `.github/workflows/python-version-matrix.yml` - Version testing + Tox

### Configuration
1. `pyproject.toml` - Build configuration
2. `tox.ini` - Tox configuration
3. `Cargo.toml` - Rust build configuration

## Expected Expert Review

Please provide:

1. **Pattern Analysis**: Are we following Python+Rust CI best practices in 2025?
2. **Tox Diagnosis**: Root cause of the `fraiseql_rs = None` issue in tox
3. **Recommendations**: Should we fix tox or remove it?
4. **Future-Proofing**: How to maintain this setup as maturin/uv evolve?

## Additional Context

### Project Constraints
- **Python**: 3.13+ only (dropped 3.11/3.12 support)
- **Rust**: Stable toolchain
- **Build**: Maturin 1.9+ (PEP 517 build backend)
- **CI**: GitHub Actions
- **Package Manager**: uv (Astral's fast package manager)

### Performance Requirements
- CI must complete in < 5 minutes
- Can't afford multi-hour matrix builds
- Need fast feedback loop for developers

### Production Requirements
- Package published to PyPI
- Must work on Linux, macOS, Windows (future)
- Wheels must contain pre-compiled Rust extensions
- Source distributions must be buildable from PyPI

## Review Checklist

Please assess:

- [ ] Workflow structure and organization
- [ ] Use of modern tools (uv, maturin)
- [ ] Build configuration correctness
- [ ] Test coverage and redundancy
- [ ] CI execution time and efficiency
- [ ] Maintainability and future-proofing
- [ ] The tox issue (fix or remove?)
- [ ] Security and best practices

## Thank You!

We deeply appreciate expert review from someone with experience in:
- Modern Python packaging (PEP 517, maturin, uv)
- Mixed Python+Rust projects
- CI/CD optimization
- Testing infrastructure

Your insights will help us ship a production-ready, maintainable CI pipeline.

---

**Reviewer**: Please feel free to clone the repo, run the workflows locally, or ask for additional information.

**Contact**: This PR and repo are open source - feel free to comment directly on PR #104.
