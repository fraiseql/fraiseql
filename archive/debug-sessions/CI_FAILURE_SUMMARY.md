# CI Failure Summary - PR #91

**Date**: 2025-10-22
**PR**: https://github.com/fraiseql/fraiseql/pull/91
**CI Run**: https://github.com/fraiseql/fraiseql/actions/runs/18723951737

---

## 🔴 Problem: Tests Pass Locally But Fail in CI

### Local Test Results ✅
```bash
$ uv run pytest --tb=short
===== 3,551 passed, 10 warnings in 71.86s =====
```

### CI Test Results ❌
```
Approximately 40+ tests FAILED in CI
```

---

## 📊 Failed Test Categories

### Category 1: Caching Tests (5 failures)
```
tests/integration/caching/test_pg_fraiseql_cache_integration.py::
  - test_cached_repository_passes_tenant_id_to_cache_key
  - test_different_tenants_get_different_cache_entries

tests/integration/caching/test_repository_integration.py::
  - test_find_with_cache_miss
  - test_skip_cache_option
  - test_custom_ttl
```

### Category 2: Dictionary WHERE Filter Tests (9 failures)
```
tests/integration/database/repository/test_dict_where_mixed_filters_bug.py::
  - test_dict_where_with_nested_filter_only
  - test_dict_where_with_direct_filter_only
  - test_dict_where_with_mixed_nested_and_direct_filters_BUG
  - test_dict_where_with_multiple_direct_filters_after_nested
  - test_dict_where_with_direct_filter_before_nested
```

### Category 3: Dynamic Filter Tests (4 failures)
```
tests/integration/database/repository/test_dynamic_filter_construction.py::
  - test_dynamic_dict_filter_construction
  - test_merged_dict_filters
  - test_empty_dict_where_to_populated
  - test_complex_nested_dict_filters
```

### Category 4: Hybrid Table Filtering (9 failures)
```
tests/integration/database/repository/test_hybrid_table_filtering_generic.py::
  - test_filter_by_regular_sql_column_is_active
  - test_dynamic_filter_construction_by_status
  - test_multiple_regular_column_filters
  - test_mixed_regular_and_jsonb_filtering
  - test_whereinput_type_on_hybrid_table

tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py::
  - test_nested_object_filter_on_hybrid_table
  - test_nested_object_filter_with_results
  - test_multiple_nested_object_filters
  - test_dict_based_nested_filter
```

### Category 5: Repository WHERE Integration (12 failures)
```
tests/integration/database/repository/test_repository_where_integration.py::
  - test_find_with_simple_where_equality
  - test_find_with_comparison_operators
  - test_find_with_multiple_operators
  - test_find_with_multiple_fields
  - test_find_with_null_handling
  - test_find_with_date_filtering
  - test_find_one_with_where
  - test_combining_where_with_kwargs
  - test_rust_pipeline_returns_valid_json
  - test_empty_where_returns_all
  - test_unsupported_operator_is_ignored
  - test_complex_nested_where
```

---

## 🔍 Root Cause Analysis

### Likely Cause: Database State or Environment Difference

The pattern of failures suggests:

1. **Database-related tests are failing** - All failures are in integration tests that use PostgreSQL
2. **Local passes, CI fails** - Environment difference
3. **Specific feature areas** - Caching, filtering, WHERE clauses

### Possible Root Causes

#### Hypothesis 1: PostgreSQL Extension Missing
The caching tests might require the `pg_fraiseql_cache` extension which may not be installed in CI.

**Evidence**:
- Caching tests fail
- Code references extension detection: `test_extension_detected_when_installed`

**Check**:
```sql
-- CI might be missing this extension
CREATE EXTENSION IF NOT EXISTS pg_fraiseql_cache;
```

#### Hypothesis 2: Database Schema Not Created
The tests might expect certain database tables/views that aren't being created in CI.

**Evidence**:
- WHERE clause tests fail
- Hybrid table tests fail
- All are database integration tests

**Check**:
- Are test database migrations running in CI?
- Is test data being seeded properly?

#### Hypothesis 3: Test Isolation Issues
Tests might be interfering with each other in CI but not locally.

**Evidence**:
- Many tests in the same categories failing
- Could be transaction rollback issues

**Check**:
- Are tests properly isolated?
- Is database being reset between tests?

#### Hypothesis 4: PostgreSQL Version Difference
CI might be using a different PostgreSQL version with different behavior.

**Evidence**:
- Database-specific functionality failing
- JSON/JSONB handling differences between PG versions

**Check**:
```yaml
# .github/workflows/quality-gate.yml
services:
  postgres:
    image: postgres:15  # What version?
```

---

## 🎯 Investigation Steps for Agent

### Step 1: Check CI Workflow Configuration
```bash
# Look at the CI workflow file
cat .github/workflows/quality-gate.yml

# Check:
# 1. PostgreSQL version
# 2. Database name, user, password
# 3. Test database setup commands
# 4. Environment variables
```

### Step 2: Check Test Configuration
```bash
# Check pytest configuration
cat pyproject.toml | grep -A 50 "\[tool.pytest"

# Check conftest.py files
find tests/ -name "conftest.py" -exec cat {} \;

# Look for database setup
grep -r "CREATE DATABASE\|CREATE EXTENSION" tests/
```

### Step 3: Run Specific Failed Test Locally
```bash
# Try to reproduce one failing test
uv run pytest tests/integration/caching/test_pg_fraiseql_cache_integration.py::TestTenantIdInCacheKeys::test_cached_repository_passes_tenant_id_to_cache_key -vv --tb=long

# Check if it passes locally
# If it does, try with CI-like environment variables
CI=true GITHUB_ACTIONS=true uv run pytest ...
```

### Step 4: Check Database Fixtures
```bash
# Look at database setup in tests
cat tests/conftest.py | grep -A 30 "postgres\|database\|db_connection"

# Check if tests have proper database setup
grep -r "@pytest.fixture" tests/integration/database/
```

### Step 5: Check Pre-commit.ci Auto-fixes
The CI logs mention "pre-commit.ci - pr fail" which might have auto-fixed something.

```bash
# Check if pre-commit.ci made changes
git log --oneline -5
git diff HEAD~1

# The auto-fixes might have introduced issues
```

---

## 🛠️ Quick Fixes to Try

### Fix 1: Add PostgreSQL Extension to CI
Edit `.github/workflows/quality-gate.yml`:

```yaml
services:
  postgres:
    image: postgres:15
    env:
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: test_db
    options: >-
      --health-cmd pg_isready
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5

steps:
  - name: Setup PostgreSQL extensions
    run: |
      PGPASSWORD=postgres psql -h localhost -U postgres -d test_db -c "CREATE EXTENSION IF NOT EXISTS pg_fraiseql_cache;"
```

### Fix 2: Check Test Database Setup
Ensure tests are setting up the database properly:

```python
# In conftest.py or test files
@pytest.fixture(scope="session")
async def setup_test_database():
    # Create tables
    # Create extensions
    # Seed data
    yield
    # Teardown
```

### Fix 3: Run Tests with Verbose Output
Modify the CI workflow to get more info:

```yaml
- name: Run tests
  run: |
    uv run pytest --tb=long -vv tests/integration/caching/test_pg_fraiseql_cache_integration.py
```

---

## 📝 Action Plan for Agent

### Priority 1: Understand the Failure
```bash
# Get detailed error logs from one failing test
gh run view 18723951737 --log | grep -A 50 "test_cached_repository_passes_tenant_id_to_cache_key"

# Look for:
# - Error message
# - Traceback
# - What the test expected vs what it got
```

### Priority 2: Check CI Configuration
```bash
# Examine workflow file
cat .github/workflows/quality-gate.yml

# Check for:
# - PostgreSQL setup
# - Test database configuration
# - Environment variables
```

### Priority 3: Reproduce Locally
```bash
# Try to make local tests fail like CI
# 1. Use same PostgreSQL version
# 2. Use same environment variables
# 3. Run tests in same order as CI
```

### Priority 4: Fix and Verify
```bash
# Apply fix
# Run tests locally
# Push changes
# Watch CI: gh pr checks 91 --watch
```

---

## 🚨 Important Notes

### Why Tests Pass Locally But Fail in CI

This is a **classic integration test problem**:
- **Local**: You have the database set up, extensions installed, test data seeded
- **CI**: Fresh environment, needs everything set up from scratch

### Common Solutions

1. **Database setup scripts** - Ensure CI runs migration/setup scripts
2. **Environment parity** - Make CI environment match local
3. **Test fixtures** - Proper setup/teardown in test fixtures
4. **Idempotent tests** - Tests should work regardless of order or state

---

## 📞 For the Agent

**Your mission**: Figure out why these database integration tests fail in CI but pass locally, and fix it.

**Start here**:
1. Read `.github/workflows/quality-gate.yml`
2. Get detailed error from one failing test
3. Compare local vs CI database setup
4. Apply fix and verify

**Success looks like**: All CI checks green ✅

Good luck! 🚀
