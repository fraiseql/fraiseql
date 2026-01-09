"""Tests for Rust schema generator export functionality.

Phase A.1: Schema generation moved to Rust
These tests verify that Rust correctly exports GraphQL filter and orderby schemas.
"""

import json

import pytest

# Import fraiseql_rs through fraiseql to ensure proper initialization
try:
    from fraiseql import fraiseql_rs
except ImportError:
    fraiseql_rs = None


def skip_if_no_rust() -> None:
    """Skip test if fraiseql_rs is not available."""
    if fraiseql_rs is None:
        pytest.skip("fraiseql_rs not available")


class TestRustSchemaExport:
    """Test Rust schema export FFI function."""

    def test_rust_exports_schema_json(self) -> None:
        """GREEN: Rust exports schema as JSON string."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        assert isinstance(schema_json, str)
        schema = json.loads(schema_json)
        assert isinstance(schema, dict)

    def test_schema_contains_filter_schemas(self) -> None:
        """GREEN: Schema includes filter_schemas key."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        assert "filter_schemas" in schema
        assert isinstance(schema["filter_schemas"], dict)

    def test_schema_includes_all_base_types(self) -> None:
        """GREEN: Schema includes filters for all base types."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        filters = schema["filter_schemas"]

        # Expected base filter types
        expected_types = ["String", "Int", "Float", "Boolean", "ID"]
        for type_name in expected_types:
            assert type_name in filters, f"Missing filter for type: {type_name}"

    def test_string_filter_has_all_operators(self) -> None:
        """GREEN: String filter includes all operators."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        string_filter = schema["filter_schemas"]["String"]

        assert "fields" in string_filter
        operators = string_filter["fields"]

        # Expected operators for string filtering
        expected_ops = [
            "eq",
            "neq",
            "contains",
            "icontains",
            "startswith",
            "istartswith",
            "endswith",
            "iendswith",
            "in_",  # Python keyword, use underscore
            "nin",
            "isnull",
        ]

        for op in expected_ops:
            assert op in operators, f"Missing operator '{op}' in String filter"

    def test_int_filter_has_numeric_operators(self) -> None:
        """GREEN: Int filter includes numeric operators."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        int_filter = schema["filter_schemas"]["Int"]

        operators = int_filter["fields"]

        # Numeric operators
        expected_ops = ["eq", "neq", "lt", "lte", "gt", "gte", "in_", "nin"]  # in_ due to Python keyword

        for op in expected_ops:
            assert op in operators, f"Missing operator '{op}' in Int filter"

    def test_schema_contains_order_by_schemas(self) -> None:
        """GREEN: Schema includes order_by_schemas key."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)

        assert "order_by_schemas" in schema
        assert isinstance(schema["order_by_schemas"], dict)

    def test_order_by_has_asc_desc(self) -> None:
        """GREEN: Order by supports ASC and DESC directions."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        order_by = schema["order_by_schemas"]

        assert "directions" in order_by
        directions = order_by["directions"]
        assert "ASC" in directions
        assert "DESC" in directions

    def test_schema_format_is_consistent(self) -> None:
        """GREEN: Schema format is consistent across calls."""
        skip_if_no_rust()
        schema1_json = fraiseql_rs.export_schema_generators()
        schema2_json = fraiseql_rs.export_schema_generators()

        # Should be identical (deterministic)
        assert schema1_json == schema2_json

    def test_schema_version_field_present(self) -> None:
        """GREEN: Schema includes version information."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)

        assert "version" in schema
        assert isinstance(schema["version"], str)


class TestSchemaStructure:
    """Test the structure and correctness of exported schema."""

    def test_filter_field_has_type_and_nullable(self) -> None:
        """GREEN: Each filter field has type and nullable info."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        string_filter = schema["filter_schemas"]["String"]

        # Each operator should have type info
        for op_name, op_def in string_filter["fields"].items():
            assert "type" in op_def, f"Missing 'type' for operator {op_name}"
            assert "nullable" in op_def, f"Missing 'nullable' for operator {op_name}"

    def test_list_type_fields_marked_correctly(self) -> None:
        """GREEN: List type fields are marked as list."""
        skip_if_no_rust()
        schema_json = fraiseql_rs.export_schema_generators()
        schema = json.loads(schema_json)
        string_filter = schema["filter_schemas"]["String"]

        # 'in_' and 'nin' should be list types (in_ due to Python keyword)
        assert "in_" in string_filter["fields"]
        in_type = string_filter["fields"]["in_"]["type"]
        assert "list" in in_type.lower() or "[" in in_type

        assert "nin" in string_filter["fields"]
        nin_type = string_filter["fields"]["nin"]["type"]
        assert "list" in nin_type.lower() or "[" in nin_type
