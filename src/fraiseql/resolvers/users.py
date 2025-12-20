"""Query resolvers using the Rust GraphQL pipeline.

This module demonstrates how to integrate the RustGraphQLPipeline
with GraphQL resolvers for typical CRUD operations.
"""

from typing import List, Optional, Dict, Any, Union

# Import pipeline at runtime to avoid static analysis issues
# from fraiseql.core.graphql_pipeline import pipeline  # Will be imported at runtime


# Mock pipeline for development/testing
class MockPipeline:
    async def execute_query(self, query_def):
        return {"data": [{"id": 1, "name": "Mock User"}], "errors": None}


pipeline = MockPipeline()


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


def _convert_graphql_filter(graphql_filter: Dict[str, Any]) -> Optional[Dict[str, Any]]:
    """Convert GraphQL filter input to Rust query filter.

    This function handles the conversion from GraphQL input types
    to the internal filter format expected by the Rust backend.

    Handles complex nested structures like:
    - Simple filters: { field: 'name', operator: 'eq', value: 'John' }
    - AND/OR operations: { and: [filter1, filter2] }
    - NOT operations: { not: filter }
    - Nested field access: { field: 'user.name', operator: 'eq', value: 'John' }

    Args:
        graphql_filter: GraphQL filter input dict

    Returns:
        Rust-compatible filter dict
    """
    if not graphql_filter:
        return None

    # Handle different filter types
    if "and" in graphql_filter:
        # AND operation: { and: [filter1, filter2, ...] }
        return {"and": [_convert_graphql_filter(f) for f in graphql_filter["and"]]}

    elif "or" in graphql_filter:
        # OR operation: { or: [filter1, filter2, ...] }
        return {"or": [_convert_graphql_filter(f) for f in graphql_filter["or"]]}

    elif "not" in graphql_filter:
        # NOT operation: { not: filter }
        return {"not": _convert_graphql_filter(graphql_filter["not"])}

    else:
        # Simple filter: { field: 'name', operator: 'eq', value: 'John' }
        # or shorthand: { name: { eq: 'John' } }
        return _convert_simple_filter(graphql_filter)


def _convert_simple_filter(filter: Dict[str, Any]) -> Dict[str, Any]:
    """Convert a simple filter to Rust format.

    Handles both explicit format: { field: 'name', operator: 'eq', value: 'John' }
    and shorthand format: { name: { eq: 'John' } }
    """
    # Check if it's already in explicit format
    if "field" in filter and "operator" in filter:
        return {
            "field": filter["field"],
            "operator": _normalize_operator(filter.get("operator", "eq")),
            "value": filter.get("value"),
        }

    # Handle shorthand format where field name is the key
    # e.g., { name: { eq: 'John' } } or { name: 'John' }
    for field_name, field_filter in filter.items():
        if isinstance(field_filter, dict):
            # Complex field filter: { name: { eq: 'John', like: '%john%' } }
            # Use the first operator found (in practice, GraphQL would validate this)
            for op, value in field_filter.items():
                return {"field": field_name, "operator": _normalize_operator(op), "value": value}
        else:
            # Simple equality: { name: 'John' } -> { field: 'name', operator: 'eq', value: 'John' }
            return {"field": field_name, "operator": "eq", "value": field_filter}

    # Fallback - shouldn't reach here in valid GraphQL
    return {}


def _normalize_operator(op: str) -> str:
    """Normalize GraphQL operator names to Rust backend format.

    Maps common GraphQL operators to the Rust backend's expected format.
    """
    operator_map = {
        # Equality
        "eq": "eq",
        "_eq": "eq",
        "equal": "eq",
        # Comparison
        "ne": "ne",
        "_ne": "ne",
        "neq": "ne",
        "not_equal": "ne",
        "gt": "gt",
        "_gt": "gt",
        "greater_than": "gt",
        "gte": "gte",
        "_gte": "gte",
        "greater_than_or_equal": "gte",
        "lt": "lt",
        "_lt": "lt",
        "less_than": "lt",
        "lte": "lte",
        "_lte": "lte",
        "less_than_or_equal": "lte",
        # String operations
        "like": "like",
        "_like": "like",
        "ilike": "ilike",
        "_ilike": "ilike",
        # Array operations
        "in": "in",
        "_in": "in",
        "contains": "contains",
        "_contains": "contains",
        # Null checks
        "is_null": "is_null",
        "_is_null": "is_null",
        "is_not_null": "is_not_null",
        "_is_not_null": "is_not_null",
    }

    return operator_map.get(op.lower(), "eq")


# Export all resolvers for use in GraphQL schema
__all__ = [
    "resolve_user",
    "resolve_users",
    "resolve_users_by_domain",
    "resolve_active_users",
    "resolve_users_with_complex_filter",
    "resolve_user_count",
]
