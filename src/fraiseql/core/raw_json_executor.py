"""Raw JSON query executor for direct PostgreSQL to HTTP passthrough.

This module provides functionality to execute queries that return raw JSON strings
from PostgreSQL, bypassing all Python object creation and JSON parsing overhead.
"""

import logging
from typing import Any, Optional, Union

from psycopg import AsyncConnection
from psycopg.sql import SQL, Composed, Literal

logger = logging.getLogger(__name__)


class RawJSONResult:
    """Marker class for raw JSON results that should bypass serialization."""

    __slots__ = ("content_type", "json_string")

    def __init__(self, json_string: str):
        """Initialize with a raw JSON string.

        Args:
            json_string: The raw JSON string from PostgreSQL
        """
        self.json_string = json_string
        self.content_type = "application/json"

    def __repr__(self):
        preview = (
            self.json_string[:100] + "..." if len(self.json_string) > 100 else self.json_string
        )
        return f"RawJSONResult({preview})"


async def execute_raw_json_query(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: dict[str, Any] | None = None,
    field_name: Optional[str] = None,
) -> RawJSONResult:
    """Execute a query and return raw JSON string wrapped for GraphQL response.

    This function executes a SQL query that returns JSON and wraps it in a
    GraphQL-compliant response structure without any Python parsing.

    Args:
        conn: The PostgreSQL connection
        query: The SQL query (should return JSON)
        params: Query parameters
        field_name: The GraphQL field name for wrapping the result

    Returns:
        RawJSONResult containing the complete GraphQL response as JSON
    """
    async with conn.cursor() as cursor:
        # Execute query without row factory to get raw text
        await cursor.execute(query, params or {})
        result = await cursor.fetchone()

        if not result:
            # Return null wrapped in GraphQL response
            if field_name:
                return RawJSONResult(f'{{"data":{{{field_name}":null}}}}')
            return RawJSONResult('{"data":null}')

        # Get the raw JSON string from the first column
        json_data = result[0]

        # Handle None result
        if json_data is None:
            if field_name:
                return RawJSONResult(f'{{"data":{{{field_name}":null}}}}')
            return RawJSONResult('{"data":null}')

        # If we have a field name, wrap in GraphQL response structure
        if field_name:
            # Escape the field name for JSON
            escaped_field = field_name.replace('"', '\\"')
            return RawJSONResult(f'{{"data":{{"{escaped_field}":{json_data}}}}}')

        # Otherwise return the JSON directly wrapped in data
        return RawJSONResult(f'{{"data":{json_data}}}')


async def execute_raw_json_list_query(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: dict[str, Any] | None = None,
    field_name: Optional[str] = None,
) -> RawJSONResult:
    """Execute a query that returns multiple rows as a JSON array.

    This function executes a SQL query that returns multiple JSON rows and
    combines them into a JSON array without parsing.

    Args:
        conn: The PostgreSQL connection
        query: The SQL query (should return JSON in each row)
        params: Query parameters
        field_name: The GraphQL field name for wrapping the result

    Returns:
        RawJSONResult containing the complete GraphQL response as JSON
    """
    async with conn.cursor() as cursor:
        # Execute query without row factory
        await cursor.execute(query, params or {})
        rows = await cursor.fetchall()

        if not rows:
            # Return empty array wrapped in GraphQL response
            if field_name:
                return RawJSONResult(f'{{"data":{{"{field_name}":[]}}}}')
            return RawJSONResult('{"data":[]}')

        # Combine JSON rows into array without parsing
        json_items = []
        for row in rows:
            if row[0] is not None:  # Skip null results
                json_items.append(row[0])

        # Join with commas to form array
        json_array = f"[{','.join(json_items)}]"

        # Wrap in GraphQL response
        if field_name:
            escaped_field = field_name.replace('"', '\\"')
            return RawJSONResult(f'{{"data":{{"{escaped_field}":{json_array}}}}}')

        return RawJSONResult(f'{{"data":{json_array}}}')


def is_query_eligible_for_raw_json(
    query_ast: Any,
    has_custom_resolvers: bool = False,
    has_field_auth: bool = False,
) -> bool:
    """Determine if a query can use raw JSON passthrough.

    A query is eligible if:
    - It's a simple query without fragments or complex features
    - No custom field resolvers that need Python execution
    - No field-level authorization
    - No computed fields

    Args:
        query_ast: The parsed GraphQL query AST
        has_custom_resolvers: Whether the type has custom resolvers
        has_field_auth: Whether field-level auth is needed

    Returns:
        True if the query can use raw JSON passthrough
    """
    # For now, be conservative - only allow very simple queries
    if has_custom_resolvers or has_field_auth:
        return False

    # Check for features that require GraphQL execution
    # - Fragments
    # - Directives
    # - Aliases (other than the root field)
    # - Complex selections

    # This is a simplified check - in practice we'd analyze the AST
    return True


def build_graphql_response_sql(
    base_query: SQL | Composed,
    field_name: str,
    is_list: bool = False,
) -> Union[SQL, Composed]:
    """Wrap a SQL query to return a complete GraphQL response structure.

    This modifies a SQL query to return results in GraphQL response format:
    {"data": {"fieldName": <result>}}

    Args:
        base_query: The base SQL query
        field_name: The GraphQL field name
        is_list: Whether the result should be an array

    Returns:
        Modified SQL query that returns GraphQL response JSON
    """
    if is_list:
        # For lists, aggregate results into JSON array
        return SQL(
            """
            SELECT json_build_object(
                'data', json_build_object(
                    {}, COALESCE(json_agg(result.data), '[]'::json)
                )
            )::text
            FROM ({}) AS result
        """
        ).format(Literal(field_name), base_query)
    # For single results, wrap in GraphQL structure
    return SQL(
        """
            SELECT json_build_object(
                'data', json_build_object(
                    {}, result.data
                )
            )::text
            FROM ({}) AS result
            LIMIT 1
        """
    ).format(Literal(field_name), base_query)
