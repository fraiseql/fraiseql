"""Tests for Python schema loader that imports Rust-exported schemas.

Phase A.2: Schema Import and Caching
These tests verify that Python can load and cache schemas exported from Rust,
eliminating the need for runtime schema generation.
"""

import json
import pytest

# Import fraiseql_rs through fraiseql to ensure proper initialization
try:
    from fraiseql import fraiseql_rs
except ImportError:
    fraiseql_rs = None


def skip_if_no_rust():
    """Skip test if fraiseql_rs is not available."""
    if fraiseql_rs is None:
        pytest.skip("fraiseql_rs not available")


class TestSchemaLoaderBasics:
    """Test basic schema loader functionality."""

    def test_schema_loader_can_import_rust_schema(self):
        """RED: Python schema loader can import Rust-exported schema."""
        skip_if_no_rust()
        # This will fail until schema_loader module is created
        from fraiseql.gql.schema_loader import load_schema

        schema = load_schema()
        assert schema is not None
        assert isinstance(schema, dict)

    def test_loaded_schema_has_required_keys(self):
        """RED: Loaded schema has filter_schemas and order_by_schemas."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema

        schema = load_schema()
        assert "filter_schemas" in schema
        assert "order_by_schemas" in schema
        assert "version" in schema

    def test_schema_loader_caches_schema(self):
        """RED: Schema loader caches loaded schema in memory."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema, _get_cached_schema

        # First load
        schema1 = load_schema()

        # Second load should return same object (cached)
        schema2 = load_schema()

        # Should be the same object in memory
        assert schema1 is schema2

    def test_cached_schema_can_be_retrieved(self):
        """RED: Can retrieve cached schema without reloading."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema, _get_cached_schema

        # Load once
        load_schema()

        # Retrieve from cache without loading
        cached = _get_cached_schema()
        assert cached is not None
        assert isinstance(cached, dict)
        assert "filter_schemas" in cached


class TestSchemaLoaderIntegration:
    """Test schema loader integration with type generation."""

    def test_schema_loader_provides_string_filter_schema(self):
        """RED: Loaded schema provides String filter schema."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema, get_filter_schema

        string_schema = get_filter_schema("String")
        assert string_schema is not None
        assert "fields" in string_schema

    def test_get_filter_schema_returns_all_operators(self):
        """RED: get_filter_schema returns complete operator list."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import get_filter_schema

        string_schema = get_filter_schema("String")
        operators = string_schema["fields"]

        # Verify all expected operators present
        expected_ops = [
            "eq", "neq", "contains", "icontains", "startswith", "istartswith",
            "endswith", "iendswith", "like", "ilike", "matches", "imatches",
            "not_matches", "in_", "nin", "notin", "isnull"
        ]

        for op in expected_ops:
            assert op in operators, f"Missing operator '{op}' in String filter"

    def test_get_filter_schema_for_all_types(self):
        """RED: get_filter_schema works for all filter types."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema, get_filter_schema

        schema = load_schema()
        filter_types = schema["filter_schemas"].keys()

        # Verify we can get schema for each type
        for type_name in filter_types:
            schema = get_filter_schema(type_name)
            assert schema is not None
            assert "fields" in schema


class TestSchemaLoaderTypeGeneration:
    """Test integration with GraphQL type generation."""

    def test_can_get_operators_for_type_generation(self):
        """RED: Can retrieve operators for a type from loaded schema."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import get_filter_operators

        ops = get_filter_operators("String")
        assert isinstance(ops, dict)
        assert len(ops) > 0
        assert "eq" in ops
        assert "in_" in ops  # Python keyword handling

    def test_get_filter_operators_returns_type_info(self):
        """RED: Filter operators include type and nullable information."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import get_filter_operators

        ops = get_filter_operators("String")

        # Each operator should have type info
        for op_name, op_def in ops.items():
            assert "type" in op_def
            assert "nullable" in op_def

    def test_order_by_schema_available(self):
        """RED: OrderBy schema is available via schema loader."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema, get_order_by_schema

        order_by = get_order_by_schema()
        assert order_by is not None
        assert "directions" in order_by
        assert "ASC" in order_by["directions"]
        assert "DESC" in order_by["directions"]
