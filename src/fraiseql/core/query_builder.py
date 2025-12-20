"""Rust-based SQL query builder with caching."""

from dataclasses import dataclass
from typing import Optional
from fraiseql._fraiseql_rs import (
    build_sql_query,
    build_sql_query_cached,
    get_cache_stats,
    clear_cache,
    GeneratedQuery,
)
from fraiseql.core.graphql_parser import ParsedQuery


@dataclass
class ComposedQuery:
    """Result of SQL composition."""

    sql: str
    parameters: dict[str, str]


class RustQueryBuilder:
    """SQL query builder with caching."""

    def build(
        self,
        parsed_query: ParsedQuery,
        schema_metadata: dict,
    ) -> GeneratedQuery:
        """
        Build complete SQL query from parsed GraphQL.

        Args:
            parsed_query: Result from GraphQL parser
            schema_metadata: Schema information

        Returns:
            GeneratedQuery with SQL and parameters
        """
        schema_json = self._serialize_schema(schema_metadata)
        return build_sql_query(parsed_query, schema_json)

    def build_cached(
        self,
        parsed_query: ParsedQuery,
        schema_metadata: dict,
    ) -> GeneratedQuery:
        """
        Build query with caching for repeated queries.

        Args:
            parsed_query: Result from GraphQL parser
            schema_metadata: Schema information

        Returns:
            GeneratedQuery with SQL and parameters (cached if possible)
        """
        schema_json = self._serialize_schema(schema_metadata)
        return build_sql_query_cached(parsed_query, schema_json)

    @staticmethod
    def get_stats() -> dict:
        """Get cache statistics."""
        return get_cache_stats()

    @staticmethod
    def clear_cache():
        """Clear query plan cache."""
        return clear_cache()

    @staticmethod
    def _serialize_schema(metadata: dict) -> str:
        """Serialize schema metadata to JSON."""
        import json

        return json.dumps(metadata)
