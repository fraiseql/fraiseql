# CI Failure Root Cause - FOUND!

**Date**: 2025-10-22
**PR**: https://github.com/fraiseql/fraiseql/pull/91

---

## 🎯 Root Cause: Rust Extension Not Built in CI

### The Real Error

```
AttributeError: module 'fraiseql_rs' has no attribute 'build_graphql_response'
```

**This has NOTHING to do with `pg_fraiseql_cache`!**

The Rust extension (`fraiseql_rs`) is not being compiled properly in the CI environment, causing ~40 tests to fail when they try to call Rust functions.

---

## 📊 Analysis

### What We Thought

Initially, the error pattern suggested database/caching issues:
- 5 caching tests failed
- 35+ repository/filtering tests failed
- All failures were in database integration tests

This led to hypothesis: "`pg_fraiseql_cache` extension missing"

### What's Actually Happening

ALL failing tests have the same error:
```python
src/fraiseql/core/rust_pipeline.py:129: in execute_via_rust_pipeline
    response_bytes = fraiseql_rs.build_graphql_response(
                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
E   AttributeError: module 'fraiseql_rs' has no attribute 'build_graphql_response'
```

**The Rust module is imported but not properly built, so it's missing its functions.**

---

## 🔍 Why It Works Locally But Fails in CI

### Local Environment ✅
```bash
# Locally, you have:
1. Rust toolchain installed
2. fraiseql_rs extension properly compiled
3. Extension installed in Python environment
4. All Rust functions available
```

### CI Environment ❌
```bash
# CI has:
1. Python installed
2. Dependencies installed via pip/uv
3. BUT: Rust extension NOT compiled
4. fraiseql_rs module exists but is empty/incomplete
```

---

## 🛠️ The Fix

### Check CI Workflow

Look at `.github/workflows/quality-gate.yml`:

```yaml
- name: Install dependencies
  run: |
    uv sync
    # ⚠️ Missing: Rust extension build step!
```

### Add Rust Build Step

```yaml
- name: Setup Rust
  uses: actions-rs/toolchain@v1
  with:
    toolchain: stable
    profile: minimal

- name: Build Rust extension
  run: |
    cd fraiseql_rs
    cargo build --release
    # Or use maturin if that's your build tool
    maturin develop --release

- name: Install dependencies
  run: |
    uv sync
```

### Alternative: Use Pre-built Wheels

If you publish wheels with the Rust extension pre-compiled:

```yaml
- name: Install dependencies
  run: |
    # Install from pre-built wheel that includes Rust extension
    uv pip install fraiseql[rust]
```

---

## 📝 Quick Fix Steps

### Option 1: Add Rust Build to CI (Recommended)

1. Edit `.github/workflows/quality-gate.yml`
2. Add Rust toolchain setup before tests
3. Build `fraiseql_rs` extension
4. Run tests

### Option 2: Skip Rust Tests in CI (Temporary)

Mark tests that require Rust as skippable:

```python
import pytest
import importlib.util

# Check if fraiseql_rs is properly built
fraiseql_rs = importlib.util.find_spec("fraiseql_rs")
HAS_RUST_EXTENSION = fraiseql_rs is not None

try:
    import fraiseql_rs
    HAS_BUILD_GRAPHQL_RESPONSE = hasattr(fraiseql_rs, 'build_graphql_response')
except ImportError:
    HAS_BUILD_GRAPHQL_RESPONSE = False

requires_rust = pytest.mark.skipif(
    not HAS_BUILD_GRAPHQL_RESPONSE,
    reason="Requires fraiseql_rs extension with build_graphql_response"
)

@requires_rust
async def test_cached_repository_passes_tenant_id_to_cache_key(...):
    ...
```

### Option 3: Provide Python Fallback

Ensure code works without Rust:

```python
# In rust_pipeline.py
try:
    import fraiseql_rs
    if hasattr(fraiseql_rs, 'build_graphql_response'):
        HAS_RUST = True
    else:
        HAS_RUST = False
        logger.warning("fraiseql_rs exists but build_graphql_response not found")
except ImportError:
    HAS_RUST = False
    logger.warning("fraiseql_rs not available, using Python fallback")

async def execute_via_rust_pipeline(...):
    if HAS_RUST:
        return fraiseql_rs.build_graphql_response(...)
    else:
        # Python fallback implementation
        return await execute_via_python_fallback(...)
```

---

## 🎯 Recommended Solution

**Use Option 1: Add Rust Build to CI**

This is the cleanest solution because:
1. Tests the actual production code path
2. Validates Rust integration works
3. No need to maintain fallback code
4. Matches local development environment

### Implementation

Edit `.github/workflows/quality-gate.yml`:

```yaml
name: Quality Gate

on:
  pull_request:
  push:
    branches: [main, dev]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:15
        env:
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: test_db
        ports:
          - 5432:5432
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.13'

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          profile: minimal
          override: true

      - name: Install uv
        run: pip install uv

      - name: Build Rust extension
        run: |
          cd fraiseql_rs
          pip install maturin
          maturin develop --release

      - name: Install dependencies
        run: uv sync

      - name: Run tests
        env:
          DATABASE_URL: postgresql://postgres:postgres@localhost:5432/test_db
        run: uv run pytest --tb=short

      - name: Run linters
        run: |
          uv run ruff check src/
          uv run ruff format --check src/
```

---

## ✅ Verification

After applying the fix, all 40+ failing tests should pass because:

1. Rust extension will be properly built
2. `fraiseql_rs.build_graphql_response` will be available
3. Tests can execute Rust pipeline code path
4. No more `AttributeError`

---

## 🎓 Lessons Learned

### Misleading Error Patterns

The failing test pattern (caching, filtering, repository tests) made it seem like a database/extension issue, but it was actually:

1. **All tests using `repository.find()`** failed
2. `find()` calls Rust pipeline
3. Rust pipeline not available in CI
4. Hence: all integration tests failed

### Always Check the Actual Error

Don't just look at test names - look at the actual error message!

```
❌ Wrong: "Caching tests failed → pg_fraiseql_cache missing"
✅ Right: "AttributeError: 'build_graphql_response' → Rust extension not built"
```

---

## 📞 For the Agent

### Your Task

1. **Edit `.github/workflows/quality-gate.yml`**
2. **Add Rust toolchain setup** (before tests)
3. **Add Rust extension build step** (maturin develop)
4. **Push changes**
5. **Watch CI** - all tests should now pass ✅

The fix is straightforward: CI needs to build the Rust extension just like your local environment does.

---

**Status**: Root cause identified ✅ | Solution documented ✅ | Ready to fix 🚀
