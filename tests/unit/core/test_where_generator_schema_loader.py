"""Tests for graphql_where_generator using schema_loader.

Phase A.3: WHERE Generator Schema Optimization
These tests verify that graphql_where_generator can use pre-built Rust schemas
instead of generating filter types at runtime.
"""


import pytest

try:
    from fraiseql import fraiseql_rs
except ImportError:
    fraiseql_rs = None


def skip_if_no_rust() -> None:
    """Skip test if fraiseql_rs is not available."""
    if fraiseql_rs is None:
        pytest.skip("fraiseql_rs not available")


class TestWhereGeneratorSchemaLoaderIntegration:
    """Test WHERE generator integration with schema_loader."""

    def test_where_generator_can_use_schema_loader(self) -> None:
        """RED: WHERE generator can access loaded schema for filter info."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema

        # Load schema from Rust
        schema = load_schema()

        # Verify we have filter schemas
        assert "filter_schemas" in schema
        assert len(schema["filter_schemas"]) > 0

        # Verify String filter schema is present and complete
        string_schema = schema["filter_schemas"]["String"]
        assert "fields" in string_schema
        assert len(string_schema["fields"]) > 0

    def test_where_generator_string_filter_operators_match_schema(self) -> None:
        """RED: String filter operators from schema match where_generator definition."""
        skip_if_no_rust()
        from typing import get_type_hints

        from fraiseql.gql.schema_loader import get_filter_operators
        from fraiseql.sql.graphql_where_generator import StringFilter

        # Get operators from schema loader
        schema_operators = get_filter_operators("String")
        operator_names = set(schema_operators.keys())

        # Get operators from StringFilter class
        type_hints = get_type_hints(StringFilter)
        class_operators = set(type_hints.keys())

        # Schema operators should be a subset of or equal to class operators
        # (schema might be subset for stability)
        assert len(operator_names) > 0
        assert len(class_operators) > 0
        # All schema operators should exist in the class
        for op in operator_names:
            assert op in class_operators, f"Schema has '{op}' but not in StringFilter"

    def test_where_generator_int_filter_operators_match_schema(self) -> None:
        """RED: Int filter operators from schema match where_generator definition."""
        skip_if_no_rust()
        from typing import get_type_hints

        from fraiseql.gql.schema_loader import get_filter_operators
        from fraiseql.sql.graphql_where_generator import IntFilter

        schema_operators = get_filter_operators("Int")
        operator_names = set(schema_operators.keys())

        type_hints = get_type_hints(IntFilter)
        class_operators = set(type_hints.keys())

        assert len(operator_names) > 0
        for op in operator_names:
            assert op in class_operators, f"Schema has '{op}' but not in IntFilter"

    def test_where_generator_float_filter_operators_match_schema(self) -> None:
        """RED: Float filter operators from schema match where_generator definition."""
        skip_if_no_rust()
        from typing import get_type_hints

        from fraiseql.gql.schema_loader import get_filter_operators
        from fraiseql.sql.graphql_where_generator import FloatFilter

        schema_operators = get_filter_operators("Float")
        operator_names = set(schema_operators.keys())

        type_hints = get_type_hints(FloatFilter)
        class_operators = set(type_hints.keys())

        assert len(operator_names) > 0
        for op in operator_names:
            assert op in class_operators, f"Schema has '{op}' but not in FloatFilter"

    def test_where_generator_array_filter_operators_match_schema(self) -> None:
        """RED: Array filter operators from schema match where_generator definition."""
        skip_if_no_rust()
        from typing import get_type_hints

        from fraiseql.gql.schema_loader import get_filter_operators
        from fraiseql.sql.graphql_where_generator import ArrayFilter

        schema_operators = get_filter_operators("Array")
        operator_names = set(schema_operators.keys())

        type_hints = get_type_hints(ArrayFilter)
        class_operators = set(type_hints.keys())

        assert len(operator_names) > 0
        for op in operator_names:
            assert op in class_operators, f"Schema has '{op}' but not in ArrayFilter"

    def test_all_base_filter_types_available_in_schema_and_generator(self) -> None:
        """RED: All base filter types available in both schema and generator."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema
        from fraiseql.sql import graphql_where_generator

        schema = load_schema()
        schema_types = set(schema["filter_schemas"].keys())

        # These should always exist
        expected_types = {"String", "Int", "Float", "Boolean", "ID", "Decimal", "Date", "DateTime", "UUID"}

        for type_name in expected_types:
            # Should be in schema
            assert type_name in schema_types, f"{type_name} not in Rust schema"

            # Should have corresponding Python class
            filter_class_name = f"{type_name}Filter"
            assert hasattr(graphql_where_generator, filter_class_name), (
                f"{filter_class_name} not found in where_generator"
            )

    def test_schema_operator_types_are_valid(self) -> None:
        """RED: Schema operator definitions have required fields."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import get_filter_operators

        # Check String operators have type and nullable
        ops = get_filter_operators("String")
        for op_name, op_def in ops.items():
            assert "type" in op_def, f"Operator {op_name} missing 'type' field"
            assert "nullable" in op_def, f"Operator {op_name} missing 'nullable' field"


class TestWhereGeneratorWithCachedSchema:
    """Test WHERE generator performance with cached schema."""

    def test_schema_caching_improves_performance(self) -> None:
        """RED: Schema caching provides fast repeated access."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import _get_cached_schema, load_schema

        # First load
        schema1 = load_schema()

        # Second load (cached) - should return same object
        schema2 = load_schema()

        # Both should be identical objects (same memory reference)
        assert schema1 is schema2, "Cached schema should be same object"

        # Direct cache access should also return same object
        schema3 = _get_cached_schema()
        assert schema3 is schema1, "Direct cache access should return same cached object"
