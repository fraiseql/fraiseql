"""Test utilities for FraiseQL.

This package contains utility functions and classes to help with testing
particularly for database integration tests and schema-qualified queries.
"""

from .schema_utils import (
    SchemaQualifiedQueryBuilder,
    build_delete_query,
    build_insert_query,
    build_select_query,
    build_update_query,
    create_test_schema_context,
    drop_test_schema_context,
    get_current_schema,
    schema_qualified_composed,
    schema_qualified_sql,
)

__all__ = [
    "SchemaQualifiedQueryBuilder",
    "build_delete_query",
    "build_insert_query",
    "build_select_query",
    "build_update_query",
    "create_test_schema_context",
    "drop_test_schema_context",
    "get_current_schema",
    "schema_qualified_composed",
    "schema_qualified_sql",
]
