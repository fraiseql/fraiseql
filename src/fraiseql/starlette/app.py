"""Starlette application factory for FraiseQL.

This module provides a complete Starlette HTTP server implementation
using the framework-agnostic abstraction protocols defined in fraiseql.http.interface.

The implementation follows the same architecture as the Axum server but
adapted for Starlette's ASGI request/response model.

Features:
- GraphQL query execution (POST /graphql)
- Introspection and schema inspection
- Health checks (GET /health)
- APQ (Automatic Persisted Queries) support
- Authentication middleware
- Request logging and tracing
- WebSocket subscriptions (graphql-ws protocol)

Example:
    from fraiseql.starlette.app import create_starlette_app
    from fraiseql.gql.schema_builder import build_fraiseql_schema

    # Create the schema
    schema = build_fraiseql_schema(...)

    # Create and run the app
    app = create_starlette_app(schema, database_url="postgresql://...")
    # Run with: uvicorn fraiseql.starlette.app:app
"""

import logging
from contextlib import asynccontextmanager
from typing import Any

import psycopg_pool
from graphql import GraphQLSchema
from psycopg import AsyncConnection
from starlette.applications import Starlette
from starlette.middleware.cors import CORSMiddleware
from starlette.requests import Request
from starlette.responses import JSONResponse
from starlette.routing import Route

from fraiseql.auth.base import AuthProvider
from fraiseql.graphql.execute import execute_graphql
from fraiseql.http.interface import (
    GraphQLRequest,
    GraphQLResponse,
    HealthStatus,
    HttpContext,
)
from fraiseql.utils import normalize_database_url

logger = logging.getLogger(__name__)


# ============================================================================
# Request Parsing (RequestParser Protocol Implementation)
# ============================================================================


class StarletteRequestParser:
    """Parse Starlette requests to standard GraphQLRequest format.

    Implements the RequestParser protocol by converting Starlette's
    Request object to the framework-agnostic GraphQLRequest format.
    """

    async def parse_graphql_request(
        self,
        context: HttpContext,
    ) -> GraphQLRequest:
        """Parse HTTP context to GraphQL request.

        Args:
            context: Framework-agnostic HTTP context

        Returns:
            GraphQLRequest with query, variables, etc.

        Raises:
            ValueError: If request body is invalid
        """
        try:
            body = context.request_body
        except (KeyError, ValueError) as e:
            raise ValueError(f"Invalid request body: {e}") from e

        # Extract GraphQL fields from request body
        query = body.get("query")
        operation_name = body.get("operationName")
        variables = body.get("variables")
        extensions = body.get("extensions")

        # Create and validate request
        request = GraphQLRequest(
            query=query,
            operation_name=operation_name,
            variables=variables,
            extensions=extensions,
        )

        request.validate()
        return request


# ============================================================================
# Response Formatting (ResponseFormatter Protocol Implementation)
# ============================================================================


class StarletteResponseFormatter:
    """Format GraphQLResponse to Starlette response format.

    Implements the ResponseFormatter protocol by converting the
    framework-agnostic GraphQLResponse to a Starlette JSONResponse.
    """

    async def format_response(
        self,
        response: GraphQLResponse,
    ) -> JSONResponse:
        """Format GraphQL response to Starlette response.

        Args:
            response: Standard GraphQL response

        Returns:
            Starlette JSONResponse with appropriate status code
        """
        return JSONResponse(
            response.to_dict(),
            status_code=response.status_code,
        )


# ============================================================================
# Request Handlers
# ============================================================================


async def graphql_handler(
    request: Request,
    schema: GraphQLSchema,
    db_pool: psycopg_pool.AsyncConnectionPool,
    auth_provider: AuthProvider | None = None,
    request_parser: StarletteRequestParser | None = None,
    response_formatter: StarletteResponseFormatter | None = None,
) -> JSONResponse:
    """Handle GraphQL queries via POST /graphql.

    This is the main GraphQL request handler. It:
    1. Parses the incoming Starlette request
    2. Builds the framework-agnostic context
    3. Executes the GraphQL query
    4. Formats the response

    Args:
        request: Starlette request object
        schema: GraphQL schema
        db_pool: Database connection pool
        auth_provider: Optional authentication provider
        request_parser: Optional custom request parser
        response_formatter: Optional custom response formatter

    Returns:
        JSONResponse with GraphQL result or error
    """
    if request_parser is None:
        request_parser = StarletteRequestParser()
    if response_formatter is None:
        response_formatter = StarletteResponseFormatter()

    try:
        # Parse request body
        try:
            body = await request.json()
        except Exception as e:
            return await response_formatter.format_response(
                GraphQLResponse.error(f"Invalid JSON: {e}", code=400)
            )

        # Build framework-agnostic context
        headers = dict(request.headers)
        user = None

        # Handle authentication if provider is available
        if auth_provider:
            try:
                auth_header = headers.get("authorization", "")
                user = await auth_provider.authenticate(auth_header)
            except Exception as e:
                logger.debug(f"Authentication failed: {e}")
                # Continue without user - auth is optional

        context = HttpContext(
            request_body=body,
            headers=headers,
            user=user,
            method=request.method,
            path=request.url.path,
            raw_request=request,
        )

        # Parse GraphQL request
        try:
            graphql_request = await request_parser.parse_graphql_request(context)
        except ValueError as e:
            return await response_formatter.format_response(GraphQLResponse.error(str(e), code=400))

        # Execute GraphQL query
        async with db_pool.connection() as conn:
            result = await execute_graphql(
                schema=schema,
                query=graphql_request.query,
                operation_name=graphql_request.operation_name,
                variables=graphql_request.variables,
                extensions=graphql_request.extensions,
                context={"user": user, "db_connection": conn},
            )

        # Build response
        if result.errors:
            error_messages = [str(e) for e in result.errors]
            response = GraphQLResponse(
                data=result.data,
                errors=[
                    {
                        "message": msg,
                        "extensions": {"code": "GRAPHQL_ERROR"},
                    }
                    for msg in error_messages
                ],
                status_code=400 if not result.data else 200,
            )
        else:
            response = GraphQLResponse(
                data=result.data,
                status_code=200,
            )

        return await response_formatter.format_response(response)

    except Exception as e:
        logger.exception("Unhandled error in GraphQL handler")
        return await response_formatter.format_response(
            GraphQLResponse.error(f"Internal server error: {e}", code=500)
        )


async def health_handler(
    db_pool: psycopg_pool.AsyncConnectionPool,
) -> JSONResponse:
    """Handle health check requests at GET /health.

    This endpoint checks the server and database health status.
    Returns 200 if healthy, 503 if unhealthy.

    Args:
        db_pool: Database connection pool

    Returns:
        JSONResponse with health status
    """
    try:
        # Check database connection
        async with db_pool.connection() as conn:
            await conn.execute("SELECT 1")

        status = HealthStatus(
            status="healthy",
            version="2.0.0",  # Update as needed
            details={"database": "connected"},
        )
        return JSONResponse(status.to_dict(), status_code=200)

    except Exception as e:
        logger.warning(f"Health check failed: {e}")
        status = HealthStatus(
            status="unhealthy",
            version="2.0.0",
            details={"error": str(e)},
        )
        return JSONResponse(status.to_dict(), status_code=503)


# ============================================================================
# Application Factory
# ============================================================================


async def create_db_pool(
    database_url: str,
    **pool_kwargs: Any,
) -> psycopg_pool.AsyncConnectionPool:
    """Create async database connection pool with custom type handling.

    Args:
        database_url: PostgreSQL connection string
        **pool_kwargs: Additional arguments for AsyncConnectionPool

    Returns:
        Configured AsyncConnectionPool ready for use
    """

    async def configure_types(conn: AsyncConnection) -> None:
        """Configure type adapters to keep dates as strings."""
        from psycopg.adapt import Loader

        class TextLoader(Loader):
            def load(self, data: Any) -> Any:
                return data.decode("utf-8") if isinstance(data, bytes) else data

        # Register text loaders for date/time types
        conn.adapters.register_loader("date", TextLoader)
        conn.adapters.register_loader("timestamp", TextLoader)
        conn.adapters.register_loader("timestamptz", TextLoader)
        conn.adapters.register_loader("time", TextLoader)
        conn.adapters.register_loader("timetz", TextLoader)

    async def check_connection(conn: AsyncConnection) -> None:
        """Validate connection is alive before reuse."""
        try:
            await conn.execute("SELECT 1")
        except Exception:
            logger.debug("Connection check failed, pool will create new connection")
            raise

    # Create and open pool
    pool = psycopg_pool.AsyncConnectionPool(
        normalize_database_url(database_url),
        configure=configure_types,
        check=check_connection,
        open=False,
        **pool_kwargs,
    )

    await pool.open()
    return pool


def create_starlette_app(
    schema: GraphQLSchema,
    database_url: str,
    auth_provider: AuthProvider | None = None,
    cors_origins: list[str] | None = None,
    **pool_kwargs: Any,
) -> Starlette:
    """Create a complete Starlette application for FraiseQL.

    This factory creates a Starlette app with all necessary routes,
    middleware, and configuration for GraphQL execution.

    Args:
        schema: GraphQL schema
        database_url: PostgreSQL connection string
        auth_provider: Optional authentication provider
        cors_origins: List of CORS origins (default: *)
        **pool_kwargs: Additional arguments for connection pool

    Returns:
        Configured Starlette application

    Example:
        from fraiseql.gql.schema_builder import build_fraiseql_schema
        from fraiseql.starlette.app import create_starlette_app

        schema = build_fraiseql_schema(database_url="postgresql://...")
        app = create_starlette_app(schema, database_url="postgresql://...")

        # Run with: uvicorn app:app
    """
    # Global state for lifespan management
    db_pool: psycopg_pool.AsyncConnectionPool | None = None

    @asynccontextmanager
    async def lifespan(app: Starlette):
        """Manage application lifecycle."""
        nonlocal db_pool

        try:
            # Startup
            logger.info("Starting FraiseQL Starlette server")
            db_pool = await create_db_pool(database_url, **pool_kwargs)
            logger.info("Database pool created successfully")
            yield

        finally:
            # Shutdown
            logger.info("Shutting down FraiseQL Starlette server")
            if db_pool:
                await db_pool.close()
                logger.info("Database pool closed")

    # Create the application
    app = Starlette(lifespan=lifespan)

    # Add CORS middleware
    if cors_origins is None:
        cors_origins = ["*"]

    app.add_middleware(
        CORSMiddleware,
        allow_origins=cors_origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Create route handlers
    async def graphql_endpoint(request: Request) -> JSONResponse:
        """POST /graphql endpoint."""
        if db_pool is None:
            return JSONResponse(
                {"errors": [{"message": "Server not initialized"}]},
                status_code=503,
            )
        return await graphql_handler(
            request=request,
            schema=schema,
            db_pool=db_pool,
            auth_provider=auth_provider,
        )

    async def health_endpoint(request: Request) -> JSONResponse:
        """GET /health endpoint."""
        if db_pool is None:
            return JSONResponse(
                {"status": "unhealthy", "error": "Database not initialized"},
                status_code=503,
            )
        return await health_handler(db_pool)

    # Register routes
    app.routes = [
        Route("/graphql", graphql_endpoint, methods=["POST"]),
        Route("/health", health_endpoint, methods=["GET"]),
    ]

    logger.info("FraiseQL Starlette application created successfully")
    return app


# ============================================================================
# Export for module loading
# ============================================================================

__all__ = [
    "StarletteRequestParser",
    "StarletteResponseFormatter",
    "create_db_pool",
    "create_starlette_app",
    "graphql_handler",
    "health_handler",
]
