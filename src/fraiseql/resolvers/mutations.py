"""Mutation resolvers using the Rust GraphQL pipeline.

This module demonstrates how to integrate the RustGraphQLPipeline
with GraphQL mutation resolvers for typical CRUD operations.
"""

from typing import Dict, Any, List
from datetime import datetime
from fraiseql.core.graphql_pipeline import pipeline


async def resolve_create_user(obj: Any, info: Any, input: Dict[str, Any]) -> Dict[str, Any]:
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
    obj: Any, info: Any, id: int, input: Dict[str, Any]
) -> Dict[str, Any]:
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


async def resolve_delete_user(obj: Any, info: Any, id: int) -> Dict[str, Any]:
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
    obj: Any, info: Any, filter_input: Dict[str, Any], input: Dict[str, Any]
) -> Dict[str, Any]:
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


async def resolve_create_post(obj: Any, info: Any, input: Dict[str, Any]) -> Dict[str, Any]:
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


async def resolve_publish_post(obj: Any, info: Any, id: int) -> Dict[str, Any]:
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


async def resolve_bulk_delete_posts(obj: Any, info: Any, user_id: int) -> Dict[str, Any]:
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


# Export all mutation resolvers
__all__ = [
    "resolve_create_user",
    "resolve_update_user",
    "resolve_delete_user",
    "resolve_bulk_update_users",
    "resolve_create_post",
    "resolve_publish_post",
    "resolve_bulk_delete_posts",
]
