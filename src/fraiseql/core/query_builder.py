"""Rust-based SQL query builder."""

from dataclasses import dataclass
from typing import Optional
from fraiseql._fraiseql_rs import build_sql_query, GeneratedQuery
from fraiseql.core.graphql_parser import ParsedQuery


@dataclass
class ComposedQuery:
    """Result of SQL composition."""

    sql: str
    parameters: dict[str, str]


class RustQueryBuilder:
    """SQL query builder using Rust pipeline."""

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

    @staticmethod
    def _serialize_schema(metadata: dict) -> str:
        """Serialize schema metadata to JSON."""
        import json

        return json.dumps(metadata)
