"""Unit tests for complex OrderBy scenarios in FraiseQL.

This test suite focuses on real-world GraphQL OrderBy transformations
that go beyond basic single-field sorting, including nested fields,
multiple orderings, and mixed formats.

These are pure unit tests that test input → SQL transformation without
requiring database connections.
"""

import pytest

from fraiseql.core.ast_parser import FieldPath
from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql
from fraiseql.sql.sql_generator import build_sql_query

# Mark all tests in this module as unit tests (no database required)
pytestmark = pytest.mark.unit


class TestComplexOrderByScenarios:
    """Test complex OrderBy scenarios that mirror real-world GraphQL usage."""

    def test_multiple_field_orderby_conversion(self):
        """Test conversion of multiple field OrderBy from GraphQL format."""
        # Typical multi-field sorting from GraphQL client
        graphql_input = [{"createdAt": "DESC"}, {"name": "ASC"}, {"status": "ASC"}]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 3
        assert result.instructions[0].field == "created_at"
        assert result.instructions[0].direction == "desc"
        assert result.instructions[1].field == "name"
        assert result.instructions[1].direction == "asc"
        assert result.instructions[2].field == "status"
        assert result.instructions[2].direction == "asc"

    def test_nested_field_orderby_conversion(self):
        """Test conversion of nested field OrderBy with dot notation."""
        # Sorting by nested object properties
        graphql_input = [
            {"profile.firstName": "ASC"},
            {"profile.lastName": "ASC"},
            {"createdAt": "DESC"},
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 3
        assert result.instructions[0].field == "profile.first_name"
        assert result.instructions[0].direction == "asc"
        assert result.instructions[1].field == "profile.last_name"
        assert result.instructions[1].direction == "asc"
        assert result.instructions[2].field == "created_at"
        assert result.instructions[2].direction == "desc"

    def test_camelcase_heavy_orderby_conversion(self):
        """Test conversion with heavy camelCase field names (realistic GraphQL)."""
        # Real-world GraphQL with camelCase field names
        graphql_input = [
            {"ipAddress": "ASC"},
            {"lastConnectedAt": "DESC"},
            {"organizationName": "ASC"},
            {"isActive": "DESC"},
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 4
        assert result.instructions[0].field == "ip_address"
        assert result.instructions[0].direction == "asc"
        assert result.instructions[1].field == "last_connected_at"
        assert result.instructions[1].direction == "desc"
        assert result.instructions[2].field == "organization_name"
        assert result.instructions[2].direction == "asc"
        assert result.instructions[3].field == "is_active"
        assert result.instructions[3].direction == "desc"

    def test_mixed_format_orderby_conversion(self):
        """Test conversion of mixed OrderBy formats (single dict with multiple fields + separate dicts)."""
        # Single dict with multiple fields + separate dicts
        graphql_input = [
            {"priority": "DESC", "urgency": "DESC"},  # Multiple in one dict
            {"assignedTo": "ASC"},  # Separate dict
            {"dueDate": "ASC"},  # Another separate dict
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 4

        # First dict should produce multiple instructions
        fields = [instr.field for instr in result.instructions]
        directions = [instr.direction for instr in result.instructions]

        assert "priority" in fields
        assert "urgency" in fields
        assert "assigned_to" in fields
        assert "due_date" in fields

        # Check specific mappings
        priority_idx = fields.index("priority")
        urgency_idx = fields.index("urgency")
        assigned_to_idx = fields.index("assigned_to")
        due_date_idx = fields.index("due_date")

        assert directions[priority_idx] == "desc"
        assert directions[urgency_idx] == "desc"
        assert directions[assigned_to_idx] == "asc"
        assert directions[due_date_idx] == "asc"

    def test_deep_nested_orderby_conversion(self):
        """Test conversion of deeply nested field OrderBy."""
        # Complex nested object sorting
        graphql_input = [
            {"user.profile.address.city": "ASC"},
            {"user.profile.firstName": "ASC"},
            {"organization.settings.timezone": "ASC"},
            {"createdAt": "DESC"},
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 4
        assert result.instructions[0].field == "user.profile.address.city"
        assert result.instructions[0].direction == "asc"
        assert result.instructions[1].field == "user.profile.first_name"
        assert result.instructions[1].direction == "asc"
        assert result.instructions[2].field == "organization.settings.timezone"
        assert result.instructions[2].direction == "asc"
        assert result.instructions[3].field == "created_at"
        assert result.instructions[3].direction == "desc"

    def test_multiple_field_sql_generation(self):
        """Test SQL generation for multiple field OrderBy - unit test only."""
        field_paths = [
            FieldPath(path=["id"], alias="id"),
            FieldPath(path=["name"], alias="name"),
            FieldPath(path=["created_at"], alias="createdAt"),
        ]

        # Multiple field ordering
        order_by = [("created_at", "desc"), ("name", "asc"), ("status", "asc")]

        sql_query = build_sql_query(
            table="test_table",
            field_paths=field_paths,
            order_by=order_by,
            json_output=True,
            auto_camel_case=True,
        )

        # Convert to string for assertion - this doesn't execute SQL
        sql_str = str(sql_query)
        assert "ORDER BY" in sql_str
        assert "created_at" in sql_str
        assert "DESC" in sql_str
        assert "name" in sql_str
        assert "ASC" in sql_str

    def test_nested_field_sql_generation(self):
        """Test SQL generation for nested field OrderBy with JSONB path operators."""
        field_paths = [
            FieldPath(path=["id"], alias="id"),
            FieldPath(path=["profile", "first_name"], alias="profile.firstName"),
        ]

        # Nested field ordering
        order_by = [("profile.first_name", "asc"), ("created_at", "desc")]

        sql_query = build_sql_query(
            table="test_table",
            field_paths=field_paths,
            order_by=order_by,
            json_output=True,
            auto_camel_case=True,
        )

        sql_str = str(sql_query)
        assert "ORDER BY" in sql_str
        # Should use JSONB path operators for nested fields
        assert "profile" in sql_str
        assert "first_name" in sql_str

    def test_deep_nested_sql_generation(self):
        """Test SQL generation for deeply nested fields with multiple path levels."""
        field_paths = [FieldPath(path=["id"], alias="id")]

        # Deep nested field ordering
        order_by = [("user.profile.address.city", "asc")]

        sql_query = build_sql_query(
            table="test_table",
            field_paths=field_paths,
            order_by=order_by,
            json_output=True,
            auto_camel_case=True,
        )

        sql_str = str(sql_query)
        assert "ORDER BY" in sql_str
        # Should handle deep nesting with multiple -> operators
        assert "user" in sql_str
        assert "profile" in sql_str
        assert "address" in sql_str
        assert "city" in sql_str

    def test_printoptim_backend_dns_servers_scenario(self):
        """Test the exact scenario from PrintOptim Backend DNS servers."""
        # This is the real-world case that was failing
        graphql_input = [
            {"ipAddress": "ASC"},
            {"organizationName": "ASC"},
            {"lastConnectedAt": "DESC"},
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 3

        # Verify exact field transformations
        assert result.instructions[0].field == "ip_address"
        assert result.instructions[0].direction == "asc"
        assert result.instructions[1].field == "organization_name"
        assert result.instructions[1].direction == "asc"
        assert result.instructions[2].field == "last_connected_at"
        assert result.instructions[2].direction == "desc"

        # Test that these can be safely unpacked (the original error case)
        tuples = [(instr.field, instr.direction) for instr in result.instructions]

        # This should not raise "not enough values to unpack" error
        for field, direction in tuples:
            assert isinstance(field, str)
            assert isinstance(direction, str)
            assert direction in ["asc", "desc"]

    def test_enterprise_contract_management_scenario(self):
        """Test a complex enterprise scenario with multiple business entity sorting."""
        # Complex enterprise GraphQL query
        graphql_input = [
            {"contract.priority": "DESC"},
            {"contract.client.organizationName": "ASC"},
            {"assignedTo.profile.lastName": "ASC"},
            {"dueDate": "ASC"},
            {"createdAt": "DESC"},
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 5

        # Verify complex field transformations
        fields = [instr.field for instr in result.instructions]

        assert "contract.priority" in fields
        assert "contract.client.organization_name" in fields
        assert "assigned_to.profile.last_name" in fields
        assert "due_date" in fields
        assert "created_at" in fields

    def test_mixed_case_directions_complex(self):
        """Test complex scenario with mixed case directions (real GraphQL client behavior)."""
        # Real GraphQL clients might send mixed case
        graphql_input = [
            {"priority": "DESC"},
            {"name": "asc"},  # lowercase
            {"updatedAt": "DESC"},  # uppercase
            {"status": "Asc"},  # mixed case - should be normalized
        ]

        result = _convert_order_by_input_to_sql(graphql_input)

        assert result is not None
        assert len(result.instructions) == 4

        # All directions should be normalized to lowercase
        directions = [instr.direction for instr in result.instructions]
        assert all(d in ["asc", "desc"] for d in directions)
        assert directions[0] == "desc"  # DESC -> desc
        assert directions[1] == "asc"  # asc -> asc
        assert directions[2] == "desc"  # DESC -> desc
        assert directions[3] == "asc"  # Asc -> asc (if handled)

    def test_integration_graphql_to_sql_complex(self):
        """Integration test: Complete GraphQL OrderBy → SQL transformation for complex scenario."""
        # Full integration test
        graphql_input = [
            {"user.profile.firstName": "ASC"},
            {"organization.settings.priority": "DESC"},
            {"lastModifiedAt": "DESC"},
        ]

        # Step 1: Convert GraphQL input
        order_by_set = _convert_order_by_input_to_sql(graphql_input)
        assert order_by_set is not None

        # Step 2: Convert to tuples for SQL generator
        tuples = [(instr.field, instr.direction) for instr in order_by_set.instructions]

        # Step 3: Generate SQL
        field_paths = [
            FieldPath(path=["id"], alias="id"),
            FieldPath(path=["user", "profile", "first_name"], alias="user.profile.firstName"),
        ]

        sql_query = build_sql_query(
            table="complex_test_table",
            field_paths=field_paths,
            order_by=tuples,
            json_output=True,
            auto_camel_case=True,
        )

        sql_str = str(sql_query)

        # Verify complete transformation
        assert "ORDER BY" in sql_str
        assert "complex_test_table" in sql_str
        assert "user" in sql_str and "profile" in sql_str and "first_name" in sql_str
        assert "organization" in sql_str and "settings" in sql_str
        assert "last_modified_at" in sql_str

    def test_error_recovery_complex_scenarios(self):
        """Test that complex scenarios handle errors gracefully."""
        # Test various error conditions with complex inputs
        error_cases = [
            # Invalid nested structure
            [{"field": {"invalid": "nested"}}],
            # Mixed valid and invalid
            [{"validField": "ASC"}, {"invalidField": {"nested": "invalid"}}],
            # Empty nested field
            [{"": "ASC"}],
            # None values in complex structure
            [{"field1": "ASC"}, None, {"field2": "DESC"}],
        ]

        for case in error_cases:
            # Should not raise exceptions, should handle gracefully
            result = _convert_order_by_input_to_sql(case)
            # Either returns valid partial result or None, but doesn't crash
            if result is not None:
                # If we get a result, it should be valid
                assert hasattr(result, "instructions")
                for instr in result.instructions:
                    assert hasattr(instr, "field")
                    assert hasattr(instr, "direction")
                    assert instr.direction in ["asc", "desc"]
