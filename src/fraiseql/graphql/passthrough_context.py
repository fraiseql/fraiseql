"""Custom GraphQL execution context that supports JSON passthrough."""

import asyncio
import json
import logging
from typing import Any, List

from graphql import (
    ExecutionContext,
    FieldNode,
    GraphQLOutputType,
    GraphQLResolveInfo,
)
from graphql.pyutils import AwaitableOrValue, Path

from fraiseql.core.raw_json_executor import RawJSONResult

logger = logging.getLogger(__name__)


class PassthroughExecutionContext(ExecutionContext):
    """Custom execution context that intercepts RawJSONResult early."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self._in_passthrough_mode = False
        self._current_field_name = None
        logger.info("PassthroughExecutionContext initialized")

    def execute_field(
        self,
        parent_type,
        source,
        field_nodes,
        path,
    ) -> AwaitableOrValue[Any]:
        """Execute field with early RawJSONResult detection."""
        # Get field definition
        from graphql.execution.execute import get_field_def

        field_def = get_field_def(self.schema, parent_type, field_nodes[0])
        if not field_def:
            return None

        # Store current field name for passthrough detection
        field_name = field_nodes[0].name.value
        self._current_field_name = field_name

        # Check if context indicates passthrough mode
        context = getattr(self, "context_value", {})
        if isinstance(context, dict):
            self._in_passthrough_mode = (
                context.get("json_passthrough", False)
                or context.get("execution_mode") == "passthrough"
                or context.get("mode") in ("production", "staging")
                or
                # Also check if DB context has passthrough enabled
                (
                    context.get("db")
                    and hasattr(context["db"], "context")
                    and context["db"].context.get("json_passthrough", False)
                )
            )

            # If in passthrough mode, wrap the resolver
            if self._in_passthrough_mode and field_def.resolve:
                original_resolve = field_def.resolve

                async def passthrough_resolve(source, info, **kwargs):
                    # Execute original resolver
                    result = original_resolve(source, info, **kwargs)
                    if asyncio.iscoroutine(result):
                        result = await result

                    # Check if it's already RawJSONResult
                    if isinstance(result, RawJSONResult):
                        return result

                    # Convert dict/list results to RawJSONResult
                    if isinstance(result, (dict, list)):
                        logger.debug(
                            f"Converting {type(result).__name__} to RawJSONResult "
                            f"for field {field_name}"
                        )
                        graphql_response = {"data": {field_name: result}}
                        return RawJSONResult(json.dumps(graphql_response))

                    return result

                # Temporarily replace the resolver
                field_def.resolve = passthrough_resolve

        # Execute normally
        return super().execute_field(parent_type, source, field_nodes, path)

    def complete_value(
        self,
        return_type: GraphQLOutputType,
        field_nodes: List[FieldNode],
        info: GraphQLResolveInfo,
        path: Path,
        result: Any,
    ) -> AwaitableOrValue[Any]:
        """Complete value with early RawJSONResult detection.

        This method intercepts before GraphQL type validation,
        allowing RawJSONResult to bypass the type system entirely.
        """
        # Check for RawJSONResult BEFORE any type validation
        if isinstance(result, RawJSONResult):
            logger.info(
                f"Found RawJSONResult in complete_value for {info.field_name}, "
                f"passthrough_mode={self._in_passthrough_mode}"
            )
            # Return the RawJSONResult directly - it will be handled by the router
            return result

        # Check for our PassthroughResult marker
        from fraiseql.graphql.passthrough_type import PassthroughResult

        if isinstance(result, PassthroughResult):
            logger.debug(f"Found PassthroughResult in complete_value for {info.field_name}")
            # Convert to RawJSONResult
            return result.to_raw_json()

        # In passthrough mode, check for raw dicts/lists
        if self._in_passthrough_mode:
            # For dicts that look like GraphQL results, treat as raw JSON
            if isinstance(result, dict):
                logger.debug(
                    f"Converting dict to RawJSONResult in passthrough mode for {info.field_name}"
                )
                # Don't wrap - return the RawJSONResult directly
                # The router will handle the final JSON response
                return RawJSONResult(json.dumps(result))

            # For lists, also convert
            if isinstance(result, list):
                logger.debug(
                    f"Converting list to RawJSONResult in passthrough mode for {info.field_name}"
                )
                # Don't wrap - return the RawJSONResult directly
                return RawJSONResult(json.dumps(result))

        # Otherwise, proceed with normal GraphQL type validation
        return super().complete_value(return_type, field_nodes, info, path, result)

    def complete_object_value(
        self,
        return_type,
        field_nodes,
        info,
        path,
        result,
    ):
        """Complete object value with passthrough support.

        This is where GraphQL normally validates that the result
        matches the expected object type. We intercept here for passthrough.
        """
        # In passthrough mode, if we get a dict, bypass type checking
        if self._in_passthrough_mode and isinstance(result, dict):
            logger.debug("Bypassing type check for dict in passthrough mode")
            # Don't convert to RawJSONResult here - that happens at the field level
            # Instead, skip the is_type_of check and directly execute subfields
            sub_field_nodes = self.collect_subfields(return_type, field_nodes)

            # Execute fields with the dict as if it were a valid object
            return self.execute_fields(return_type, result, path, sub_field_nodes)

        # Otherwise use normal completion
        return super().complete_object_value(return_type, field_nodes, info, path, result)

    def complete_list_value(
        self,
        return_type,
        field_nodes,
        info,
        path,
        result,
    ):
        """Complete list value with passthrough support."""
        # In passthrough mode, handle list completion ourselves
        if self._in_passthrough_mode and isinstance(result, list):
            logger.debug(f"Handling list in passthrough mode, length: {len(result)}")

            # Get the inner type
            item_type = return_type.of_type
            completed_results = []

            # Complete each item
            for index, item in enumerate(result):
                item_path = path.add_key(index, None)

                # For dicts in passthrough mode, we need to handle them specially
                if isinstance(item, dict):
                    # Skip the normal object type validation
                    completed_item = item
                else:
                    # Use normal completion for non-dict items
                    completed_item = self.complete_value(
                        item_type, field_nodes, info, item_path, item
                    )

                completed_results.append(completed_item)

            return completed_results

        # Otherwise use normal completion
        return super().complete_list_value(return_type, field_nodes, info, path, result)
