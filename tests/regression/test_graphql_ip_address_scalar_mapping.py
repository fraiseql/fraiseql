"""Regression test for GitHub issue #37: IpAddress GraphQL scalar mapping.

This test ensures that the IpAddress Python type correctly maps to IpAddressString
in GraphQL schema generation and that field name conversion works properly.

GitHub Issue: https://github.com/printoptim/printoptim_backend/issues/37

Problem: Frontend team reported inconsistent GraphQL type validation errors:
- "Variable of type 'IpAddressString!' used in position expecting type 'String'"
- "Variable of type 'String!' used in position expecting type 'IpAddressString'"

Root Cause: Potential mismapping of Python IpAddress type to GraphQL scalar.

Expected Behavior:
1. Python `IpAddress` type should map to GraphQL `IpAddressString` scalar
2. Field names should convert from snake_case to camelCase
3. GraphQL validation should accept IpAddressString variables
4. GraphQL validation should reject String variables for IpAddress fields
"""

import pytest
from graphql import parse, print_schema, validate

import fraiseql
from fraiseql.gql.builders.registry import SchemaRegistry
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.types import IpAddress


@fraiseql.input
class CreateDnsServerInput:
    """Input type with IpAddress field to test scalar mapping."""

    identifier: str
    ip_address: IpAddress  # Should map to ipAddress: IpAddressString in GraphQL


@fraiseql.success
class CreateDnsServerSuccess:
    """Success response for DNS server creation."""

    message: str = "DNS server created successfully"


@fraiseql.error
class CreateDnsServerError:
    """Error response for DNS server creation."""

    message: str


@fraiseql.mutation(
    function="create_dns_server",
    context_params={},
    error_config=fraiseql.DEFAULT_ERROR_CONFIG,
)
class CreateDnsServer:
    """Mutation to test IpAddress scalar mapping in GraphQL schema."""

    input: CreateDnsServerInput
    success: CreateDnsServerSuccess
    error: CreateDnsServerError


@fraiseql.query
async def health_check(info) -> str:
    """Required query for valid GraphQL schema."""
    return "OK"


@pytest.fixture(autouse=True)
def clear_schema_registry():
    """Clear the schema registry before and after each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


def test_ip_address_scalar_mapping() -> None:
    """Test that IpAddress Python type maps correctly to IpAddressString GraphQL scalar."""
    # Build schema with the test types
    schema = build_fraiseql_schema(
        query_types=[
            CreateDnsServerInput,
            health_check,
        ],
        mutation_resolvers=[CreateDnsServer],
        camel_case_fields=True,
    )

    # Get schema SDL
    schema_sdl = print_schema(schema)

    # Verify IpAddressString scalar is present
    assert "scalar IpAddressString" in schema_sdl, "IpAddressString scalar missing from schema"

    # Verify CreateDnsServerInput is present
    assert "input CreateDnsServerInput" in schema_sdl, "CreateDnsServerInput missing from schema"

    # Extract CreateDnsServerInput definition
    lines = schema_sdl.split("\n")
    input_definition = []
    in_input = False

    for line in lines:
        if "input CreateDnsServerInput" in line:
            in_input = True
            input_definition.append(line)
        elif in_input and line.strip() == "}":
            input_definition.append(line)
            break
        elif in_input:
            input_definition.append(line)

    input_text = "\n".join(input_definition)

    # Test field name conversion: ip_address → ipAddress
    assert "ipAddress:" in input_text, "Field name not converted to camelCase"
    assert "ip_address:" not in input_text, "Field still in snake_case"

    # Test scalar type mapping: IpAddress → IpAddressString
    assert "ipAddress: IpAddressString" in input_text, "Field not mapped to IpAddressString scalar"
    assert "ipAddress: String" not in input_text, "Field incorrectly mapped to String"


def test_graphql_validation_with_ip_address_scalar() -> None:
    """Test that GraphQL validation correctly handles IpAddressString variables."""
    # Build schema
    schema = build_fraiseql_schema(
        query_types=[
            CreateDnsServerInput,
            health_check,
        ],
        mutation_resolvers=[CreateDnsServer],
        camel_case_fields=True,
    )

    # Query with IpAddressString variable (should work)
    valid_query = """
    mutation CreateDnsServer($ipAddress: IpAddressString!) {
        createDnsServer(input: { identifier: "test-server", ipAddress: $ipAddress }) {
            ... on CreateDnsServerSuccess {
                message
            }
            ... on CreateDnsServerError {
                message
            }
        }
    }
    """

    # Query with String variable (should fail)
    invalid_query = """
    mutation CreateDnsServer($ipAddress: String!) {
        createDnsServer(input: { identifier: "test-server", ipAddress: $ipAddress }) {
            ... on CreateDnsServerSuccess {
                message
            }
            ... on CreateDnsServerError {
                message
            }
        }
    }
    """

    # Validate correct query (should pass)
    valid_document = parse(valid_query)
    valid_errors = validate(schema, valid_document)
    assert not valid_errors, f"Valid query failed validation: {valid_errors}"

    # Validate incorrect query (should fail)
    invalid_document = parse(invalid_query)
    invalid_errors = validate(schema, invalid_document)
    assert invalid_errors, "Invalid query passed validation when it should have failed"

    # Check that the error message is about type mismatch
    error_message = str(invalid_errors[0])
    assert "String!" in error_message, "Error should mention String type"
    assert "IpAddressString" in error_message, "Error should mention IpAddressString type"


def test_ip_address_field_type_mapping() -> None:
    """Test that IpAddressField correctly maps to IpAddressScalar."""
    from fraiseql.types.scalars.graphql_utils import convert_scalar_to_graphql
    from fraiseql.types.scalars.ip_address import IpAddressField, IpAddressScalar

    # Test direct type mapping
    mapped_scalar = convert_scalar_to_graphql(IpAddressField)
    assert mapped_scalar == IpAddressScalar, "IpAddressField not mapped to IpAddressScalar"
    assert mapped_scalar.name == "IpAddressString", "Scalar name incorrect"


def test_multiple_ip_address_field_name_conversions() -> None:
    """Test that various snake_case IP address field names convert correctly to camelCase."""

    @fraiseql.input
    class ServerConfigInput:
        """Test input with multiple IP address fields."""

        ip_address: IpAddress
        server_ip_address: IpAddress
        dns_server_ip: IpAddress

    @fraiseql.success
    class ServerConfigSuccess:
        message: str = "Server configured successfully"

    @fraiseql.error
    class ServerConfigError:
        message: str

    @fraiseql.mutation(
        function="configure_server",
        context_params={},
        error_config=fraiseql.DEFAULT_ERROR_CONFIG,
    )
    class ConfigureServer:
        """Mutation to test multiple IP address field name conversions."""

        input: ServerConfigInput
        success: ServerConfigSuccess
        error: ServerConfigError

    @fraiseql.query
    @pytest.mark.asyncio
    async def test_query(info) -> str:
        return "test"

    # Build schema
    schema = build_fraiseql_schema(
        query_types=[ServerConfigInput, test_query],
        mutation_resolvers=[ConfigureServer],
        camel_case_fields=True,
    )

    schema_sdl = print_schema(schema)

    # Test field name conversions
    test_cases = [
        ("ip_address", "ipAddress"),
        ("server_ip_address", "serverIpAddress"),
        ("dns_server_ip", "dnsServerIp"),
    ]

    for snake_case, camel_case in test_cases:
        # Check that the expected GraphQL field name is present
        assert f"{camel_case}: IpAddressString" in schema_sdl, (
            f"Field {snake_case} not converted to {camel_case}"
        )

        # Check that the original snake_case name is not present (except in comments)
        schema_lines = [line for line in schema_sdl.split("\n") if not line.strip().startswith("#")]
        schema_without_comments = "\n".join(schema_lines)
        assert f"{snake_case}:" not in schema_without_comments, (
            f"Original snake_case field {snake_case} still present in schema"
        )


if __name__ == "__main__":
    # Run tests manually for development
    test_ip_address_scalar_mapping()
    test_graphql_validation_with_ip_address_scalar()
    test_ip_address_field_type_mapping()
    print("✅ All regression tests passed!")
