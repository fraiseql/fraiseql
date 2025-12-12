# Phase 1: RED - Write Failing Tests for Error Field Population

## Objective

Write comprehensive tests that define the expected behavior for error field population from database `entity` and `metadata` fields. All tests should **fail** with the current v1.8.0 implementation.

## Context

**Current State**: FraiseQL v1.8.0 only populates 5 hardcoded fields in error responses:
- `__typename`
- `message`
- `status`
- `code`
- `errors`

**Target State**: Error responses should populate custom fields from database `entity`/`metadata`, matching v1.7.1 behavior.

**Reference**: Success responses (response_builder.rs:45-188) already implement this pattern correctly.

## Files to Create/Modify

### New Test File
- `tests/integration/mutations/test_error_field_population.py`

## Implementation Steps

### Step 1: Create Base Test Structure

Create test file with fixtures and helper types:

```python
"""Test error field population from database entity/metadata fields.

This test suite verifies that custom error class fields are automatically
populated from database mutation_response.entity and .metadata fields.

Regression test for v1.8.0 where this functionality was lost during
the Rust pipeline rewrite.
"""

import pytest
import pytest_asyncio
from uuid import UUID, uuid4

import fraiseql
from fraiseql import type, success, failure, mutation, input
from fraiseql.gql.schema_builder import build_fraiseql_schema

pytestmark = pytest.mark.integration


# Test types
@type
class DnsServer:
    """Network DNS server entity."""
    id: str
    ip_address: str
    hostname: str | None = None


@type
class ValidationError:
    """Validation error detail."""
    field: str
    message: str
    code: str


@input
class CreateDnsServerInput:
    ip_address: str
    hostname: str | None = None


@success
class CreateDnsServerSuccess:
    message: str
    dns_server: DnsServer


@failure
class CreateDnsServerError:
    """Error with custom fields from database."""
    message: str
    conflict_dns_server: DnsServer | None = None  # Custom field from entity
    validation_errors: list[ValidationError] | None = None  # Custom field from metadata
    conflict_count: int | None = None  # Custom scalar field


@mutation(function="create_dns_server", schema="app")
class CreateDnsServer:
    input: CreateDnsServerInput
    success: CreateDnsServerSuccess
    failure: CreateDnsServerError


class TestErrorFieldPopulation:
    """Test custom error field population from database entity/metadata."""
```

### Step 2: Add Database Setup Fixture

```python
    @pytest_asyncio.fixture
    async def setup_error_field_test_db(self, db_connection):
        """Set up database with functions that return errors with custom fields."""
        async with db_connection.cursor() as cur:
            # Create app schema if not exists
            await cur.execute("CREATE SCHEMA IF NOT EXISTS app")

            # Ensure mutation_response type exists
            await cur.execute("""
                DO $$ BEGIN
                    CREATE TYPE app.mutation_response AS (
                        status TEXT,
                        message TEXT,
                        entity_id UUID,
                        entity_type TEXT,
                        entity JSONB,
                        updated_fields TEXT[],
                        cascade JSONB,
                        metadata JSONB
                    );
                EXCEPTION
                    WHEN duplicate_object THEN null;
                END $$;
            """)

            # Function that returns conflict in entity field
            await cur.execute("""
                CREATE OR REPLACE FUNCTION app.create_dns_server(input_data JSONB)
                RETURNS app.mutation_response AS $$
                DECLARE
                    v_ip TEXT;
                    v_existing_id UUID := '123e4567-e89b-12d3-a456-426614174000'::UUID;
                BEGIN
                    v_ip := input_data->>'ip_address';

                    -- Simulate conflict scenario
                    IF v_ip = '192.168.1.1' THEN
                        RETURN ROW(
                            'failed:conflict',
                            'DNS Server with this IP already exists',
                            v_existing_id,
                            'DnsServer',
                            jsonb_build_object(
                                'conflict_dns_server', jsonb_build_object(
                                    'id', v_existing_id::TEXT,
                                    'ip_address', '192.168.1.1',
                                    'hostname', 'existing-dns.local'
                                ),
                                'conflict_count', 1
                            ),
                            NULL,
                            NULL,
                            NULL
                        )::app.mutation_response;
                    END IF;

                    -- Simulate validation error with metadata
                    IF v_ip = 'invalid' THEN
                        RETURN ROW(
                            'failed:validation',
                            'Validation failed',
                            NULL,
                            NULL,
                            NULL,
                            NULL,
                            NULL,
                            jsonb_build_object(
                                'validation_errors', jsonb_build_array(
                                    jsonb_build_object(
                                        'field', 'ip_address',
                                        'message', 'Invalid IP address format',
                                        'code', 'INVALID_FORMAT'
                                    )
                                )
                            )
                        )::app.mutation_response;
                    END IF;

                    -- Success case
                    RETURN ROW(
                        'success',
                        'DNS Server created',
                        gen_random_uuid(),
                        'DnsServer',
                        jsonb_build_object(
                            'dns_server', jsonb_build_object(
                                'id', gen_random_uuid()::TEXT,
                                'ip_address', v_ip,
                                'hostname', input_data->>'hostname'
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
        """Build GraphQL schema with error field population mutation."""
        @fraiseql.type
        class Query:
            dummy: str = "test"

        return build_fraiseql_schema(
            query_types=[Query],
            mutation_resolvers=[CreateDnsServer],
            camel_case_fields=True
        )
```

### Step 3: Test Core Error Field Population

```python
    @pytest.mark.asyncio
    async def test_error_field_from_entity_object(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that custom object fields are populated from entity.

        RED: This should fail - conflict_dns_server will be null.
        """
        from graphql import execute, parse

        mutation = """
            mutation CreateDnsServer($input: CreateDnsServerInput!) {
                createDnsServer(input: $input) {
                    __typename
                    ... on CreateDnsServerError {
                        message
                        status
                        code
                        conflictDnsServer {
                            id
                            ipAddress
                            hostname
                        }
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"ipAddress": "192.168.1.1"}}
        )

        assert result.errors is None, f"GraphQL errors: {result.errors}"

        error = result.data["createDnsServer"]
        assert error["__typename"] == "CreateDnsServerError"
        assert error["message"] == "DNS Server with this IP already exists"
        assert error["status"] == "failed:conflict"
        assert error["code"] == 409

        # RED: This assertion will FAIL - conflictDnsServer is currently null
        assert error["conflictDnsServer"] is not None, (
            "conflict_dns_server from entity field should be populated"
        )
        assert error["conflictDnsServer"]["id"] == "123e4567-e89b-12d3-a456-426614174000"
        assert error["conflictDnsServer"]["ipAddress"] == "192.168.1.1"
        assert error["conflictDnsServer"]["hostname"] == "existing-dns.local"
```

### Step 4: Test Scalar Field Population

```python
    @pytest.mark.asyncio
    async def test_error_scalar_field_from_entity(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that custom scalar fields are populated from entity.

        RED: This should fail - conflict_count will be null.
        """
        from graphql import execute, parse

        mutation = """
            mutation CreateDnsServer($input: CreateDnsServerInput!) {
                createDnsServer(input: $input) {
                    ... on CreateDnsServerError {
                        conflictCount
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"ipAddress": "192.168.1.1"}}
        )

        error = result.data["createDnsServer"]

        # RED: This will FAIL - conflictCount is null
        assert error["conflictCount"] == 1, (
            "conflict_count scalar from entity should be populated"
        )
```

### Step 5: Test Field Population from Metadata

```python
    @pytest.mark.asyncio
    async def test_error_field_from_metadata(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that custom fields are populated from metadata as fallback.

        RED: This should fail - validation_errors will be null.
        """
        from graphql import execute, parse

        mutation = """
            mutation CreateDnsServer($input: CreateDnsServerInput!) {
                createDnsServer(input: $input) {
                    ... on CreateDnsServerError {
                        message
                        validationErrors {
                            field
                            message
                            code
                        }
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"ipAddress": "invalid"}}
        )

        error = result.data["createDnsServer"]

        # RED: This will FAIL - validationErrors is null
        assert error["validationErrors"] is not None, (
            "validation_errors from metadata should be populated"
        )
        assert len(error["validationErrors"]) == 1
        assert error["validationErrors"][0]["field"] == "ip_address"
        assert error["validationErrors"][0]["code"] == "INVALID_FORMAT"
```

### Step 6: Test CamelCase Transformation

```python
    @pytest.mark.asyncio
    async def test_error_field_camelcase_transformation(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that snake_case database fields are transformed to camelCase.

        RED: This will fail due to field not being populated at all.
        """
        from graphql import execute, parse

        mutation = """
            mutation CreateDnsServer($input: CreateDnsServerInput!) {
                createDnsServer(input: $input) {
                    ... on CreateDnsServerError {
                        conflictDnsServer {
                            ipAddress  # snake_case -> camelCase
                        }
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"ipAddress": "192.168.1.1"}}
        )

        error = result.data["createDnsServer"]

        # RED: Field is null, so camelCase isn't even tested yet
        assert error["conflictDnsServer"] is not None
        assert "ipAddress" in error["conflictDnsServer"], (
            "Field keys should be transformed to camelCase"
        )
```

### Step 7: Test Nested Entity __typename Addition

```python
    @pytest.mark.asyncio
    async def test_error_nested_entity_typename(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that nested entities get __typename added.

        RED: This will fail - no __typename in nested object.
        """
        from graphql import execute, parse

        mutation = """
            mutation CreateDnsServer($input: CreateDnsServerInput!) {
                createDnsServer(input: $input) {
                    ... on CreateDnsServerError {
                        conflictDnsServer {
                            __typename
                            id
                        }
                    }
                }
            }
        """

        result = await execute(
            graphql_schema,
            parse(mutation),
            variable_values={"input": {"ipAddress": "192.168.1.1"}}
        )

        error = result.data["createDnsServer"]

        # RED: conflictDnsServer is null, so __typename doesn't exist
        assert error["conflictDnsServer"] is not None
        assert error["conflictDnsServer"]["__typename"] == "DnsServer", (
            "Nested entities should have __typename for GraphQL union resolution"
        )
```

### Step 8: Test Reserved Fields Are Not Overridden

```python
    @pytest.mark.asyncio
    async def test_error_reserved_fields_not_overridden(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that reserved fields (message, status, code, errors) are not overridden.

        This should pass even in RED phase - verifies we don't break existing behavior.
        """
        from graphql import execute, parse

        mutation = """
            mutation CreateDnsServer($input: CreateDnsServerInput!) {
                createDnsServer(input: $input) {
                    ... on CreateDnsServerError {
                        __typename
                        message
                        status
                        code
                        errors {
                            field
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
            variable_values={"input": {"ipAddress": "192.168.1.1"}}
        )

        error = result.data["createDnsServer"]

        # These should already work in v1.8.0
        assert error["__typename"] == "CreateDnsServerError"
        assert error["message"] == "DNS Server with this IP already exists"
        assert error["status"] == "failed:conflict"
        assert error["code"] == 409
        assert isinstance(error["errors"], list)
```

## Verification Commands

### Run Tests (Expect Failures)

```bash
# Run all error field population tests
uv run pytest tests/integration/mutations/test_error_field_population.py -v

# Expected output:
# test_error_field_from_entity_object FAILED - conflictDnsServer is None
# test_error_scalar_field_from_entity FAILED - conflictCount is None
# test_error_field_from_metadata FAILED - validationErrors is None
# test_error_field_camelcase_transformation FAILED - conflictDnsServer is None
# test_error_nested_entity_typename FAILED - conflictDnsServer is None
# test_error_reserved_fields_not_overridden PASSED - existing behavior works
```

### Verify Test Structure

```bash
# Ensure tests are properly structured
uv run pytest tests/integration/mutations/test_error_field_population.py --collect-only

# Expected: 6 tests collected
```

## Acceptance Criteria

- [ ] Test file created at `tests/integration/mutations/test_error_field_population.py`
- [ ] All 6 test functions defined with proper docstrings
- [ ] Database fixture creates functions with entity/metadata error fields
- [ ] Tests cover:
  - [ ] Object field population from entity
  - [ ] Scalar field population from entity
  - [ ] Field population from metadata
  - [ ] CamelCase transformation
  - [ ] Nested entity __typename
  - [ ] Reserved fields not overridden
- [ ] Running tests produces **5 failures, 1 pass** (reserved fields test passes)
- [ ] Test output clearly shows what's missing (assertions with helpful messages)

## DO NOT

- ❌ Implement any Rust code in this phase
- ❌ Modify `response_builder.rs` yet
- ❌ Make tests pass - they should fail
- ❌ Skip test cases to make them pass faster
- ❌ Use mocks instead of real database functions

## Next Phase

After all tests are written and failing (RED), proceed to **Phase 2: GREEN** to implement the Rust functionality that makes tests pass.
