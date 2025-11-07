"""Rust-first pipeline for PostgreSQL → HTTP response.

This module provides zero-copy path from database to HTTP by delegating
ALL string operations to Rust after query execution.

Updated for fraiseql_rs v0.2.0: Uses unified build_graphql_response() API
instead of deprecated build_*_response() functions.
"""

import json
import logging
from typing import Any, Optional

from psycopg import AsyncConnection
from psycopg.sql import SQL, Composed


# Lazy-load the Rust extension to avoid circular import issues
# The fraiseql package re-exports _fraiseql_rs in its __init__.py
def _get_fraiseql_rs():
    """Lazy-load the Rust extension module."""
    try:
        from fraiseql import _fraiseql_rs

        return _fraiseql_rs
    except ImportError as e:
        raise ImportError(
            "fraiseql Rust extension is not available. "
            "Please reinstall fraiseql: pip install --force-reinstall fraiseql"
        ) from e


# Create a namespace object that lazy-loads functions
class _FraiseQLRs:
    _module = None

    @staticmethod
    def build_graphql_response(*args: Any, **kwargs: Any) -> Any:
        if _FraiseQLRs._module is None:
            _FraiseQLRs._module = _get_fraiseql_rs()
        return _FraiseQLRs._module.build_graphql_response(*args, **kwargs)


fraiseql_rs = _FraiseQLRs()

logger = logging.getLogger(__name__)


class RustResponseBytes:
    """Marker for pre-serialized response bytes from Rust.

    FastAPI detects this type and sends bytes directly without any
    Python serialization or string operations.

    This class supports optional schema_type tracking for debugging and
    provides a to_json() method for testing purposes (not recommended for
    production due to performance overhead).

    WORKAROUND: Fixes known Rust bug where closing brace is missing for
    data object when query has nested objects. This is a temporary fix
    until fraiseql-rs is updated.

    Args:
        data: Pre-serialized JSON bytes from Rust
        schema_type: Optional GraphQL schema type name for debugging (e.g., "Product", "User")

    Examples:
        >>> # Basic usage (existing code - backwards compatible)
        >>> response = RustResponseBytes(b'{"data":{"hello":"world"}}')
        >>> bytes(response)
        b'{"data":{"hello":"world"}}'

        >>> # With schema type tracking (Phase 3 enhancement)
        >>> response = RustResponseBytes(b'{"data":{"products":[]}}', schema_type="Product")
        >>> response.schema_type
        'Product'

        >>> # Testing with to_json() (Phase 3 - for tests only!)
        >>> response.to_json()
        {'data': {'products': []}}
    """

    __slots__ = ("_data", "_fixed", "_schema_type", "content_type")

    def __init__(self, data: bytes, schema_type: Optional[str] = None) -> None:
        self._data = data
        self.content_type = "application/json"
        self._fixed = False
        self._schema_type = schema_type

    @property
    def bytes(self) -> bytes:
        """Backward compatibility property for accessing the data."""
        return self._data

    @property
    def schema_type(self) -> Optional[str]:
        """Get the GraphQL schema type name for this response.

        This property is useful for debugging and understanding what type
        the RustResponseBytes represents. For example, if this response
        contains a list of Product objects, schema_type would be "Product".

        Returns:
            The GraphQL schema type name, or None if not set

        Examples:
            >>> response = RustResponseBytes(b'{"data":{"products":[]}}', schema_type="Product")
            >>> response.schema_type
            'Product'

            >>> response = RustResponseBytes(b'{"data":{}}')
            >>> response.schema_type is None
            True
        """
        return self._schema_type

    def to_json(self) -> dict:
        """Parse the response bytes as JSON and return as dict.

        ⚠️ WARNING: This method is intended for TESTING ONLY!

        In production, RustResponseBytes should be sent directly to the client
        via __bytes__() without any parsing. This method defeats the purpose
        of the zero-copy architecture and should only be used in test code
        for assertions.

        Returns:
            Parsed JSON as a Python dict

        Raises:
            json.JSONDecodeError: If the bytes don't contain valid JSON

        Examples:
            >>> response = RustResponseBytes(b'{"data":{"hello":"world"}}')
            >>> response.to_json()
            {'data': {'hello': 'world'}}

            >>> # In tests, you can use this to verify structure
            >>> data = response.to_json()
            >>> assert data["data"]["hello"] == "world"

            >>> # But DON'T use this in production - use __bytes__() instead!
            >>> bytes(response)  # ✅ Good - zero-copy
            b'{"data":{"hello":"world"}}'
        """
        # Use __bytes__() to get the (potentially fixed) bytes
        data_bytes = self.__bytes__()
        return json.loads(data_bytes)

    def __bytes__(self) -> bytes:
        # Workaround for Rust bug: Check if JSON is missing closing brace
        if not self._fixed:
            try:
                # Try to parse the JSON
                json_str = self._data.decode("utf-8")  # type: ignore[union-attr]
                json.loads(json_str)
                # If it parses, no fix needed
                self._fixed = True
            except json.JSONDecodeError as e:
                # Check if it's the known "missing closing brace" bug
                if "Expecting ',' delimiter" in str(e) and e.pos >= len(json_str) - 2:
                    # Count braces to confirm
                    open_braces = json_str.count("{")
                    close_braces = json_str.count("}")

                    if open_braces > close_braces:
                        # Missing closing brace(s) - add them
                        missing_braces = open_braces - close_braces
                        fixed_json = json_str + ("}" * missing_braces)  # type: ignore[operator]

                        # Verify the fix works
                        try:
                            json.loads(fixed_json)
                            logger.warning(
                                f"Applied workaround for Rust JSON bug: "
                                f"Added {missing_braces} missing closing brace(s). "
                                f"This bug affects queries with nested objects. "
                                f"Update fraiseql-rs to fix permanently."
                            )
                            self._data = fixed_json.encode("utf-8")
                            self._fixed = True
                        except json.JSONDecodeError:
                            # Fix didn't work, return original
                            logger.error(
                                "Rust JSON workaround failed - returning original malformed JSON"
                            )
                    else:
                        # Different JSON error, return original
                        pass
                else:
                    # Different JSON error, return original
                    pass

        return self._data


async def execute_via_rust_pipeline(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: Optional[dict[str, Any]],
    field_name: str,
    type_name: Optional[str],
    is_list: bool = True,
    field_paths: Optional[list[list[str]]] = None,
    field_selections: Optional[list[dict[str, Any]]] = None,
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
        field_selections: Optional field selections with aliases and type info (NEW for Phase 3)

    Returns:
        RustResponseBytes ready for HTTP response

    Note:
        field_selections parameter is for Phase 3 aliasing support. Currently passed but not yet
        used by Rust until Task 3.4 (Update FFI) is complete. Maintains backward compatibility.
    """
    async with conn.cursor() as cursor:
        # Handle statement execution based on type and parameter presence
        # When query is Composed without params, all values are embedded as literals
        # Passing an empty dict causes psycopg to look for % placeholders
        if isinstance(query, Composed) and not params:
            # Composed objects without params have only embedded literals
            await cursor.execute(query)
        elif params:
            # Pass parameters for queries that need them
            await cursor.execute(query, params)
        else:
            # SQL objects or other types without parameters
            await cursor.execute(query)

        if is_list:
            rows = await cursor.fetchall()

            if not rows:
                response_bytes = fraiseql_rs.build_graphql_response(
                    json_strings=[],
                    field_name=field_name,
                    type_name=None,
                    field_paths=None,
                    is_list=True,
                )
                return RustResponseBytes(response_bytes)

            # Extract JSON strings (PostgreSQL returns as text)
            json_strings = [row[0] for row in rows if row[0] is not None]

            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=json_strings,
                field_name=field_name,
                type_name=type_name,
                field_paths=field_paths,
                is_list=True,
            )

            return RustResponseBytes(response_bytes)
        # Single object
        row = await cursor.fetchone()

        if not row or row[0] is None:
            response_bytes = fraiseql_rs.build_graphql_response(
                json_strings=[],
                field_name=field_name,
                type_name=None,
                field_paths=None,
                is_list=False,
            )
            return RustResponseBytes(response_bytes)

        json_string = row[0]

        response_bytes = fraiseql_rs.build_graphql_response(
            json_strings=[json_string],
            field_name=field_name,
            type_name=type_name,
            field_paths=field_paths,
            is_list=False,
        )

        return RustResponseBytes(response_bytes)
