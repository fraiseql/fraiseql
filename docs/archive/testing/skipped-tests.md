# Skipped Tests Documentation

**Version:** v1.7.0
**Last Updated:** 2025-11-24
**Total Skipped:** 28 tests
**Total Passing:** 4,603 tests
**Skip Rate:** 0.6% (excellent test coverage)

---

## Executive Summary

FraiseQL maintains a comprehensive test suite with 4,603 passing tests and only 28 skipped tests (0.6% skip rate). All skipped tests have valid technical reasons and clear documentation for how to run them individually or enable them in specific environments.

**Why Tests Are Skipped:**
- Schema Registry Singleton (7 tests) - Architectural limitation, tests pass individually
- External Services Required (10 tests) - KMS providers, database features
- Incomplete Features (5 tests) - Work in progress
- Known External Issues (3 tests) - Third-party limitations
- Advanced Type System (2 tests) - Future enhancement
- Deprecated Features (1 test) - Legacy code path

---

## Category 1: Schema Registry Singleton (7 tests) 🏗️

### Root Cause
FraiseQL's Schema Registry is a global singleton that can only be initialized once per Python process. This is by design for performance and consistency, but it means tests that create multiple schemas cannot run in the same process.

### Status: ✅ Tests Pass Individually

All these tests work perfectly when run in isolation. The skip is only needed for full test suite runs.

### Affected Tests

1. **`tests/regression/test_issue_112_nested_jsonb_typename.py`** (4 tests)
   - `test_nested_object_has_correct_typename`
   - `test_nested_object_has_all_fields`
   - `test_nested_object_type_inference_from_schema`
   - `test_multiple_assignments_all_have_correct_nested_typename`

   **Run individually:**
   ```bash
   pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
   ```

   **Purpose:** Regression tests for Issue #112 - ensures nested JSONB objects return correct `__typename` and all fields.

2. **`tests/integration/examples/test_blog_simple_integration.py::test_blog_simple_basic_queries`**

   **Run individually:**
   ```bash
   pytest tests/integration/examples/test_blog_simple_integration.py::test_blog_simple_basic_queries -v
   ```

   **Purpose:** Integration test for blog_simple example - validates basic queries work correctly.

3. **`tests/integration/examples/test_blog_simple_integration.py::test_blog_simple_performance_baseline`**

   **Run individually:**
   ```bash
   pytest tests/integration/examples/test_blog_simple_integration.py::test_blog_simple_performance_baseline -v
   ```

   **Purpose:** Performance baseline test for blog_simple example.

4. **`tests/integration/graphql/test_graphql_query_execution_complete.py::test_graphql_field_selection`**

   **Run individually:**
   ```bash
   pytest tests/integration/graphql/test_graphql_query_execution_complete.py::test_graphql_field_selection -v
   ```

   **Purpose:** Validates Rust field projection filters fields to only those requested in GraphQL query.

5. **`tests/unit/core/test_rust_pipeline.py::test_build_graphql_response_with_nested_object_aliases`**

   **Run individually:**
   ```bash
   pytest tests/unit/core/test_rust_pipeline.py::test_build_graphql_response_with_nested_object_aliases -v
   ```

   **Purpose:** Tests field selections with nested object aliases in Rust pipeline.

### Future Resolution

**Option A (Preferred):** Keep as-is - tests work individually, minimal impact
**Option B:** Refactor to use pytest-xdist with process isolation (requires significant fixture refactoring)
**Option C:** Make Schema Registry process-local instead of global (breaking architectural change)

**Recommendation:** Keep current approach. Individual test execution is documented and works perfectly.

---

## Category 2: External Services Required (10 tests) 🔌

### Root Cause
These tests require external services (Vault, AWS KMS, pgvector) that are not available in standard CI environments.

### Status: ⚠️ Can Be Enabled Locally

Set appropriate environment variables to enable these tests.

### 2.1 KMS Integration Tests (6 tests)

#### Vault Tests (3 tests)
**Location:** `tests/integration/security/test_kms_integration.py`

```python
@pytest.mark.skipif(not os.environ.get("VAULT_ADDR"), reason="Vault not configured")
```

**Tests:**
- `test_vault_provider_encrypt_decrypt`
- `test_vault_provider_key_rotation`
- `test_vault_provider_connection_error`

**Enable locally:**
```bash
# Start Vault in dev mode
docker run --rm --cap-add=IPC_LOCK \
  -e 'VAULT_DEV_ROOT_TOKEN_ID=myroot' \
  -p 8200:8200 \
  --name=vault vault:1.13.3

# Set environment variables
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=myroot
export VAULT_MOUNT=transit
export VAULT_KEY_NAME=fraiseql

# Run tests
pytest tests/integration/security/test_kms_integration.py::test_vault_provider_encrypt_decrypt -v
```

#### AWS KMS Tests (3 tests)
**Location:** `tests/integration/security/test_kms_integration.py`

```python
@pytest.mark.skipif(not os.environ.get("AWS_REGION"), reason="AWS not configured")
```

**Tests:**
- `test_aws_kms_provider_encrypt_decrypt`
- `test_aws_kms_provider_key_rotation`
- `test_aws_kms_provider_connection_error`

**Enable locally:**
```bash
# Configure AWS credentials
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=your_key_id
export AWS_SECRET_ACCESS_KEY=your_secret_key
export AWS_KMS_KEY_ID=your_kms_key_id

# Or use LocalStack for testing
docker run --rm -p 4566:4566 localstack/localstack
export AWS_ENDPOINT_URL=http://localhost:4566

# Run tests
pytest tests/integration/security/test_kms_integration.py::test_aws_kms_provider_encrypt_decrypt -v
```

### 2.2 Cascade Feature Tests (4 tests)
**Location:** `tests/integration/test_graphql_cascade.py`

**Skip Reason:** "Cascade feature not fully implemented"

**Tests:**
- `test_cascade_mutation_updates_cache`
- `test_cascade_delete_propagates`
- `test_cascade_update_with_side_effects`
- `test_cascade_tracks_affected_queries`

**Status:** 🚧 Work in Progress

The GraphQL Cascade feature is planned but not yet implemented. These tests define the expected behavior for automatic cache invalidation and side effect tracking.

**Enable when:** Cascade feature is completed (planned for v2.1)

---

## Category 3: Database Schema Required (2 tests) 📋

### Root Cause
Tests require SpecQL-generated database schema that must be created separately.

**Location:** `tests/integration/test_introspection/test_composite_type_generation_integration.py`

**Skip Reason:** "SpecQL test schema not found - run SpecQL or apply test schema SQL"

**Tests:**
- `test_composite_type_to_graphql_type`
- `test_enum_type_generation`

**Enable by creating test schema:**
```bash
# Create test database
psql -c "CREATE DATABASE specql_test;"

# Apply SpecQL test schema
psql specql_test < tests/fixtures/specql_test_schema.sql

# Run tests
pytest tests/integration/test_introspection/test_composite_type_generation_integration.py -v
```

**Alternative:** Set `SPECQL_TEST_DB` environment variable:
```bash
export SPECQL_TEST_DB=postgresql://localhost/specql_test
pytest tests/integration/test_introspection/ -v
```

---

## Category 4: Known External Issues (3 tests) 🐛

### 4.1 Starlette TestClient Lifespan Deadlock (1 test)
**Location:** `tests/system/fastapi_system/test_lifespan.py:157`

**Skip Reason:**
> Starlette TestClient hangs on lifespan errors due to thread join deadlock. This is a known limitation - lifespan error handling works in production but cannot be reliably tested with TestClient. See: starlette issue #1315

**Test:** `test_lifespan_error_handling`

**Status:** ❌ Cannot Fix (Third-party Issue)

**Upstream Issue:** https://github.com/encode/starlette/issues/1315

**Workaround:** Manual testing with actual server:
```python
# works in production, just can't be tested with TestClient
uvicorn myapp:app --host 0.0.0.0 --port 8000
```

### 4.2 Flaky Performance Test (1 test)
**Location:** `tests/performance/test_rustresponsebytes_performance.py:39`

**Skip Reason:** "Flaky performance test - threshold depends on system load. Target: <2ms for 10,000 checks"

**Test:** `test_rustresponsebytes_check_performance`

**Status:** ⚠️ System-Dependent

**Enable manually:**
```bash
# Run on quiet system with minimal load
pytest tests/performance/test_rustresponsebytes_performance.py::test_rustresponsebytes_check_performance -v
```

**Note:** Performance thresholds are environment-specific. Test passes on development machines but may fail on CI runners.

### 4.3 APQ PostgreSQL Connection (1 test)
**Location:** `tests/test_apq_registration.py:93`

**Skip Reason:** "Requires actual PostgreSQL connection"

**Test:** `test_apq_postgresql_backend_registration`

**Enable with database:**
```bash
# Start PostgreSQL
docker run --rm -p 5432:5432 -e POSTGRES_PASSWORD=test postgres:15

# Set connection string
export DATABASE_URL=postgresql://postgres:test@localhost/postgres

# Run test
pytest tests/test_apq_registration.py::test_apq_postgresql_backend_registration -v
```

---

## Category 5: Advanced Type System (2 tests) 🧬

### Root Cause
Tests require advanced forward reference and self-referential type handling that's not yet implemented.

**Location:** `tests/unit/sql/test_nested_where_input_auto_generation.py`

**Tests:**
- `test_self_referential_type` (line 58)
- `test_forward_reference_during_decoration` (line 167)

**Skip Reasons:**
- "Self-referential types require advanced forward reference handling"
- "Forward references during decoration require special handling - use correct definition order instead"

**Status:** 🚧 Future Enhancement

**Workaround:** Use correct definition order:
```python
# ❌ Don't do this (forward reference)
@fraise_type
class Node:
    parent: Optional["Node"]  # Forward reference to self

# ✅ Do this instead (explicit ordering)
@fraise_type
class Node:
    id: int
    parent_id: Optional[int]

# Define relationship separately
```

**Enable when:** Type system supports PEP 563 (postponed evaluation of annotations)

---

## Category 6: Deprecated Features (1 test) 🗑️

### Root Cause
Legacy code path that has been superseded by newer implementation.

**Location:** `tests/unit/core/test_rust_pipeline.py:117`

**Skip Reason:** "Legacy field_paths projection not supported in schema-aware pipeline. Use field_selections with aliases instead. See docs/rust/rust-field-projection.md"

**Test:** `test_legacy_field_paths_projection`

**Status:** ✅ Intentionally Skipped

**Replacement:** Use `field_selections` with aliases (see `docs/rust/rust-field-projection.md`)

**Old API:**
```python
# ❌ Deprecated
field_paths = ["user.id", "user.name"]
```

**New API:**
```python
# ✅ Current
field_selections = [
    FieldSelection(field="id", alias=None, materialized_path="user.id"),
    FieldSelection(field="name", alias=None, materialized_path="user.name"),
]
```

---

## Summary Statistics

| Category | Count | Can Enable? | How |
|----------|-------|-------------|-----|
| Schema Registry Singleton | 7 | ✅ Yes | Run individually |
| External Services | 10 | ⚠️ With setup | Configure Vault/AWS/DB |
| Incomplete Features | 5 | 🚧 Future | Wait for implementation |
| Known External Issues | 3 | ❌ No | Third-party limitations |
| Advanced Type System | 2 | 🚧 Future | Requires type system work |
| Deprecated Features | 1 | ❌ No | Use new API instead |
| **Total** | **28** | **7 can enable** | **See above** |

---

## Running Specific Skipped Tests

### Run All Schema Registry Tests
```bash
# Run each file individually
pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
pytest tests/integration/examples/test_blog_simple_integration.py::test_blog_simple_basic_queries -v
pytest tests/integration/graphql/test_graphql_query_execution_complete.py::test_graphql_field_selection -v
pytest tests/unit/core/test_rust_pipeline.py::test_build_graphql_response_with_nested_object_aliases -v
```

### Run KMS Tests (with services)
```bash
# Start Vault
docker run -d --rm --cap-add=IPC_LOCK \
  -e 'VAULT_DEV_ROOT_TOKEN_ID=myroot' \
  -p 8200:8200 \
  --name=vault vault:1.13.3

# Configure environment
export VAULT_ADDR=http://localhost:8200
export VAULT_TOKEN=myroot
export VAULT_MOUNT=transit
export VAULT_KEY_NAME=fraiseql

# Run Vault tests
pytest tests/integration/security/test_kms_integration.py -k vault -v

# Cleanup
docker stop vault
```

### Run SpecQL Tests (with schema)
```bash
# Setup test database
createdb specql_test
psql specql_test < tests/fixtures/specql_test_schema.sql

# Run tests
pytest tests/integration/test_introspection/test_composite_type_generation_integration.py -v

# Cleanup
dropdb specql_test
```

---

## CI/CD Considerations

### GitHub Actions
All 28 skipped tests are expected in CI. The skip conditions handle missing services gracefully:

```yaml
# .github/workflows/test.yml
- name: Run Tests
  run: |
    pytest --tb=short -v
    # Expected: 4603 passed, 28 skipped
```

### Local Development
Developers can enable specific tests by setting up external services:

```bash
# .env.test
VAULT_ADDR=http://localhost:8200
VAULT_TOKEN=myroot
AWS_REGION=us-east-1
DATABASE_URL=postgresql://localhost/test
```

### Pre-Commit Hook
Schema Registry tests should be run individually before committing changes to schema code:

```bash
# .git/hooks/pre-commit
pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
```

---

## Maintenance Guidelines

### When Adding New Tests

1. **Default to NOT skipping** - Only skip if absolutely necessary
2. **Document skip reason** - Use descriptive, actionable skip messages
3. **Provide run instructions** - Show how to enable the test locally
4. **Update this document** - Add new skip to appropriate category

### When Fixing Skipped Tests

1. **Remove skip marker** - Delete `@pytest.mark.skip` or `@pytest.mark.skipif`
2. **Verify in CI** - Ensure test passes in GitHub Actions
3. **Update this document** - Remove from skip list, update count
4. **Announce in changelog** - Note test coverage improvement

### Review Schedule

- **Monthly:** Review skipped tests for potential fixes
- **Per Release:** Verify skip counts match this documentation
- **Annual:** Evaluate architectural changes to reduce Schema Registry skips

---

## Related Documentation

- [Testing Checklist](../reference/testing-checklist.md) - Testing documentation
- [Rust Field Projection](../rust/rust-field-projection.md) - Field selection API
- [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) - KMS provider architecture

---

**Last Updated:** 2025-11-24
**Next Review:** 2026-01-24
**Maintained By:** FraiseQL Core Team
**Questions:** https://github.com/fraiseql/fraiseql/issues
