"""Raw JSON query resolver for optimized GraphQL execution.

This module provides a resolver wrapper that can use raw JSON passthrough
when possible, falling back to normal execution for complex queries.
"""

import inspect
import logging
import re
from typing import Any, Callable, Optional

logger = logging.getLogger(__name__)


def create_raw_json_resolver(
    fn: Callable,
    field_name: str,
    return_type: Any,
    arg_name_mapping: dict[str, str] | None = None,
):
    """Create a resolver that uses raw JSON passthrough when possible.

    This wrapper checks if a query can use raw JSON passthrough and executes
    it accordingly. Falls back to normal execution for complex queries.

    Args:
        fn: The original resolver function
        field_name: The GraphQL field name
        return_type: The expected return type
        arg_name_mapping: Mapping from GraphQL to Python argument names

    Returns:
        A wrapped resolver function
    """
    import asyncio

    # Check if this is a simple query that could use raw JSON
    sig = inspect.signature(fn)
    params = sig.parameters

    # Simple heuristic: queries with only info and basic args can use raw JSON
    has_complex_args = any(
        param.annotation and hasattr(param.annotation, "__fraiseql_definition__")
        for name, param in params.items()
        if name not in ("info", "root", "return")
    )

    if asyncio.iscoroutinefunction(fn):

        async def async_raw_json_resolver(root, info, **kwargs):
            # Check if raw JSON passthrough is enabled
            context = getattr(info, "context", {})

            # Determine if we should use raw JSON
            use_raw_json = (
                context.get("json_passthrough", False)
                and context.get("mode") == "production"
                and not has_complex_args
                and hasattr(context.get("db"), "find_one_raw_json")
            )

            if use_raw_json:
                # Try to determine if this is a find or find_one query
                db = context["db"]

                # Map GraphQL argument names to Python parameter names
                if arg_name_mapping:
                    mapped_kwargs = {}
                    for gql_name, value in kwargs.items():
                        python_name = arg_name_mapping.get(gql_name, gql_name)
                        mapped_kwargs[python_name] = value
                    kwargs = mapped_kwargs

                # Inspect the function to see what it does
                # This is a simplified check - in practice we'd analyze the AST
                try:
                    # Get the source code to check what db method is called
                    import textwrap

                    source = inspect.getsource(fn)
                    source = textwrap.dedent(source)

                    # Very basic heuristic - check for db.find_one or db.find
                    if "db.find_one(" in source:
                        # Extract view name if possible (very simplified)
                        match = re.search(r'db\.find_one\("([^"]+)"', source)
                        if match:
                            view_name = match.group(1)
                            logger.debug(f"Using raw JSON for find_one query on {view_name}")
                            result = await db.find_one_raw_json(
                                view_name, field_name, info, **kwargs
                            )
                            return result

                    elif "db.find(" in source:
                        # Extract view name
                        match = re.search(r'db\.find\("([^"]+)"', source)
                        if match:
                            view_name = match.group(1)
                            logger.debug(f"Using raw JSON for find query on {view_name}")
                            result = await db.find_raw_json(view_name, field_name, info, **kwargs)
                            return result

                except Exception as e:
                    logger.debug(f"Could not analyze function for raw JSON: {e}")

            # Fall back to normal execution
            return await fn(info, **kwargs)

        return async_raw_json_resolver

    def sync_raw_json_resolver(root, info, **kwargs):
        # Sync version - raw JSON is primarily for async
        # Map argument names and execute normally
        if arg_name_mapping:
            mapped_kwargs = {}
            for gql_name, value in kwargs.items():
                python_name = arg_name_mapping.get(gql_name, gql_name)
                mapped_kwargs[python_name] = value
            kwargs = mapped_kwargs

        return fn(info, **kwargs)

    return sync_raw_json_resolver


def should_use_raw_json_resolver(
    context: dict[str, Any],
    query_complexity: Optional[int] = None,
) -> bool:
    """Determine if raw JSON resolver should be used.

    Args:
        context: The GraphQL context
        query_complexity: Optional query complexity score

    Returns:
        True if raw JSON resolver should be used
    """
    # Check if explicitly enabled
    if not context.get("json_passthrough", False):
        return False

    # Only in production mode
    if context.get("mode") != "production":
        return False

    # Check if database supports it
    db = context.get("db")
    if not db or not hasattr(db, "find_one_raw_json"):
        return False

    # Check query complexity if provided
    if query_complexity and query_complexity > 100:
        return False

    return True
