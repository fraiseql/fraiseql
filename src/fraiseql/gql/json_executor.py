"""JSON passthrough executor for optimized GraphQL queries.

This module provides the integration between GraphQL resolvers and raw JSON
passthrough, enabling direct database-to-HTTP response flow.

With the new architecture, the repository layer automatically handles raw JSON
in production mode, simplifying this module significantly.
"""

import logging
from typing import Any, Callable

logger = logging.getLogger(__name__)


def should_use_json_passthrough(context: dict[str, Any]) -> bool:
    """Determine if JSON passthrough should be used for this request.

    Args:
        context: The GraphQL request context

    Returns:
        True if JSON passthrough is enabled and conditions are met
    """
    # Check if explicitly enabled
    if not context.get("json_passthrough", False):
        return False

    # Only in production mode
    if context.get("mode") != "production":
        return False

    # Check if database supports raw JSON methods
    db = context.get("db")
    if not db or not hasattr(db, "find_raw_json"):
        return False

    return True


async def execute_json_query(fn: Callable, info: Any, **kwargs: Any) -> Any:
    """Execute a query function.

    With the new architecture, the repository layer handles raw JSON
    automatically in production mode, so this function just executes
    the resolver normally.

    Args:
        fn: The resolver function
        info: GraphQL resolve info
        **kwargs: Query arguments

    Returns:
        The result from the resolver (which may be a RawJSONResult in production)
    """
    # Simply execute the resolver
    # The repository will return RawJSONResult in production mode
    return await fn(info, **kwargs)


def execute_sync_json_query(fn: Callable, info: Any, **kwargs: Any) -> Any:
    """Execute a sync query function.

    With the new architecture, the repository layer handles raw JSON
    automatically in production mode, so this function just executes
    the resolver normally.

    Args:
        fn: The resolver function
        info: GraphQL resolve info
        **kwargs: Query arguments

    Returns:
        The result from the resolver
    """
    # Simply execute the resolver
    # The repository will return appropriate result based on mode
    return fn(info, **kwargs)


# Complex logic detection is no longer needed with the new architecture
# The repository layer handles raw JSON automatically in production mode


# The create_json_passthrough_resolver function is no longer needed
# The repository layer handles raw JSON automatically in production mode
