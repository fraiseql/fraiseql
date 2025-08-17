"""Unified adaptive GraphQL router for all environments."""

import json
import logging
from collections.abc import Awaitable, Callable
from typing import Any
from uuid import uuid4

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import HTMLResponse
from graphql import GraphQLSchema
from pydantic import BaseModel

from fraiseql.analysis.query_analyzer import QueryAnalyzer
from fraiseql.auth.base import AuthProvider
from fraiseql.core.raw_json_executor import RawJSONResult
from fraiseql.execution.mode_selector import ModeSelector
from fraiseql.execution.unified_executor import UnifiedExecutor
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.custom_response import RawJSONResponse
from fraiseql.fastapi.dependencies import build_graphql_context
from fraiseql.fastapi.json_encoder import FraiseQLJSONResponse, clean_unset_values
from fraiseql.fastapi.turbo import TurboRegistry, TurboRouter
from fraiseql.graphql import execute_with_passthrough_check
from fraiseql.optimization.n_plus_one_detector import (
    N1QueryDetectedError,
    configure_detector,
    n1_detection_context,
)

logger = logging.getLogger(__name__)

# Module-level dependency singletons to avoid B008
_default_context_dependency = Depends(build_graphql_context)


class GraphQLRequest(BaseModel):
    """GraphQL request model."""

    query: str
    variables: dict[str, Any] | None = None
    operationName: str | None = None  # noqa: N815 - GraphQL spec requires this name


def create_graphql_router(
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
    context_getter: Callable[[Request], Awaitable[dict[str, Any]]] | None = None,
    turbo_registry: TurboRegistry | None = None,
) -> APIRouter:
    """Create unified adaptive GraphQL router.

    This router adapts its behavior based on configuration and runtime headers,
    providing appropriate features for each environment while maintaining a
    single code path.

    Args:
        schema: GraphQL schema
        config: FraiseQL configuration
        auth_provider: Optional auth provider
        context_getter: Optional custom context getter
        turbo_registry: Optional TurboRouter registry

    Returns:
        Configured router
    """
    router = APIRouter(prefix="", tags=["GraphQL"])

    # Determine base behavior from environment
    is_production_env = config.environment == "production"
    logger.info(
        f"Creating unified GraphQL router: environment={config.environment}, "
        f"turbo_enabled={turbo_registry is not None}, "
        f"turbo_registry_type={type(turbo_registry).__name__}"
    )

    # Configure N+1 detection for non-production environments
    if not is_production_env:
        from fraiseql.optimization.n_plus_one_detector import get_detector

        detector = get_detector()
        if not hasattr(detector, "_configured"):
            configure_detector(
                threshold=10,  # Warn after 10 similar queries
                time_window=1.0,  # Within 1 second
                enabled=True,
                raise_on_detection=False,  # Just warn, don't raise
            )
            detector._configured = True

    # Always create unified execution components
    turbo_router = None
    if turbo_registry is not None:
        try:
            logger.info(f"Creating TurboRouter with registry: {turbo_registry}")
            turbo_router = TurboRouter(turbo_registry)
            logger.info(f"TurboRouter created successfully: {turbo_router}")
        except Exception:
            logger.exception("Failed to create TurboRouter")

    logger.info(
        f"TurboRouter creation final state: turbo_registry={turbo_registry is not None}, "
        f"turbo_router={turbo_router is not None}, turbo_router_value={turbo_router}"
    )
    query_analyzer = QueryAnalyzer(schema)
    mode_selector = ModeSelector(config)

    # Create unified executor
    unified_executor = None
    if getattr(config, "unified_executor_enabled", True):
        unified_executor = UnifiedExecutor(
            schema=schema,
            mode_selector=mode_selector,
            turbo_router=turbo_router,
            query_analyzer=query_analyzer,
        )
        logger.info(
            "Created UnifiedExecutor: has_turbo=%s, environment=%s",
            turbo_router is not None,
            config.environment,
        )

    # Create context dependency
    if context_getter:
        # Merge custom context with default
        async def get_merged_context(
            http_request: Request,
            default_context: dict[str, Any] = _default_context_dependency,
        ) -> dict[str, Any]:
            user = default_context.get("user")
            # Try to pass user as second argument if context_getter accepts it
            import inspect

            sig = inspect.signature(context_getter)
            if len(sig.parameters) >= 2:
                custom_context = await context_getter(http_request, user)
            else:
                custom_context = await context_getter(http_request)
            # Merge with default context (custom values override defaults)
            return {**default_context, **custom_context}

        context_dependency = Depends(get_merged_context)
    else:
        context_dependency = Depends(build_graphql_context)

    @router.post("/graphql", response_class=FraiseQLJSONResponse)
    async def graphql_endpoint(
        request: GraphQLRequest,
        http_request: Request,
        context: dict[str, Any] = context_dependency,
    ):
        """Execute GraphQL query with adaptive behavior."""
        # Check authentication if required
        if (
            config.auth_enabled
            and auth_provider
            and not context.get("authenticated", False)
            and not (config.environment == "development" and "__schema" in request.query)
        ):
            # Return 401 for unauthenticated requests when auth is required
            raise HTTPException(status_code=401, detail="Authentication required")

        try:
            # Determine execution mode from headers and config
            mode = config.environment
            json_passthrough = False

            # Check for mode headers
            if "x-mode" in http_request.headers:
                mode = http_request.headers["x-mode"].lower()
                context["mode"] = mode

                # Enable passthrough for production/staging modes if configured
                if mode in ("production", "staging"):  # noqa: SIM102
                    # Respect json_passthrough configuration settings
                    if config.json_passthrough_enabled and getattr(
                        config, "json_passthrough_in_production", True
                    ):
                        json_passthrough = True
            else:
                # Use environment as default mode
                context["mode"] = mode
                if is_production_env:  # noqa: SIM102
                    # Respect json_passthrough configuration settings
                    if config.json_passthrough_enabled and getattr(
                        config, "json_passthrough_in_production", True
                    ):
                        json_passthrough = True

            # Check for explicit passthrough header
            if "x-json-passthrough" in http_request.headers:
                json_passthrough = http_request.headers["x-json-passthrough"].lower() == "true"

            # Set passthrough flags in context
            if json_passthrough:
                context["execution_mode"] = "passthrough"
                context["json_passthrough"] = True

                # Update repository context if available
                if "db" in context:
                    context["db"].context["mode"] = mode
                    context["db"].context["json_passthrough"] = True
                    context["db"].mode = mode

            # Use unified executor if available
            if unified_executor:
                # Add execution metadata if in development
                if not is_production_env:
                    context["include_execution_metadata"] = True

                result = await unified_executor.execute(
                    query=request.query,
                    variables=request.variables,
                    operation_name=request.operationName,
                    context=context,
                )

                # Check if result is RawJSONResult
                if isinstance(result, RawJSONResult):
                    return RawJSONResponse(
                        content=result.json_string,
                        media_type=result.content_type,
                    )

                return result

            # Fallback to standard execution
            # Generate unique request ID for N+1 detection
            request_id = str(uuid4())

            # Execute with N+1 detection in non-production
            if not is_production_env:
                async with n1_detection_context(request_id) as detector:
                    context["n1_detector"] = detector
                    result = await execute_with_passthrough_check(
                        schema,
                        request.query,
                        context_value=context,
                        variable_values=request.variables,
                        operation_name=request.operationName,
                        enable_introspection=config.enable_introspection,
                    )
            else:
                result = await execute_with_passthrough_check(
                    schema,
                    request.query,
                    context_value=context,
                    variable_values=request.variables,
                    operation_name=request.operationName,
                    enable_introspection=config.enable_introspection,
                )

            # Check if result contains RawJSONResult
            if isinstance(result.data, RawJSONResult):
                return RawJSONResponse(
                    content=result.data.json_string,
                    media_type=result.data.content_type,
                )

            if result.data and isinstance(result.data, dict):
                for value in result.data.values():
                    if isinstance(value, RawJSONResult):
                        return RawJSONResponse(
                            content=value.json_string,
                            media_type=value.content_type,
                        )

            # Build response
            response: dict[str, Any] = {}
            if result.data is not None:
                response["data"] = result.data
            if result.errors:
                response["errors"] = [
                    _format_error(error, is_production_env) for error in result.errors
                ]

            return response

        except N1QueryDetectedError as e:
            # N+1 query pattern detected (only in development)
            return {
                "errors": [
                    {
                        "message": str(e),
                        "extensions": clean_unset_values(
                            {
                                "code": "N1_QUERY_DETECTED",
                                "patterns": [
                                    {
                                        "field": p.field_name,
                                        "type": p.parent_type,
                                        "count": p.count,
                                    }
                                    for p in e.patterns
                                ],
                            },
                        ),
                    },
                ],
            }
        except Exception as e:
            # Format error based on environment
            logger.exception("GraphQL execution error")

            if is_production_env:
                # Minimal error info in production
                return {
                    "errors": [
                        {
                            "message": "Internal server error",
                            "extensions": {"code": "INTERNAL_SERVER_ERROR"},
                        },
                    ],
                }
            # Detailed error info in development
            return {
                "errors": [
                    {
                        "message": str(e),
                        "extensions": clean_unset_values(
                            {
                                "code": "INTERNAL_SERVER_ERROR",
                                "exception": type(e).__name__,
                            },
                        ),
                    },
                ],
            }

    @router.get("/graphql")
    async def graphql_get_endpoint(
        query: str | None = None,
        http_request: Request = None,
        variables: str | None = None,
        operationName: str | None = None,  # noqa: N803
        context: dict[str, Any] = context_dependency,
    ):
        """Handle GraphQL GET requests."""
        # Only allow in non-production or if explicitly enabled
        if is_production_env and not config.enable_playground:
            raise HTTPException(404, "Not found")

        # If no query and playground enabled, serve it
        if query is None and config.enable_playground:
            if config.playground_tool == "apollo-sandbox":
                return HTMLResponse(content=APOLLO_SANDBOX_HTML)
            return HTMLResponse(content=GRAPHIQL_HTML)

        # If no query and playground disabled, error
        if query is None:
            raise HTTPException(400, "Query parameter is required")

        # Parse variables
        parsed_variables = None
        if variables:
            try:
                parsed_variables = json.loads(variables)
            except json.JSONDecodeError as e:
                raise HTTPException(400, "Invalid JSON in variables parameter") from e

        request_obj = GraphQLRequest(
            query=query,
            variables=parsed_variables,
            operationName=operationName,
        )

        return await graphql_endpoint(request_obj, http_request, context)

    # Add metrics endpoint if enabled
    if hasattr(unified_executor, "get_metrics") and not is_production_env:

        @router.get("/graphql/metrics")
        async def metrics_endpoint():
            """Get execution metrics."""
            return unified_executor.get_metrics()

    # Store turbo_registry for access by lifespan
    if turbo_registry is not None:
        router.turbo_registry = turbo_registry

    return router


def _format_error(error, is_production: bool) -> dict[str, Any]:
    """Format GraphQL error based on environment."""
    if is_production:
        # Minimal info in production
        return {
            "message": "Internal server error",
            "extensions": {"code": "INTERNAL_SERVER_ERROR"},
        }

    # Full details in development
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


# GraphiQL 2.0 HTML
GRAPHIQL_HTML = """
<!DOCTYPE html>
<html>
<head>
    <title>FraiseQL GraphiQL</title>
    <style>
        body {
            height: 100%;
            margin: 0;
            width: 100%;
            overflow: hidden;
        }
        #graphiql {
            height: 100vh;
        }
    </style>
    <script
        crossorigin
        src="https://unpkg.com/react@18/umd/react.production.min.js"
    ></script>
    <script
        crossorigin
        src="https://unpkg.com/react-dom@18/umd/react-dom.production.min.js"
    ></script>
    <link rel="stylesheet" href="https://unpkg.com/graphiql/graphiql.min.css" />
</head>
<body>
    <div id="graphiql">Loading...</div>
    <script
        src="https://unpkg.com/graphiql/graphiql.min.js"
        type="application/javascript"
    ></script>
    <script>
        ReactDOM.render(
            React.createElement(GraphiQL, {
                fetcher: GraphiQL.createFetcher({
                    url: '/graphql',
                    headers: {
                        'Accept': 'application/json',
                        'Content-Type': 'application/json',
                    },
                }),
                defaultEditorToolsVisibility: true,
            }),
            document.getElementById('graphiql'),
        );
    </script>
</body>
</html>
"""

# Apollo Sandbox HTML
APOLLO_SANDBOX_HTML = """
<!DOCTYPE html>
<html>
<head>
    <title>FraiseQL Apollo Sandbox</title>
    <style>
        body {
            margin: 0;
            overflow: hidden;
        }
        #sandbox {
            height: 100vh;
            width: 100vw;
        }
    </style>
</head>
<body>
    <div id="sandbox"></div>
    <script src="https://embeddable-sandbox.cdn.apollographql.com/_latest/embeddable-sandbox.umd.production.min.js"></script>
    <script>
        new window.EmbeddedSandbox({
            target: '#sandbox',
            initialEndpoint: '/graphql',
            includeCookies: true,
        });
    </script>
</body>
</html>
"""
