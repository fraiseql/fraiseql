"""Rust-based GraphQL query parser."""

from typing import Optional
from dataclasses import dataclass
from fraiseql._fraiseql_rs import (
    parse_graphql_query,
    ParsedQuery,
    FieldSelection,
    GraphQLArgument,
    VariableDefinition,
)

__all__ = [
    "RustGraphQLParser",
    "ParsedQuery",
    "FieldSelection",
]


class RustGraphQLParser:
    """Wrapper around Rust GraphQL parser for FraiseQL."""

    async def parse(self, query_string: str) -> ParsedQuery:
        """
        Parse GraphQL query string into structured AST.

        Args:
            query_string: Raw GraphQL query text

        Returns:
            ParsedQuery with operation type, fields, arguments, etc.

        Raises:
            SyntaxError: If query is invalid GraphQL
        """
        return parse_graphql_query(query_string)

    def parse_sync(self, query_string: str) -> ParsedQuery:
        """
        Synchronous wrapper (not recommended - use async version).

        This is for testing only. In production, use async version.
        """
        # Note: This would need special handling - for now skip
        raise NotImplementedError("Use async parse() instead")
