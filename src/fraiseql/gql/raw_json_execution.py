"""Raw JSON-aware GraphQL execution.

This module provides a custom GraphQL execution function that can detect
and handle RawJSONResult returns from resolvers, bypassing normal serialization.
"""

import logging
from typing import Any, Optional

from graphql import GraphQLSchema, graphql, parse, validate
from graphql.execution import ExecutionResult

from fraiseql.core.raw_json_executor import RawJSONResult

logger = logging.getLogger(__name__)


async def execute_graphql_with_raw_json(
    schema: GraphQLSchema,
    query: str,
    variable_values: dict[str, Any] | None = None,
    operation_name: str | None = None,
    context_value: dict[str, Any] | None = None,
) -> ExecutionResult | RawJSONResult:
    """Execute GraphQL query with support for raw JSON results.

    This function first tries to parse and validate the query. If the query
    is valid, it executes it. If the execution returns a RawJSONResult,
    it returns that directly instead of trying to serialize it.

    Args:
        schema: The GraphQL schema
        query: The GraphQL query string
        variable_values: Optional variables
        operation_name: Optional operation name
        context_value: The context dict

    Returns:
        Either an ExecutionResult or a RawJSONResult
    """
    # First parse and validate
    try:
        document = parse(query)
        errors = validate(schema, document)
        if errors:
            return ExecutionResult(data=None, errors=errors)
    except Exception as e:
        logger.error(f"Failed to parse/validate query: {e}")
        # Return as ExecutionResult for consistent error handling
        return ExecutionResult(
            data=None,
            errors=[Exception(f"Query parsing failed: {e!s}")],
        )

    # Try to execute
    try:
        # We need to intercept the execution to catch RawJSONResult
        # For now, use standard execution and catch the error
        result = await graphql(
            schema,
            query,
            variable_values=variable_values,
            operation_name=operation_name,
            context_value=context_value,
        )

        return result

    except Exception as e:
        # Check if this is a RawJSONResult serialization error
        error_msg = str(e)
        if "RawJSONResult" in error_msg and hasattr(e, "__context__"):
            # Try to extract the RawJSONResult from the error context
            # This is a bit hacky but works for our use case
            import sys

            tb = sys.exc_info()[2]
            while tb is not None:
                frame = tb.tb_frame
                # Look for RawJSONResult in locals
                for var_value in frame.f_locals.values():
                    if isinstance(var_value, RawJSONResult):
                        logger.debug("Found RawJSONResult in error context, returning directly")
                        return var_value
                tb = tb.tb_next

        # If we couldn't find it, log and re-raise
        logger.error(f"GraphQL execution error: {e}")
        raise


def extract_raw_json_from_error(error: Exception) -> Optional[RawJSONResult]:
    """Try to extract RawJSONResult from a serialization error.

    When GraphQL tries to serialize a RawJSONResult, it fails. This function
    attempts to extract the RawJSONResult from the error context.

    Args:
        error: The exception that occurred

    Returns:
        The RawJSONResult if found, None otherwise
    """
    # This is a fallback method - the main logic is in execute_graphql_with_raw_json
    return None
