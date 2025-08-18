"""Unified query executor with mode switching."""

import json
import time
from typing import Any, Dict, Optional

from graphql import GraphQLSchema

from fraiseql.analysis.query_analyzer import QueryAnalyzer
from fraiseql.core.raw_json_executor import RawJSONResult
from fraiseql.execution.mode_selector import ExecutionMode, ModeSelector
from fraiseql.fastapi.json_encoder import clean_unset_values
from fraiseql.fastapi.raw_json_handler import try_raw_json_execution_enhanced
from fraiseql.fastapi.turbo import TurboRouter
from fraiseql.graphql.execute import execute_with_passthrough_check


class UnifiedExecutor:
    """Unified executor for all query execution modes."""

    def __init__(
        self,
        schema: GraphQLSchema,
        mode_selector: ModeSelector,
        turbo_router: Optional[TurboRouter] = None,
        query_analyzer: Optional[QueryAnalyzer] = None,
    ):
        """Initialize unified executor.

        Args:
            schema: GraphQL schema
            mode_selector: Mode selection logic
            turbo_router: Optional TurboRouter instance
            query_analyzer: Optional QueryAnalyzer instance
        """
        self.schema = schema
        self.mode_selector = mode_selector
        self.turbo_router = turbo_router
        self.query_analyzer = query_analyzer or QueryAnalyzer(schema)

        # Set dependencies in mode selector
        if turbo_router:
            mode_selector.set_turbo_registry(turbo_router.registry)
        mode_selector.set_query_analyzer(self.query_analyzer)

        # Metrics
        self._execution_counts = {
            ExecutionMode.TURBO: 0,
            ExecutionMode.PASSTHROUGH: 0,
            ExecutionMode.NORMAL: 0,
        }
        self._execution_times = {
            ExecutionMode.TURBO: [],
            ExecutionMode.PASSTHROUGH: [],
            ExecutionMode.NORMAL: [],
        }

    async def execute(
        self,
        query: str,
        variables: Optional[Dict[str, Any]] = None,
        operation_name: Optional[str] = None,
        context: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Execute query using optimal mode.

        Args:
            query: GraphQL query string
            variables: Query variables
            operation_name: Operation name for multi-operation documents
            context: Request context

        Returns:
            GraphQL response dictionary
        """
        if context is None:
            context = {}

        if variables is None:
            variables = {}

        start_time = time.time()

        # Select execution mode
        mode = self.mode_selector.select_mode(query, variables, context)

        # Track mode in context for debugging
        context["execution_mode"] = mode.value

        # Execute based on mode
        try:
            if mode == ExecutionMode.TURBO:
                result = await self._execute_turbo(query, variables, context)
            elif mode == ExecutionMode.PASSTHROUGH:
                result = await self._execute_passthrough(query, variables, context)
            else:
                result = await self._execute_normal(query, variables, operation_name, context)

            # Track metrics
            execution_time = time.time() - start_time
            self._track_execution(mode, execution_time)

            # Add execution metadata if requested
            if context.get("include_execution_metadata"):
                if "extensions" not in result:
                    result["extensions"] = {}

                result["extensions"]["execution"] = {
                    "mode": mode.value,
                    "time_ms": round(execution_time * 1000, 2),
                }

            return result

        except Exception as e:
            # Log error with mode information
            import logging

            logger = logging.getLogger(__name__)
            logger.exception(f"Query execution failed in mode {mode.value}")

            # Return error response
            return {
                "errors": [
                    {
                        "message": str(e),
                        "extensions": {
                            "code": "EXECUTION_ERROR",
                            "mode": mode.value,
                        },
                    }
                ]
            }

    async def _execute_turbo(
        self, query: str, variables: Dict[str, Any], context: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Execute via TurboRouter.

        Args:
            query: GraphQL query
            variables: Query variables
            context: Request context

        Returns:
            GraphQL response
        """
        if not self.turbo_router:
            # Fallback to normal if no turbo router
            return await self._execute_normal(query, variables, None, context)

        result = await self.turbo_router.execute(query, variables, context)

        if result is None:
            # Query not in registry, fallback to normal
            return await self._execute_normal(query, variables, None, context)

        return result

    async def _execute_passthrough(
        self, query: str, variables: Dict[str, Any], context: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Execute via raw JSON passthrough.

        Args:
            query: GraphQL query
            variables: Query variables
            context: Request context

        Returns:
            GraphQL response
        """
        # Ensure we're in production mode for passthrough
        context["mode"] = "production"

        # Try enhanced raw JSON execution
        result = await try_raw_json_execution_enhanced(
            self.schema, query, variables, context, self.query_analyzer
        )

        if result is not None:
            # Return RawJSONResult directly - the router will handle it
            return result

        # Fallback to normal execution
        return await self._execute_normal(query, variables, None, context)

    async def _execute_normal(
        self,
        query: str,
        variables: Dict[str, Any],
        operation_name: Optional[str],
        context: Dict[str, Any],
    ) -> Dict[str, Any]:
        """Execute via standard GraphQL.

        Args:
            query: GraphQL query
            variables: Query variables
            operation_name: Operation name
            context: Request context

        Returns:
            GraphQL response
        """
        # Execute query with passthrough support
        result = await execute_with_passthrough_check(
            self.schema,
            query,
            context_value=context,
            variable_values=variables,
            operation_name=operation_name,
            enable_introspection=getattr(self.mode_selector.config, "enable_introspection", True),
        )

        # Check if the entire result.data is RawJSONResult
        if isinstance(result.data, RawJSONResult):
            # This means execute_with_passthrough_check returned RawJSONResult
            return result.data

        # Check if result contains RawJSONResult in fields
        if result.data and isinstance(result.data, dict):
            for key, value in result.data.items():
                if isinstance(value, RawJSONResult):
                    # The RawJSONResult should contain the complete GraphQL response
                    # But due to double-wrapping in execute.py, we need to handle it carefully
                    try:
                        # Parse to check if it's already a complete response
                        parsed = json.loads(value.json_string)
                        if isinstance(parsed, dict) and "data" in parsed:
                            # It's already a complete GraphQL response
                            return value
                        # It's just the field data, wrap it
                        graphql_response = {"data": {key: parsed}}
                        return RawJSONResult(json.dumps(graphql_response))
                    except Exception:
                        # If parsing fails, return as-is and let router handle it
                        return value

        # Build standard response
        response: Dict[str, Any] = {}

        if result.data is not None:
            response["data"] = result.data

        if result.errors:
            response["errors"] = [self._format_error(error) for error in result.errors]

        return response

    def _format_error(self, error) -> Dict[str, Any]:
        """Format GraphQL error for response.

        Args:
            error: GraphQL error

        Returns:
            Formatted error dictionary
        """
        formatted = {
            "message": error.message,
        }

        if error.locations:
            formatted["locations"] = [
                {"line": loc.line, "column": loc.column} for loc in error.locations
            ]

        if error.path:
            formatted["path"] = error.path

        if error.extensions:
            formatted["extensions"] = clean_unset_values(error.extensions)

        return formatted

    def _track_execution(self, mode: ExecutionMode, execution_time: float):
        """Track execution metrics.

        Args:
            mode: Execution mode used
            execution_time: Time taken in seconds
        """
        self._execution_counts[mode] += 1

        # Keep last 100 execution times
        times = self._execution_times[mode]
        times.append(execution_time)
        if len(times) > 100:
            times.pop(0)

    def get_metrics(self) -> Dict[str, Any]:
        """Get execution metrics.

        Returns:
            Dictionary of metrics
        """
        metrics = {
            "execution_counts": {
                mode.value: count for mode, count in self._execution_counts.items()
            },
            "average_execution_times": {},
            "mode_selector_metrics": self.mode_selector.get_mode_metrics(),
        }

        # Calculate average execution times
        for mode, times in self._execution_times.items():
            if times:
                avg_time = sum(times) / len(times)
                metrics["average_execution_times"][mode.value] = round(avg_time * 1000, 2)
            else:
                metrics["average_execution_times"][mode.value] = 0

        # Add cache metrics if available
        if self.turbo_router and hasattr(self.turbo_router.registry, "get_metrics"):
            metrics["turbo_cache_metrics"] = self.turbo_router.registry.get_metrics()

        return metrics
