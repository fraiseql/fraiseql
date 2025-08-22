# Comprehensive Test Suite Fix for FraiseQL

## Overview
The FraiseQL test suite has 6 failing tests out of 2504 total tests. These failures fall into two categories:
1. Test isolation issues (2 tests)
2. Database connection issues (4 tests)

## Current State
- **Total Tests**: 2504
- **Passing**: 2498
- **Failing**: 6
- **Skipped**: 14

## Goal
Fix all 6 failing tests to achieve 100% pass rate for the v0.3.4 release.

## Failing Tests Summary

### Category 1: Test Isolation Issues
Tests that pass individually but fail in the full suite:
- `tests/database/test_developer_experience.py::TestValidationUtilities::test_validate_where_input_type_mismatch`
- `tests/gql/test_validation_extended.py::TestValidateWhereInput::test_operator_type_validation`

### Category 2: Database Connection Issues
Tests expecting local PostgreSQL with "fraiseql" role:
- `tests/test_nested_tenant_fix_real_db.py::test_nested_organization_without_tenant_id`
- `tests/test_nested_tenant_fix_real_db.py::test_comparison_with_and_without_embedded`
- `tests/test_resolve_nested_parameter.py::test_default_behavior_assumes_embedded`
- `tests/test_resolve_nested_parameter.py::test_explicit_nested_resolution`

## Fix Strategy

### For Test Isolation Issues:
1. Add proper cleanup fixtures that clear:
   - GraphQL type cache
   - Schema registry
   - Any global state
2. Ensure each test starts with a clean slate
3. Use pytest's `autouse` fixtures where needed

### For Database Connection Issues:
1. Update tests to use `db_pool` fixture
2. Remove hardcoded connection strings
3. Ensure testcontainers support
4. Follow the pattern from working integration tests

## Success Criteria
- All 2504 tests pass
- Tests pass both individually and in full suite
- Tests work with both local PostgreSQL and testcontainers
- No test order dependencies

## Testing Commands
```bash
# Run full suite
.venv/bin/pytest tests/

# Run each problematic test individually
.venv/bin/pytest tests/database/test_developer_experience.py::TestValidationUtilities::test_validate_where_input_type_mismatch

# Run with verbose output to debug
.venv/bin/pytest tests/ -xvs
```
