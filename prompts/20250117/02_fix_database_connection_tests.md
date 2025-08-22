# Fix Database Connection Tests in FraiseQL

## Problem
Several tests are failing because they expect a local PostgreSQL database with specific credentials that don't exist in the test environment.

## Failing Tests
1. `tests/test_nested_tenant_fix_real_db.py::test_nested_organization_without_tenant_id`
2. `tests/test_nested_tenant_fix_real_db.py::test_comparison_with_and_without_embedded`
3. `tests/test_resolve_nested_parameter.py::test_default_behavior_assumes_embedded`
4. `tests/test_resolve_nested_parameter.py::test_explicit_nested_resolution`

## Error Pattern
```
psycopg.OperationalError: connection failed: connection to server at "127.0.0.1", port 5432 failed: FATAL:  role "fraiseql" does not exist
```

## Task
Update these tests to use the test database infrastructure that other tests use successfully.

## Solution Approach
1. These tests should use the `db_pool` fixture from conftest.py
2. They should use testcontainers when no local database is available
3. Remove hardcoded database connection strings
4. Use the same database setup pattern as working integration tests

## Example Pattern to Follow
Look at how `tests/database/integration/test_json_passthrough_integration.py` handles database connections - it uses fixtures and testcontainers properly.

## Expected Outcome
All database-dependent tests should work with both local PostgreSQL and testcontainers.
