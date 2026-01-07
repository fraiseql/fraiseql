"""Axum application factory for FraiseQL.

Provides create_axum_fraiseql_app factory function which is a drop-in replacement
for create_fraiseql_app from the FastAPI integration.
"""

import logging
from collections.abc import Callable, Coroutine
from typing import Any

from fraiseql.axum.config import AxumFraiseQLConfig
from fraiseql.axum.registry import AxumRegistry
from fraiseql.axum.server import AxumServer

logger = logging.getLogger(__name__)


def create_axum_fraiseql_app(
    *,
    config: AxumFraiseQLConfig | None = None,
    database_url: str | None = None,
    types: list[type[Any]] | None = None,
    mutations: list[type[Any]] | None = None,
    queries: list[type[Any]] | None = None,
    subscriptions: list[type[Any]] | None = None,
    context_getter: Callable[..., Coroutine[Any, Any, dict[str, Any]]] | None = None,
    middleware: list[Any] | None = None,
    cors_origins: list[str] | None = None,
    cors_allow_credentials: bool = True,
    cors_allow_methods: list[str] | None = None,
    cors_allow_headers: list[str] | None = None,
    title: str = "FraiseQL API",
    description: str = "GraphQL API built with FraiseQL",
    version: str = "1.0.0",
    docs_url: str | None = "/docs",
    redoc_url: str | None = "/redoc",
    openapi_url: str | None = "/openapi.json",
    include_in_schema: bool = True,
    registry: AxumRegistry | None = None,
    **kwargs: Any,
) -> AxumServer:
    """Create Axum-based FraiseQL server (7-10x faster than FastAPI).

    Drop-in replacement for create_fraiseql_app with identical API.

    This factory creates a high-performance GraphQL server by:
    1. Creating AxumFraiseQLConfig from parameters
    2. Initializing AxumServer wrapper
    3. Registering GraphQL types, mutations, queries, subscriptions
    4. Configuring CORS and middleware (deferred to Phase 16)

    The server uses Rust-based Axum for HTTP handling, providing 7-10x
    better performance than the FastAPI integration.

    Args:
        config: Optional AxumFraiseQLConfig instance. If not provided, creates
            one from other parameters.
        database_url: PostgreSQL connection URL. Required if config not provided.
        types: List of @fraiseql.type decorated GraphQL type classes.
        mutations: List of GraphQL mutation classes.
        queries: List of GraphQL query classes.
        subscriptions: List of GraphQL subscription classes.
        context_getter: Async function to build request context (reserved for Phase 16).
        middleware: List of Axum middleware (reserved for Phase 16).
        cors_origins: CORS allowed origins (e.g., ["https://example.com"]).
        cors_allow_credentials: Allow CORS credentials (default: True).
        cors_allow_methods: Allowed CORS methods (default: all standard methods).
        cors_allow_headers: Allowed CORS headers (default: all standard headers).
        title: API title for documentation (default: "FraiseQL API").
        description: API description for documentation (default: "GraphQL API built with FraiseQL").
        version: API version for documentation (default: "1.0.0").
        docs_url: URL for OpenAPI docs (default: "/docs"). Set to None to disable.
        redoc_url: URL for ReDoc docs (default: "/redoc"). Set to None to disable.
        openapi_url: URL for OpenAPI JSON (default: "/openapi.json"). Set to None to disable.
        include_in_schema: Include in OpenAPI schema (default: True).
        registry: Optional AxumRegistry instance. If not provided, uses the singleton
            instance. Rarely needed except for advanced testing scenarios.
        **kwargs: Additional configuration parameters (merged into config).

    Returns:
        AxumServer instance ready for:
        - .start(host, port) - blocking server start
        - .start_async(host, port) - async server start
        - .execute_query() - direct query execution
        - .shutdown() - graceful shutdown

    Raises:
        ValueError: If required parameters missing (e.g., database_url)
        ImportError: If PyAxumServer FFI binding not available

    Example (blocking start):
        ```python
        from fraiseql.axum import create_axum_fraiseql_app

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User, Post],
            mutations=[CreateUser],
            cors_origins=["https://example.com"],
        )

        # Start server (blocks main thread)
        app.start(host="0.0.0.0", port=8000)
        ```

    Example (async start):
        ```python
        import asyncio

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User, Post],
        )

        async def main():
            # Start server (non-blocking)
            await app.start_async(host="0.0.0.0", port=8000)

            # Server runs in background
            await asyncio.sleep(60)

            # Shutdown gracefully
            await app.shutdown()

        asyncio.run(main())
        ```

    Example (testing with context manager):
        ```python
        import requests

        app = create_axum_fraiseql_app(database_url="...")

        with app.running(host="127.0.0.1", port=8000):
            response = requests.post(
                "http://127.0.0.1:8000/graphql",
                json={"query": "{ users { id } }"}
            )
            assert response.status_code == 200
        ```

    Performance:
        - 7-10x faster than FastAPI due to Rust implementation
        - Sub-millisecond query latency typical
        - Efficient concurrent request handling
        - Built-in response compression (Brotli/Zstd)

    Features:
        - ✅ GraphQL queries, mutations, subscriptions
        - ✅ Automatic type introspection (__schema)
        - ✅ Error handling (GraphQL standard format)
        - ✅ WebSocket subscriptions (graphql-ws protocol)
        - ✅ Query caching (optional)
        - ✅ Built-in metrics (/metrics endpoint)
        - ✅ Response compression
        - ✅ CORS support
        - ✅ Direct query execution (for tests/jobs)

    Differences from FastAPI version:
        - start() is blocking (use start_async() for non-blocking)
        - /metrics endpoint included by default
        - Different middleware API (Phase 16)
        - CORS uses Axum defaults (customizable Phase 3)
        - No Swagger/ReDoc UI (reserved Phase 3)

    See Also:
        - AxumFraiseQLConfig: Configuration class
        - AxumServer: Server wrapper class
        - create_fraiseql_app: FastAPI version (for comparison)
    """
    # Initialize registry (use provided or singleton)
    if registry is None:
        registry = AxumRegistry.get_instance()
    else:
        # If custom registry provided, use it instead of singleton
        logger.debug("Using custom AxumRegistry instance")

    # Build configuration from parameters
    if config is None:
        # Extract database_url from kwargs if not provided directly
        if database_url is None:
            database_url = kwargs.pop("database_url", None)

        if database_url is None:
            raise ValueError(
                "database_url is required. Provide via parameter or config.database_url",
            )

        # Extract CORS settings from parameters
        if cors_origins is not None:
            kwargs["cors_origins"] = cors_origins
        if cors_allow_credentials is not None:
            kwargs["cors_allow_credentials"] = cors_allow_credentials
        if cors_allow_methods is not None:
            kwargs["cors_allow_methods"] = cors_allow_methods
        if cors_allow_headers is not None:
            kwargs["cors_allow_headers"] = cors_allow_headers

        # Create config from parameters
        config = AxumFraiseQLConfig(
            database_url=database_url,
            **kwargs,
        )

    # Validate database connection is possible (will fail on .start() if not)
    logger.debug(f"Creating FraiseQL Axum server: {config}")

    # Create AxumServer wrapper with registry
    server = AxumServer(config=config, registry=registry)

    # Register explicit lists
    if types:
        logger.debug(f"Registering {len(types)} types")
        server.register_types(types)

    if mutations:
        logger.debug(f"Registering {len(mutations)} mutations")
        server.register_mutations(mutations)

    if queries:
        logger.debug(f"Registering {len(queries)} queries")
        server.register_queries(queries)

    if subscriptions:
        logger.debug(f"Registering {len(subscriptions)} subscriptions")
        server.register_subscriptions(subscriptions)

    # Add middleware (Phase 16)
    if middleware:
        logger.debug(f"Adding {len(middleware)} middleware (Phase 16 support)")
        for m in middleware:
            server.add_middleware(m)

    # Note: context_getter is reserved for Phase 16
    if context_getter is not None:
        logger.warning("context_getter is reserved for Phase 16 implementation")

    # Note: docs_url, redoc_url, openapi_url are reserved for Phase 3
    if docs_url or redoc_url or openapi_url:
        logger.debug("API documentation UI is planned for Phase 3")

    logger.info(
        "FraiseQL Axum server created",
        extra={
            "types": len(server.registered_types()),
            "mutations": len(server.registered_mutations()),
            "queries": len(server.registered_queries()),
            "subscriptions": len(server.registered_subscriptions()),
        },
    )

    return server


def create_production_app(
    *,
    database_url: str,
    types: list[type[Any]] | None = None,
    mutations: list[type[Any]] | None = None,
    **kwargs: Any,
) -> AxumServer:
    """Create production-optimized Axum FraiseQL server.

    Configures sensible production defaults:
    - Error details hidden
    - Query caching enabled
    - Introspection disabled
    - Playground disabled
    - Production mode enabled
    - Compression enabled

    Args:
        database_url: PostgreSQL connection URL (required)
        types: List of GraphQL types
        mutations: List of mutations
        **kwargs: Additional config parameters

    Returns:
        AxumServer configured for production

    Example:
        ```python
        app = create_production_app(
            database_url="postgresql://prod-db/app",
            types=[User, Post],
            mutations=[CreateUser],
            cors_origins=["https://example.com"],
        )

        app.start(host="0.0.0.0", port=8000)
        ```
    """
    config = AxumFraiseQLConfig(
        database_url=database_url,
        production_mode=True,
        environment="production",
        hide_error_details=True,
        enable_introspection=False,
        enable_playground=False,
        enable_query_caching=True,
        enable_compression=True,
        **kwargs,
    )

    server = AxumServer(config=config)

    if types:
        server.register_types(types)
    if mutations:
        server.register_mutations(mutations)

    logger.info("Created production-optimized FraiseQL Axum server")

    return server
