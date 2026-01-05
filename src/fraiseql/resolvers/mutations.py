"""Mutation resolvers using the Rust GraphQL pipeline.

This module demonstrates how to integrate the RustGraphQLPipeline
with GraphQL mutation resolvers for typical CRUD operations.
"""

from datetime import datetime
from typing import Any

# from fraiseql.core.graphql_pipeline import pipeline  # Will be imported at runtime


# Mock pipeline for development/testing
class MockPipeline:
    async def execute_mutation(self, mutation_def):
        return {"data": {"id": 1, "name": "Created User"}, "errors": None}


pipeline = MockPipeline()


async def resolve_create_user(obj: Any, info: Any, input: dict[str, Any]) -> dict[str, Any]:
    """Create user mutation: mutation { createUser(input: {name, email}) { id, name, email } }

    Args:
        obj: Parent object (None for root mutations)
        info: GraphQL execution info
        input: User input data

    Returns:
        Created user data
    """
    mutation_def = {
        "operation": "mutation",
        "type": "insert",
        "table": "users",
        "input": {
            "name": input["name"],
            "email": input["email"],
            "is_active": input.get("is_active", True),
            "created_at": datetime.utcnow().isoformat(),
        },
        "return_fields": ["id", "name", "email", "is_active", "created_at"],
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_update_user(
    obj: Any,
    info: Any,
    id: int,
    input: dict[str, Any],
) -> dict[str, Any]:
    """Update user mutation: mutation { updateUser(id: 1, input: {name}) { id, name, email } }

    Args:
        obj: Parent object
        info: GraphQL execution info
        id: User ID to update
        input: Updated user data

    Returns:
        Updated user data
    """
    mutation_def = {
        "operation": "mutation",
        "type": "update",
        "table": "users",
        "filters": {"field": "id", "operator": "eq", "value": id},
        "input": {
            key: value
            for key, value in input.items()
            if value is not None  # Only update provided fields
        },
        "return_fields": ["id", "name", "email", "is_active", "updated_at"],
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_delete_user(obj: Any, info: Any, id: int) -> dict[str, Any]:
    """Delete user mutation: mutation { deleteUser(id: 1) { success, message } }

    Args:
        obj: Parent object
        info: GraphQL execution info
        id: User ID to delete

    Returns:
        Deletion confirmation
    """
    mutation_def = {
        "operation": "mutation",
        "type": "delete",
        "table": "users",
        "filters": {"field": "id", "operator": "eq", "value": id},
        "return_fields": None,  # No need to return deleted record
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return {"success": True, "message": f"User {id} deleted"}


async def resolve_bulk_update_users(
    obj: Any,
    info: Any,
    filter_input: dict[str, Any],
    input: dict[str, Any],
) -> dict[str, Any]:
    """Bulk update users matching filter.

    Args:
        obj: Parent object
        info: GraphQL execution info
        filter_input: Filter criteria for users to update
        input: Update data to apply

    Returns:
        Bulk update results
    """
    filters = _convert_graphql_filter(filter_input)

    mutation_def = {
        "operation": "mutation",
        "type": "update",
        "table": "users",
        "filters": filters,  # Can be complex filter
        "input": input,
        "return_fields": ["id", "name", "email", "updated_at"],
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    # Result is list of updated records
    updated_count = len(result["data"]) if result["data"] else 0
    return {"success": True, "updated_count": updated_count, "records": result["data"]}


async def resolve_create_post(obj: Any, info: Any, input: dict[str, Any]) -> dict[str, Any]:
    """Create post mutation with user association.

    Args:
        obj: Parent object
        info: GraphQL execution info
        input: Post input data including user_id

    Returns:
        Created post data
    """
    mutation_def = {
        "operation": "mutation",
        "type": "insert",
        "table": "posts",
        "input": {
            "title": input["title"],
            "content": input["content"],
            "user_id": input["user_id"],
            "published": input.get("published", False),
            "created_at": datetime.utcnow().isoformat(),
        },
        "return_fields": ["id", "title", "content", "user_id", "published", "created_at"],
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_publish_post(obj: Any, info: Any, id: int) -> dict[str, Any]:
    """Publish a post by updating its published status.

    Args:
        obj: Parent object
        info: GraphQL execution info
        id: Post ID to publish

    Returns:
        Updated post data
    """
    mutation_def = {
        "operation": "mutation",
        "type": "update",
        "table": "posts",
        "filters": {"field": "id", "operator": "eq", "value": id},
        "input": {"published": True, "published_at": datetime.utcnow().isoformat()},
        "return_fields": ["id", "title", "published", "published_at"],
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return result["data"]


async def resolve_bulk_delete_posts(obj: Any, info: Any, user_id: int) -> dict[str, Any]:
    """Delete all posts by a specific user.

    Args:
        obj: Parent object
        info: GraphQL execution info
        user_id: User ID whose posts to delete

    Returns:
        Deletion results
    """
    mutation_def = {
        "operation": "mutation",
        "type": "delete",
        "table": "posts",
        "filters": {"field": "user_id", "operator": "eq", "value": user_id},
        "return_fields": None,
    }

    result = await pipeline.execute_mutation(mutation_def)

    if result["errors"]:
        raise Exception(result["errors"][0]["message"])

    return {"success": True, "message": f"All posts by user {user_id} deleted"}


def _convert_graphql_filter(graphql_filter: dict[str, Any]) -> dict[str, Any] | None:
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
        Rust-compatible filter dict or None if empty
    """
    if not graphql_filter:
        return None

    # Handle different filter types
    if "and" in graphql_filter:
        # AND operation: { and: [filter1, filter2, ...] }
        return {"and": [_convert_graphql_filter(f) for f in graphql_filter["and"] if f]}

    if "or" in graphql_filter:
        # OR operation: { or: [filter1, filter2, ...] }
        return {"or": [_convert_graphql_filter(f) for f in graphql_filter["or"] if f]}

    if "not" in graphql_filter:
        # NOT operation: { not: filter }
        return {"not": _convert_graphql_filter(graphql_filter["not"])}

    # Simple filter: { field: 'name', operator: 'eq', value: 'John' }
    # or shorthand: { name: { eq: 'John' } }
    return _convert_simple_filter(graphql_filter)


def _convert_simple_filter(filter: dict[str, Any]) -> dict[str, Any]:
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


# Export all mutation resolvers
__all__ = [
    "resolve_bulk_delete_posts",
    "resolve_bulk_update_users",
    "resolve_create_post",
    "resolve_create_user",
    "resolve_delete_user",
    "resolve_publish_post",
    "resolve_update_user",
]
