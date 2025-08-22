"""Integration tests for ALWAYS_DATA_CONFIG errors array auto-population.

These tests verify that ALWAYS_DATA_CONFIG properly transforms PostgreSQL
function results with error statuses into populated errors arrays in the
final GraphQL response.
"""

import uuid
from typing import Any

import pytest

import fraiseql
from fraiseql import ALWAYS_DATA_CONFIG
from fraiseql.mutations.parser import parse_mutation_result


@fraiseql.type
class ErrorType:
    """Test error type."""

    message: str
    code: int
    identifier: str
    details: dict[str, Any] | None = None


@fraiseql.type
class MutationBase:
    """Base mutation result type."""

    status: str
    message: str | None = None
    errors: list[ErrorType] | None = None


@fraiseql.success
class TestMutationSuccess(MutationBase):
    """Success mutation result."""

    entity_id: str | None = None


@fraiseql.failure
class TestMutationError(MutationBase):
    """Error mutation result."""

    original_data: dict[str, Any] | None = None


class TestAlwaysDataConfigIntegration:
    """Test ALWAYS_DATA_CONFIG integration with GraphQL execution."""

    def test_parser_populates_errors_array(self):
        """Test that mutation parser correctly populates errors array."""
        # GIVEN: A database result with error status
        db_result = {
            "id": str(uuid.uuid4()),
            "updated_fields": [],
            "status": "noop:invalid_entity",
            "message": "Entity not found",
            "object_data": None,
            "extra_metadata": {"reason": "not_found"},
        }

        # WHEN: Parsing with ALWAYS_DATA_CONFIG
        result = parse_mutation_result(
            db_result, TestMutationSuccess, TestMutationError, ALWAYS_DATA_CONFIG
        )

        # THEN: Result should be error type with populated errors array
        assert isinstance(result, TestMutationError)
        assert result.status == "noop:invalid_entity"
        assert result.message == "Entity not found"
        assert result.errors is not None, "Errors array should be auto-populated"
        assert len(result.errors) == 1

        error = result.errors[0]
        assert error.code == 422
        assert error.identifier == "invalid_entity"
        assert error.message == "Entity not found"

    async def test_graphql_execution_preserves_errors_array(self):
        """Test that GraphQL execution preserves auto-populated errors array."""
        from graphql import build_schema, execute, parse

        # Create a minimal schema with our test mutation
        schema_sdl = """
            type Query {
                dummy: String
            }

            type TestMutationSuccess {
                status: String!
                message: String
                entityId: String
                errors: [ErrorType]
            }

            type TestMutationError {
                status: String!
                message: String
                originalData: JSON
                errors: [ErrorType]
            }

            type ErrorType {
                message: String!
                code: Int!
                identifier: String!
                details: JSON
            }

            union TestMutationResult = TestMutationSuccess | TestMutationError

            type Mutation {
                testMutation: TestMutationResult
            }

            scalar JSON
        """

        async def mock_resolver(obj, info):
            # Simulate what PrintOptim's mutation does - call PostgreSQL function
            # that returns error status, then parse with ALWAYS_DATA_CONFIG
            db_result = {
                "id": "96e312d1-f83d-4189-9aec-f6d704c3861e",
                "updated_fields": [],
                "status": "noop:invalid_contract",
                "message": "Contract not found or has been deleted.",
                "object_data": None,
                "extra_metadata": {"reason": "contract_not_found_or_deleted"},
            }

            # Parse using the same logic as PrintOptim
            from fraiseql.mutations.parser import parse_mutation_result

            return parse_mutation_result(
                db_result, TestMutationSuccess, TestMutationError, ALWAYS_DATA_CONFIG
            )

        # Build schema and execute
        schema = build_schema(schema_sdl)

        # Add type resolvers for union
        def resolve_mutation_result_type(obj, info, type_):
            if isinstance(obj, TestMutationError):
                return "TestMutationError"
            if isinstance(obj, TestMutationSuccess):
                return "TestMutationSuccess"
            return None

        schema.get_type("TestMutationResult").resolve_type = resolve_mutation_result_type

        # Add resolver
        schema.get_type("Mutation").fields["testMutation"].resolve = mock_resolver

        mutation = """
            mutation TestMutation {
                testMutation {
                    __typename
                    ... on TestMutationError {
                        status
                        message
                        errors {
                            message
                            code
                            identifier
                        }
                    }
                }
            }
        """

        # Execute the GraphQL mutation
        document = parse(mutation)
        result = await execute(schema, document)

        # Verify successful execution
        assert result.errors is None, "GraphQL execution should succeed"
        assert result.data["testMutation"]["__typename"] == "TestMutationError"
        assert result.data["testMutation"]["status"] == "noop:invalid_contract"

        # Verify error auto-population works
        errors = result.data["testMutation"]["errors"]
        assert errors is not None, "errors array should be auto-populated"
        assert len(errors) == 1, "Should have exactly one error"
        assert errors[0]["code"] == 422
        assert errors[0]["identifier"] == "invalid_contract"

    def test_different_error_prefixes(self):
        """Test ALWAYS_DATA_CONFIG with different error prefixes."""
        test_cases = [
            ("noop:invalid_entity", 422, "invalid_entity"),
            ("blocked:insufficient_permissions", 422, "insufficient_permissions"),
            ("skipped:already_processed", 422, "already_processed"),
            ("ignored:other_reason", 422, "other_reason"),  # Changed to avoid 'duplicate' keyword
        ]

        for status, expected_code, expected_identifier in test_cases:
            db_result = {
                "id": str(uuid.uuid4()),
                "updated_fields": [],
                "status": status,
                "message": f"Test message for {status}",
                "object_data": None,
                "extra_metadata": {},
            }

            result = parse_mutation_result(
                db_result, TestMutationSuccess, TestMutationError, ALWAYS_DATA_CONFIG
            )

            assert isinstance(result, TestMutationError)
            assert result.errors is not None
            assert len(result.errors) == 1

            error = result.errors[0]
            assert error.code == expected_code
            assert error.identifier == expected_identifier

    def test_success_status_no_errors_array(self):
        """Test that success statuses don't get errors array."""
        db_result = {
            "id": str(uuid.uuid4()),
            "updated_fields": ["name"],
            "status": "success",
            "message": "Entity created successfully",
            "object_data": {"id": "entity-123", "name": "Test Entity"},
            "extra_metadata": {},
        }

        result = parse_mutation_result(
            db_result, TestMutationSuccess, TestMutationError, ALWAYS_DATA_CONFIG
        )

        assert isinstance(result, TestMutationSuccess)
        assert result.status == "success"
        assert result.message == "Entity created successfully"
        assert result.errors is None  # Success should not have errors


async def test_mutation_disables_json_passthrough():
    """Test that mutations disable JSON passthrough for error auto-population."""
    from graphql import build_schema

    from fraiseql.graphql.execute import execute_with_passthrough_check

    schema = build_schema("""
        type Query {
            test: String
        }

        type Mutation {
            testMutation: String
        }
    """)

    # Create context with passthrough enabled (simulates production config)
    context = {"json_passthrough": True, "execution_mode": "passthrough"}

    mutation_query = """
        mutation {
            testMutation
        }
    """

    # Execute the mutation
    await execute_with_passthrough_check(
        schema=schema, source=mutation_query, context_value=context
    )

    # Verify that passthrough was automatically disabled for the mutation
    assert not context["json_passthrough"]
    assert context.get("execution_mode") == "standard"

    # This ensures error auto-population will work correctly


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
