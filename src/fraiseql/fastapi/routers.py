"""Unified adaptive GraphQL router for all environments."""

import json
import logging
from collections.abc import Awaitable, Callable
from typing import Any
from uuid import uuid4

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import HTMLResponse
from graphql import GraphQLSchema
from pydantic import BaseModel, field_validator

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
    """GraphQL request model supporting Apollo Automatic Persisted Queries (APQ)."""

    query: str | None = None
    variables: dict[str, Any] | None = None
    operationName: str | None = None  # noqa: N815 - GraphQL spec requires this name
    extensions: dict[str, Any] | None = None

    @field_validator("extensions")
    @classmethod
    def validate_extensions(cls, v: dict[str, Any] | None) -> dict[str, Any] | None:
        """Validate extensions field structure for APQ compliance."""
        if v is None:
            return v

        # If extensions contains persistedQuery, validate APQ structure
        if "persistedQuery" in v:
            persisted_query = v["persistedQuery"]
            if not isinstance(persisted_query, dict):
                raise ValueError("persistedQuery must be an object")

            # APQ requires version and sha256Hash
            if "version" not in persisted_query:
                raise ValueError("persistedQuery.version is required")
            if "sha256Hash" not in persisted_query:
                raise ValueError("persistedQuery.sha256Hash is required")

            # Version must be 1 (APQ v1)
            if persisted_query["version"] != 1:
                raise ValueError("Only APQ version 1 is supported")

            # sha256Hash must be a non-empty string
            sha256_hash = persisted_query["sha256Hash"]
            if not isinstance(sha256_hash, str) or not sha256_hash:
                raise ValueError("persistedQuery.sha256Hash must be a non-empty string")

        return v


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
        # Check authentication first (before APQ processing to ensure security)
        # For APQ requests, we need to check auth regardless of query availability
        if (
            config.auth_enabled
            and auth_provider
            and not context.get("authenticated", False)
            and not (
                config.environment == "development"
                and request.query
                and "__schema" in request.query
            )
        ):
            # Return 401 for unauthenticated requests when auth is required
            raise HTTPException(status_code=401, detail="Authentication required")

        # Initialize APQ backend for potential caching
        apq_backend = None
        is_apq_request = request.extensions and "persistedQuery" in request.extensions

        # Handle APQ (Automatic Persisted Queries) if detected
        if is_apq_request:
            from fraiseql.middleware.apq import create_apq_error_response, get_persisted_query
            from fraiseql.middleware.apq_caching import (
                get_apq_backend,
                handle_apq_request_with_cache,
            )
            from fraiseql.storage.apq_store import store_persisted_query

            logger.debug("APQ request detected, processing...")

            persisted_query = request.extensions["persistedQuery"]
            sha256_hash = persisted_query.get("sha256Hash")

            # Validate hash format
            if not sha256_hash or not isinstance(sha256_hash, str) or not sha256_hash.strip():
                logger.debug("APQ request failed: invalid hash format")
                return create_apq_error_response(
                    "PERSISTED_QUERY_NOT_FOUND", "PersistedQueryNotFound"
                )

            # Get APQ backend for caching
            apq_backend = get_apq_backend(config)

            # Check if this is a registration request (has both hash and query)
            if request.query:
                # This is a registration request - store the query
                logger.debug(f"APQ registration: storing query with hash {sha256_hash[:8]}...")

                # Store in the global store (for backward compatibility)
                store_persisted_query(sha256_hash, request.query)

                # Also store in the backend if available
                if apq_backend:
                    apq_backend.store_persisted_query(sha256_hash, request.query)

                # Continue with normal execution using the provided query
                # The response will be cached after execution (see lines 361-370)

            else:
                # This is a hash-only request - try to retrieve the query

                # 1. Try cached response first (JSON passthrough)
                cached_response = handle_apq_request_with_cache(
                    request, apq_backend, config, context=context
                )
                if cached_response:
                    logger.debug(f"APQ cache hit: {sha256_hash[:8]}...")
                    return cached_response

                # 2. Fallback to query resolution from backend
                persisted_query_text = None

                # Try backend first
                if apq_backend:
                    persisted_query_text = apq_backend.get_persisted_query(sha256_hash)

                # Fallback to global store
                if not persisted_query_text:
                    persisted_query_text = get_persisted_query(sha256_hash)

                if not persisted_query_text:
                    logger.debug(f"APQ request failed: hash not found: {sha256_hash[:8]}...")
                    return create_apq_error_response(
                        "PERSISTED_QUERY_NOT_FOUND", "PersistedQueryNotFound"
                    )

                # Replace request query with persisted query for normal execution
                logger.debug(
                    f"APQ request resolved: hash {sha256_hash[:8]}... -> "
                    f"query length {len(persisted_query_text)}"
                )
                request.query = persisted_query_text

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

            # Cache response for APQ if it was an APQ request and response is cacheable
            if is_apq_request and apq_backend:
                from fraiseql.middleware.apq_caching import (
                    get_apq_hash_from_request,
                    store_response_in_cache,
                )

                apq_hash = get_apq_hash_from_request(request)
                if apq_hash:
                    # Store the response in cache for future requests
                    store_response_in_cache(
                        apq_hash, response, apq_backend, config, context=context
                    )

                    # Also store the cached response in the backend
                    import json

                    response_json = json.dumps(response, separators=(",", ":"))
                    apq_backend.store_cached_response(apq_hash, response_json, context=context)

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
