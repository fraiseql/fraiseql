# Phase 4: QA - Validation & Documentation

## Objective

Validate the implementation across different scenarios, ensure backward compatibility with v1.7.1 patterns, document the changes, and prepare for release.

## Context

**Current State**: All tests passing, code refactored and clean

**Target State**: Feature validated, documented, and ready for v1.8.1 release

## Files to Create/Modify

1. `CHANGELOG.md` - Document the fix
2. `docs/mutations/error-handling.md` - Update error handling documentation
3. `docs/migrations/v1.8.0-to-v1.8.1.md` - Migration guide (if needed)
4. `tests/regression/test_v1_7_1_error_compatibility.py` - Backward compatibility tests

## Implementation Steps

### Step 1: Backward Compatibility Testing

**Create**: `tests/regression/test_v1_7_1_error_compatibility.py`

Test that v1.7.1 error patterns still work with v1.8.1:

```python
"""Regression tests: Ensure v1.8.1 is compatible with v1.7.1 error patterns.

These tests verify that error handling patterns from v1.7.1 continue to work
in v1.8.1 after restoring error field population functionality.
"""

import pytest
import pytest_asyncio

import fraiseql
from fraiseql import type, success, failure, mutation, input
from fraiseql.gql.schema_builder import build_fraiseql_schema

pytestmark = pytest.mark.regression


@type
class Machine:
    """Legacy v1.7.1 entity."""
    id: str
    serial_number: str
    model_id: str


@failure
class CreateMachineError:
    """v1.7.1 style error with message and error_code."""
    message: str
    error_code: str  # This was populated from object_data in v1.7.1


@success
class CreateMachineSuccess:
    message: str
    machine: Machine


@input
class CreateMachineInput:
    serial_number: str
    model_id: str


@mutation(function="create_machine", schema="app")
class CreateMachine:
    input: CreateMachineInput
    success: CreateMachineSuccess
    failure: CreateMachineError


class TestV171ErrorCompatibility:
    """Verify v1.7.1 error patterns work in v1.8.1."""

    @pytest_asyncio.fixture
    async def setup_v171_pattern_db(self, db_connection):
        """Set up database using v1.7.1 error pattern (object_data field)."""
        async with db_connection.cursor() as cur:
            await cur.execute("CREATE SCHEMA IF NOT EXISTS app")

            # Note: v1.7.1 used object_data, v1.8.0+ uses entity
            # This test uses entity (new format) to populate object_data pattern
            await cur.execute("""
                CREATE OR REPLACE FUNCTION app.create_machine(input_data JSONB)
                RETURNS app.mutation_response AS $$
                BEGIN
                    -- Simulate v1.7.1 pattern: error_code in entity
                    IF (input_data->>'serial_number') = 'DUPLICATE' THEN
                        RETURN ROW(
                            'noop:machine_already_exists',
                            'Machine with this serial already exists',
                            NULL,
                            NULL,
                            jsonb_build_object('error_code', 'MACHINE_ALREADY_EXISTS'),
                            NULL,
                            NULL,
                            NULL
                        )::app.mutation_response;
                    END IF;

                    RETURN ROW(
                        'success',
                        'Machine created',
                        gen_random_uuid(),
                        'Machine',
                        jsonb_build_object(
                            'machine', jsonb_build_object(
                                'id', gen_random_uuid()::TEXT,
                                'serial_number', input_data->>'serial_number',
                                'model_id', input_data->>'model_id'
                            )
                        ),
                        ARRAY['created'],
                        NULL,
                        NULL
                    )::app.mutation_response;
                END;
                $$ LANGUAGE plpgsql;
            """)

        await db_connection.commit()
        yield db_connection

    @pytest.fixture
    def graphql_schema(self):
        """Build GraphQL schema."""
        @fraiseql.type
        class Query:
            dummy: str = "test"

        return build_fraiseql_schema(
            query_types=[Query],
            mutation_resolvers=[CreateMachine],
            camel_case_fields=True
        )

    @pytest.mark.asyncio
    async def test_v171_error_code_field_populated(
        self, setup_v171_pattern_db, graphql_schema
    ):
        """Test that v1.7.1 error_code pattern still works."""
        from graphql import execute, parse

        mutation = """
            mutation CreateMachine($input: CreateMachineInput!) {
                createMachine(input: $input) {
                    __typename
                    ... on CreateMachineError {
                        message
                        errorCode
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"serialNumber": "DUPLICATE", "modelId": "M123"}}
        )

        assert result.errors is None

        error = result.data["createMachine"]
        assert error["__typename"] == "CreateMachineError"
        assert error["message"] == "Machine with this serial already exists"

        # v1.7.1 behavior: error_code populated from object_data
        assert error["errorCode"] == "MACHINE_ALREADY_EXISTS"

    @pytest.mark.asyncio
    async def test_v171_error_without_custom_fields(
        self, setup_v171_pattern_db, graphql_schema
    ):
        """Test that errors without custom fields still work (standard 5 fields)."""
        from graphql import execute, parse

        mutation = """
            mutation CreateMachine($input: CreateMachineInput!) {
                createMachine(input: $input) {
                    ... on CreateMachineError {
                        __typename
                        message
                        status
                        code
                        errors {
                            code
                            message
                        }
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"serialNumber": "DUPLICATE", "modelId": "M123"}}
        )

        error = result.data["createMachine"]

        # Standard fields should always work
        assert error["__typename"] == "CreateMachineError"
        assert error["message"]
        assert error["status"] == "noop:machine_already_exists"
        assert error["code"] == 422
        assert isinstance(error["errors"], list)
        assert len(error["errors"]) > 0
```

### Step 2: Cross-Version Integration Tests

**Add to**: `tests/integration/mutations/test_error_field_population.py`

```python
class TestErrorFieldPopulationCrossVersion:
    """Test error field population works with various database patterns."""

    @pytest.mark.asyncio
    async def test_mixed_entity_metadata_fields(self, db_connection, graphql_schema):
        """Test fields from both entity AND metadata are populated correctly."""
        # Some fields in entity, some in metadata
        # Verify priority: entity first, metadata as fallback
        pass

    @pytest.mark.asyncio
    async def test_entity_without_matching_fields(self, db_connection, graphql_schema):
        """Test that entity with no matching fields doesn't break response."""
        # entity has fields that aren't in error class
        # Should ignore extra fields, populate what matches
        pass

    @pytest.mark.asyncio
    async def test_error_field_override_attempt_blocked(self, db_connection, graphql_schema):
        """Test that reserved fields in entity are NOT used (security)."""
        # entity contains 'message' or 'status' - should be ignored
        # Only use top-level message/status from mutation_response
        pass
```

### Step 3: Performance Validation

**Create**: `tests/performance/test_error_response_performance.py`

```python
"""Performance tests for error field population.

Ensure the new field extraction doesn't significantly slow down error responses.
"""

import pytest
import time

pytestmark = pytest.mark.performance


class TestErrorResponsePerformance:
    """Benchmark error response building."""

    @pytest.mark.asyncio
    async def test_error_response_with_many_fields(self, benchmark, db_connection, graphql_schema):
        """Benchmark error with 10+ custom fields."""
        # Create error with many custom fields
        # Measure time to build response
        # Target: < 5ms per response
        pass

    @pytest.mark.asyncio
    async def test_error_response_with_nested_entities(self, benchmark, db_connection, graphql_schema):
        """Benchmark error with deeply nested entity fields."""
        # Create error with 3-level nested entities
        # Measure transformation time
        # Target: < 10ms per response
        pass

    @pytest.mark.asyncio
    async def test_error_response_baseline(self, benchmark, db_connection, graphql_schema):
        """Benchmark simple error (no custom fields) as baseline."""
        # Standard 5-field error
        # Should be very fast (< 1ms)
        pass
```

### Step 4: Update Documentation

**File**: `docs/mutations/error-handling.md`

Add section on custom error fields:

```markdown
## Custom Error Fields

FraiseQL automatically populates custom error class fields from the database
`mutation_response.entity` or `mutation_response.metadata` fields.

### Basic Example

```python
@fraiseql.failure
class CreateUserError:
    message: str
    conflict_user: User | None = None  # Custom field
    conflict_count: int | None = None  # Custom scalar
```

### Database Pattern

Return error details in the `entity` field:

```sql
CREATE OR REPLACE FUNCTION app.create_user(input_data JSONB)
RETURNS app.mutation_response AS $$
DECLARE
    v_existing_user RECORD;
BEGIN
    SELECT * INTO v_existing_user
    FROM users
    WHERE email = input_data->>'email';

    IF FOUND THEN
        RETURN ROW(
            'failed:conflict',
            'User with this email already exists',
            v_existing_user.id,
            'User',
            jsonb_build_object(
                'conflict_user', row_to_json(v_existing_user),
                'conflict_count', 1
            ),  -- ← Custom fields here
            NULL,
            NULL,
            NULL
        )::app.mutation_response;
    END IF;

    -- ... success logic
END;
$$ LANGUAGE plpgsql;
```

### GraphQL Response

```json
{
  "createUser": {
    "__typename": "CreateUserError",
    "message": "User with this email already exists",
    "status": "failed:conflict",
    "code": 409,
    "errors": [...],
    "conflictUser": {
      "__typename": "User",
      "id": "...",
      "email": "existing@example.com"
    },
    "conflictCount": 1
  }
}
```

### Field Population Rules

1. **Entity First**: Custom fields are extracted from `entity` field first
2. **Metadata Fallback**: If not found in entity, check `metadata` field
3. **Reserved Fields**: `message`, `status`, `code`, `errors`, `__typename` cannot be overridden
4. **CamelCase**: Snake_case database fields → camelCase GraphQL fields (if enabled)
5. **Type Inference**: Nested entities automatically get `__typename` added

### Nested Entities

FraiseQL automatically infers entity types from field names:

```python
conflict_user: User          → __typename: "User"
validation_errors: list[ValidationError]  → __typename: "ValidationError"
existing_dns_server: DnsServer  → __typename: "DnsServer"
```

### Metadata vs Entity

**Use `entity` for**:
- Business objects (conflicting records, related data)
- Data the client should see
- Structured error context

**Use `metadata` for**:
- System-level details
- Internal tracking data
- Fallback when entity is used for something else
```

### Step 5: Update CHANGELOG

**File**: `CHANGELOG.md`

```markdown
## [v1.8.1] - 2025-XX-XX

### Fixed

- **Error Field Population Restored**: Custom error class fields are now populated
  from database `entity` and `metadata` fields, restoring v1.7.1 functionality
  lost during v1.8.0 Rust pipeline rewrite (#XXX)

  **What Changed**:
  - Error responses now extract custom fields from `mutation_response.entity`
  - Falls back to `mutation_response.metadata` if field not found in entity
  - Nested entities automatically get `__typename` added
  - CamelCase transformation applied to field keys

  **Migration**: No changes required. Existing database patterns will
  automatically populate custom error fields.

  **Example**:
  ```python
  @fraiseql.failure
  class CreateDnsServerError:
      message: str
      conflict_dns_server: DnsServer | None = None  # Now populated!
  ```

### Internal

- Added shared field extraction utilities (`field_extractor.rs`)
- Added entity type inference module (`type_inference.rs`)
- Comprehensive test coverage for error field population edge cases
```

### Step 6: Create Migration Guide (If Needed)

**File**: `docs/migrations/v1.8.0-to-v1.8.1.md`

```markdown
# Migrating from v1.8.0 to v1.8.1

## Overview

v1.8.1 is a **patch release** that restores error field population functionality
from v1.7.1 that was inadvertently lost in the v1.8.0 Rust pipeline rewrite.

## Breaking Changes

**None** - This is a bug fix release with no breaking changes.

## New Features

### Restored: Custom Error Field Population

Error classes can now have custom fields populated from database responses,
just like in v1.7.1.

**Before v1.8.1** (broken):
```json
{
  "createDnsServer": {
    "message": "Conflict",
    "conflictDnsServer": null  // ← Always null
  }
}
```

**After v1.8.1** (fixed):
```json
{
  "createDnsServer": {
    "message": "Conflict",
    "conflictDnsServer": {  // ← Populated!
      "id": "...",
      "ipAddress": "192.168.1.1"
    }
  }
}
```

## Action Required

**None** - Just upgrade to v1.8.1 and custom error fields will start working.

## Recommendations

1. **Review Error Classes**: Check if your error classes have custom fields
   that should be populated
2. **Update Database Functions**: Ensure your functions return error details
   in `entity` or `metadata` fields
3. **Test Error Responses**: Verify custom fields are populated correctly

## Need Help?

See [Error Handling Documentation](../mutations/error-handling.md) for examples
and best practices.
```

### Step 7: Manual QA Checklist

Test the implementation manually with real scenarios:

```bash
# Test 1: Simple error with custom object field
# - Create DNS server with duplicate IP
# - Verify conflict_dns_server is populated

# Test 2: Error with custom array field
# - Trigger validation error with multiple issues
# - Verify validation_errors array is populated

# Test 3: Error with metadata field
# - Trigger error that uses metadata instead of entity
# - Verify field is populated from metadata

# Test 4: Error with both entity and metadata
# - Trigger error with fields in both sources
# - Verify entity takes priority, metadata is fallback

# Test 5: CamelCase transformation
# - Check that snake_case DB fields → camelCase GraphQL
# - Verify in GraphQL Playground or introspection

# Test 6: Nested entity __typename
# - Query for __typename in nested error objects
# - Verify correct type name is added

# Test 7: Reserved fields not overridden
# - Try to override 'message' from entity
# - Verify top-level message is used, not entity's
```

## Verification Commands

### Run Full Test Suite

```bash
# All tests
uv run pytest -v

# Just error field population tests
uv run pytest tests/integration/mutations/test_error_field_population.py -v

# Regression tests
uv run pytest tests/regression/test_v1_7_1_error_compatibility.py -v

# Performance tests (optional)
uv run pytest tests/performance/test_error_response_performance.py -v --benchmark-only
```

### Validate Documentation

```bash
# Check all markdown links work
markdown-link-check docs/**/*.md

# Check code examples in docs are valid
# (manually review or use doc testing tool)
```

### Pre-Release Checks

```bash
# Ensure version bumped
grep -r "1.8.1" pyproject.toml Cargo.toml

# Ensure changelog updated
grep -A 5 "v1.8.1" CHANGELOG.md

# Build and test package
uv build
uv run pytest -v

# Build Rust extension
cd fraiseql_rs
cargo build --release
cargo test
cd ..
```

## Acceptance Criteria

### Testing
- [ ] All error field population tests pass
- [ ] All regression tests (v1.7.1 compatibility) pass
- [ ] All existing mutation tests still pass
- [ ] Performance tests show no significant regression (< 10% slower)
- [ ] Manual QA checklist completed

### Documentation
- [ ] `docs/mutations/error-handling.md` updated with custom fields section
- [ ] `CHANGELOG.md` updated with v1.8.1 entry
- [ ] Migration guide created (if needed)
- [ ] Code examples in docs are tested and correct
- [ ] All doc links are valid

### Release Preparation
- [ ] Version bumped to v1.8.1 in:
  - [ ] `pyproject.toml`
  - [ ] `fraiseql_rs/Cargo.toml`
  - [ ] `src/fraiseql/__init__.py`
- [ ] Git tag created: `v1.8.1`
- [ ] Release notes drafted
- [ ] PyPI package built and tested locally

### Code Quality
- [ ] No clippy warnings in Rust code
- [ ] No ruff warnings in Python code
- [ ] Type hints pass mypy checks
- [ ] Code coverage maintained (or improved)

## Release Checklist

```bash
# 1. Final test run
uv run pytest -v

# 2. Build package
uv build

# 3. Test wheel locally
pip install dist/fraiseql-1.8.1-*.whl

# 4. Create git tag
git tag -a v1.8.1 -m "Release v1.8.1: Restore error field population"
git push origin v1.8.1

# 5. Upload to PyPI (test first)
twine upload --repository-url https://test.pypi.org/legacy/ dist/*

# 6. Verify on test PyPI
pip install --index-url https://test.pypi.org/simple/ fraiseql==1.8.1

# 7. Upload to production PyPI
twine upload dist/*

# 8. Create GitHub release
gh release create v1.8.1 --notes-file docs/releases/v1.8.1-notes.md
```

## DO NOT

- ❌ Add new features unrelated to error field population
- ❌ Change version to v1.9.0 (this is a patch, not minor/major)
- ❌ Skip documentation updates
- ❌ Release without running full test suite
- ❌ Forget to test on test.pypi.org first

## Success Criteria

This phase is complete when:

1. ✅ All tests pass (integration, regression, performance)
2. ✅ Documentation is comprehensive and accurate
3. ✅ CHANGELOG clearly describes the fix
4. ✅ Manual QA validates real-world scenarios
5. ✅ Package is ready for PyPI release
6. ✅ Migration path is clear (even though no migration needed)

## Next Steps

After QA is complete:

1. **Merge to main**: Merge the feature branch
2. **Create release**: Tag v1.8.1 and create GitHub release
3. **Publish**: Upload to PyPI
4. **Announce**: Update docs site, notify users
5. **Monitor**: Watch for issues after release
