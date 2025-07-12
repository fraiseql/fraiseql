"""GraphQL routers for development and production environments."""

import json
from collections.abc import Awaitable, Callable
from typing import Any
from uuid import uuid4

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import HTMLResponse
from graphql import GraphQLSchema, graphql, parse, validate
from pydantic import BaseModel

from fraiseql.auth.base import AuthProvider
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import build_graphql_context
from fraiseql.fastapi.json_encoder import FraiseQLJSONResponse, clean_unset_values
from fraiseql.fastapi.turbo import TurboRegistry, TurboRouter
from fraiseql.optimization.n_plus_one_detector import (
    N1QueryDetectedError,
    configure_detector,
    n1_detection_context,
)

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
    """Create appropriate router based on environment."""
    if config.environment == "production":
        return create_production_router(
            schema,
            config,
            auth_provider,
            context_getter,
            turbo_registry,
        )
    return create_development_router(schema, config, auth_provider, context_getter)


def create_development_router(
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
    context_getter: Callable[[Request], Awaitable[dict[str, Any]]] | None = None,
) -> APIRouter:
    """Create development router with full GraphQL features."""
    router = APIRouter(prefix="", tags=["GraphQL"])

    # Configure N+1 detection for development mode
    # Only configure if not already configured (allows tests to override)
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

    # Create context dependency based on whether custom context_getter is provided
    if context_getter:
        # When custom context_getter is provided, merge it with the default context
        async def get_merged_context(
            http_request: Request,
            default_context: dict[str, Any] = _default_context_dependency,
        ) -> dict[str, Any]:
            # Get custom context
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
        """Execute GraphQL query with full validation and introspection."""
        try:
            # Generate unique request ID for N+1 detection
            request_id = str(uuid4())

            # Execute query with N+1 detection
            async with n1_detection_context(request_id) as detector:
                # Add detector to context for field resolvers
                context["n1_detector"] = detector

                # Execute query
                result = await graphql(
                    schema,
                    request.query,
                    variable_values=request.variables,
                    operation_name=request.operationName,
                    context_value=context,
                )

            # Build response
            response: dict[str, Any] = {}
            if result.data is not None:
                response["data"] = result.data
            if result.errors:
                response["errors"] = [
                    {
                        "message": error.message,
                        "locations": (
                            [{"line": loc.line, "column": loc.column} for loc in error.locations]
                            if error.locations
                            else None
                        ),
                        "path": error.path,
                        "extensions": (
                            clean_unset_values(error.extensions) if error.extensions else {}
                        ),
                    }
                    for error in result.errors
                ]

            return response  # noqa: TRY300

        except N1QueryDetectedError as e:
            # N+1 query pattern detected
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
            # In development, provide detailed error info
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
        operationName: str | None = None,  # noqa: N803 - GraphQL spec requires this name
        context: dict[str, Any] = context_dependency,
    ):
        """Handle GraphQL GET requests."""
        # If no query is provided and playground is enabled, serve the playground HTML
        if query is None and config.enable_playground:
            if config.playground_tool == "apollo-sandbox":
                return HTMLResponse(content=APOLLO_SANDBOX_HTML)
            # Default to GraphiQL
            return HTMLResponse(content=GRAPHIQL_HTML)

        # If no query is provided and playground is disabled, return an error
        if query is None:
            raise HTTPException(400, "Query parameter is required")

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

    if config.enable_introspection:
        # Introspection is handled by GraphQL itself when enabled
        pass

    return router


def create_production_router(
    schema: GraphQLSchema,
    config: FraiseQLConfig,
    auth_provider: AuthProvider | None = None,
    context_getter: Callable[[Request], Awaitable[dict[str, Any]]] | None = None,
    turbo_registry: TurboRegistry | None = None,
) -> APIRouter:
    """Create production router with optimizations.

    Features:
    - TurboRouter for registered queries
    - Optimized query execution
    - Minimal error information
    - No introspection
    - No playground
    """
    router = APIRouter(prefix="", tags=["GraphQL"])

    # Create TurboRouter if registry provided
    turbo_router = TurboRouter(turbo_registry) if turbo_registry else None

    # Create context dependency based on whether custom context_getter is provided
    if context_getter:
        # When custom context_getter is provided, merge it with the default context
        async def get_merged_context(
            http_request: Request,
            default_context: dict[str, Any] = _default_context_dependency,
        ) -> dict[str, Any]:
            # Get custom context
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
        """Execute GraphQL query in production mode."""
        try:
            # Try TurboRouter first for registered queries
            if turbo_router:
                turbo_result = await turbo_router.execute(
                    query=request.query,
                    variables=request.variables or {},
                    context=context,
                )
                if turbo_result is not None:
                    return turbo_result

            # Fall back to standard GraphQL execution
            # Parse and validate first to fail fast
            try:
                document = parse(request.query)
                errors = validate(schema, document)
                if errors:
                    return {
                        "errors": [
                            {
                                "message": error.message,
                                "extensions": (
                                    clean_unset_values(error.extensions)
                                    if error.extensions
                                    else {"code": "GRAPHQL_VALIDATION_FAILED"}
                                ),
                            }
                            for error in errors
                        ],
                    }
            except Exception:
                return {
                    "errors": [
                        {
                            "message": "Invalid query",
                            "extensions": {"code": "GRAPHQL_PARSE_FAILED"},
                        },
                    ],
                }

            # Execute query
            result = await graphql(
                schema,
                request.query,
                variable_values=request.variables,
                operation_name=request.operationName,
                context_value=context,
            )

            # Build response with minimal error info
            response: dict[str, Any] = {}
            if result.data is not None:
                response["data"] = result.data
            if result.errors:
                response["errors"] = [
                    {
                        "message": (
                            "Internal server error"
                            if config.get("hide_error_details", True)
                            else error.message
                        ),
                        "extensions": (
                            clean_unset_values(error.extensions)
                            if error.extensions
                            else {"code": "INTERNAL_SERVER_ERROR"}
                        ),
                    }
                    for error in result.errors
                ]

            return response

        except Exception as e:
            # In production, log the actual error for debugging but don't expose details to client
            error_msg = str(e)
            logger.exception("Production GraphQL execution error: %s", error_msg)

            # Special logging for UNSET serialization issues
            if "Unset is not JSON serializable" in error_msg:
                logger.error(
                    "UNSET serialization error in production mode. "
                    "This may be caused by UNSET values in JSONB data that weren't properly cleaned. "
                    "Query: %s, Variables: %s",
                    request.query[:200] if request.query else "None",
                    str(request.variables)[:200] if request.variables else "None",
                )

            return {
                "errors": [
                    {
                        "message": "Internal server error",
                        "extensions": {"code": "INTERNAL_SERVER_ERROR"},
                    },
                ],
            }

    # No GET endpoint in production
    # No playground in production
    # No introspection in production

    return router


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
