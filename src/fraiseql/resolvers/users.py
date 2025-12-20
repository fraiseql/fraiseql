"""Query resolvers using the Rust GraphQL pipeline.

This module demonstrates how to integrate the RustGraphQLPipeline
with GraphQL resolvers for typical CRUD operations.
"""

from typing import List, Optional, Dict, Any
from fraiseql.core.graphql_pipeline import pipeline


async def resolve_user(obj: Any, info: Any, id: int) -> Optional[Dict[str, Any]]:
    """Resolve single user query: query { user(id: 1) { id, name, email } }

    Args:
        obj: Parent object (None for root queries)
        info: GraphQL execution info
        id: User ID to fetch

    Returns:
        User dict or None if not found
    """
    query_def = {
        "operation": "query",
        "table": "users",
        "fields": ["id", "name", "email", "created_at"],
        "filters": {"field": "id", "operator": "eq", "value": id},
    }

    result = await pipeline.execute_query(query_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    # Result is list, return first item or None
    data = result["data"]
    return data[0] if data else None


async def resolve_users(
    obj: Any, info: Any, limit: int = 10, offset: int = 0, sort_by: str = "name"
) -> List[Dict[str, Any]]:
    """Resolve users list query: query { users(limit: 10) { id, name, email } }

    Args:
        obj: Parent object (None for root queries)
        info: GraphQL execution info
        limit: Maximum number of users to return
        offset: Number of users to skip
        sort_by: Field to sort by

    Returns:
        List of user dicts
    """
    query_def = {
        "operation": "query",
        "table": "users",
        "fields": ["id", "name", "email", "created_at"],
        "filters": None,  # No WHERE clause
        "pagination": {"limit": limit, "offset": offset},
        "sort": [{"field": sort_by, "direction": "ASC"}],
    }

    result = await pipeline.execute_query(query_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_users_by_domain(obj: Any, info: Any, domain: str) -> List[Dict[str, Any]]:
    """Resolve users filtered by email domain.

    Args:
        obj: Parent object
        info: GraphQL execution info
        domain: Email domain to filter by (e.g., "example.com")

    Returns:
        List of users with emails in the specified domain
    """
    query_def = {
        "operation": "query",
        "table": "users",
        "fields": ["id", "name", "email"],
        "filters": {"field": "email", "operator": "like", "value": f"%@{domain}"},
    }

    result = await pipeline.execute_query(query_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_active_users(obj: Any, info: Any) -> List[Dict[str, Any]]:
    """Resolve only active users.

    Args:
        obj: Parent object
        info: GraphQL execution info

    Returns:
        List of active users
    """
    query_def = {
        "operation": "query",
        "table": "users",
        "fields": ["id", "name", "email", "is_active"],
        "filters": {"field": "is_active", "operator": "eq", "value": True},
    }

    result = await pipeline.execute_query(query_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_users_with_complex_filter(
    obj: Any, info: Any, filter_input: Dict[str, Any]
) -> List[Dict[str, Any]]:
    """Resolve users with complex nested filters.

    Args:
        obj: Parent object
        info: GraphQL execution info
        filter_input: Complex filter input (converted from GraphQL input types)

    Returns:
        List of users matching the complex filter
    """
    # Convert GraphQL filter input to Rust query filter
    filters = _convert_graphql_filter(filter_input)

    query_def = {
        "operation": "query",
        "table": "users",
        "fields": ["id", "name", "email", "is_active", "created_at"],
        "filters": filters,  # Complex AND/OR/NOT structure
    }

    result = await pipeline.execute_query(query_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_user_count(obj: Any, info: Any) -> int:
    """Resolve total user count.

    Args:
        obj: Parent object
        info: GraphQL execution info

    Returns:
        Total number of users
    """
    query_def = {
        "operation": "query",
        "table": "users",
        "fields": ["count(*)"],
        "aggregation": True,  # Special flag for count queries
    }

    result = await pipeline.execute_query(query_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    # For count queries, return the first row's first column
    data = result["data"]
    if data and len(data) > 0:
        return int(data[0]["count"])
    return 0


def _convert_graphql_filter(graphql_filter: Dict[str, Any]) -> Dict[str, Any]:
    """Convert GraphQL filter input to Rust query filter.

    This function handles the conversion from GraphQL input types
    to the internal filter format expected by the Rust backend.

    Args:
        graphql_filter: GraphQL filter input dict

    Returns:
        Rust-compatible filter dict
    """
    # This is a simplified conversion. In a real implementation,
    # you'd handle complex nested structures like:
    # { and: [{ field: 'is_active', eq: true }, { field: 'created_at', gte: '2025-01-01' }] }

    # For now, pass through directly (assuming GraphQL input matches Rust format)
    return graphql_filter


# Export all resolvers for use in GraphQL schema
__all__ = [
    "resolve_user",
    "resolve_users",
    "resolve_users_by_domain",
    "resolve_active_users",
    "resolve_users_with_complex_filter",
    "resolve_user_count",
]
