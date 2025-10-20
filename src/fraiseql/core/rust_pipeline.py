"""Rust-first pipeline for PostgreSQL → HTTP response.

This module provides zero-copy path from database to HTTP by delegating
ALL string operations to Rust after query execution.

Updated for fraiseql_rs v0.2.0: Uses unified build_graphql_response() API
instead of deprecated build_*_response() functions.
"""

from typing import Any, Dict, List, Optional

from psycopg import AsyncConnection
from psycopg.sql import SQL, Composed

try:
    import fraiseql_rs
except ImportError as e:
    raise ImportError(
        "fraiseql-rs is required for the Rust pipeline. Install: pip install fraiseql-rs"
    ) from e


class RustResponseBytes:
    """Marker for pre-serialized response bytes from Rust.

    FastAPI detects this type and sends bytes directly without any
    Python serialization or string operations.
    """

    __slots__ = ("bytes", "content_type")

    def __init__(self, data):
        self.bytes = data
        self.content_type = "application/json"

    def __bytes__(self):
        return self.bytes


async def execute_via_rust_pipeline(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: Optional[Dict[str, Any]],
    field_name: str,
    type_name: Optional[str],
    is_list: bool = True,
    field_paths: Optional[List[List[str]]] = None,
) -> RustResponseBytes:
    """Execute query and build HTTP response entirely in Rust.

    This is the FASTEST path: PostgreSQL → Rust → HTTP bytes.
    Zero Python string operations, zero JSON parsing, zero copies.

    Uses fraiseql_rs v0.2.0 unified build_graphql_response() API for
    camelCase conversion, __typename injection, and field projection.

    Args:
        conn: PostgreSQL connection
        query: SQL query returning JSON strings
        params: Query parameters
        field_name: GraphQL field name (e.g., "users")
        type_name: GraphQL type for transformation (e.g., "User")
        is_list: True for arrays, False for single objects
        field_paths: Optional field paths for projection (e.g., [["id"], ["firstName"]])

    Returns:
        RustResponseBytes ready for HTTP response
    """
    async with conn.cursor() as cursor:
        await cursor.execute(query, params or {})

        if is_list:
            rows = await cursor.fetchall()

            if not rows:
                # Empty array response
                response_bytes = fraiseql_rs.build_graphql_response(
                    json_strings=[],
                    field_name=field_name,
                    type_name=None,  # No typename for empty
                    field_paths=None,
                )
                return RustResponseBytes(response_bytes)

            # Extract JSON strings (PostgreSQL returns as text)
            json_strings = [row[0] for row in rows if row[0] is not None]

            # 🚀 UNIFIED API (v0.2.0):
            # - Field projection: Filter only requested fields
            # - Concatenate: ['{"id":"1"}', '{"id":"2"}'] → '[{"id":"1"},{"id":"2"}]'
            # - Wrap: '[...]' → '{"data":{"users":[...]}}'
            # - Transform: snake_case → camelCase + __typename
            # - Encode: String → UTF-8 bytes
            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=json_strings,
                field_name=field_name,
                type_name=type_name,
                field_paths=field_paths,  # None = no projection
            )

            return RustResponseBytes(response_bytes)
        # Single object
        row = await cursor.fetchone()

        if not row or row[0] is None:
            # Null response - use empty structure or null
            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=[], field_name=field_name, type_name=None, field_paths=None
            )
            return RustResponseBytes(response_bytes)

        json_string = row[0]

        # 🚀 UNIFIED API (v0.2.0):
        # - Field projection: Filter only requested fields
        # - Wrap: '{"id":"1"}' → '{"data":{"user":{"id":"1"}}}'
        # - Transform: snake_case → camelCase + __typename
        # - Encode: String → UTF-8 bytes
        response_bytes = fraiseql_rs.build_graphql_response(
            json_strings=[json_string],  # Single item as list
            field_name=field_name,
            type_name=type_name,
            field_paths=field_paths,  # None = no projection
        )

        return RustResponseBytes(response_bytes)
