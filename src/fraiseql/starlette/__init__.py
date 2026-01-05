"""Starlette integration for FraiseQL GraphQL framework.

This module provides a complete Starlette HTTP server implementation
as an alternative to FastAPI. Starlette is a lightweight ASGI framework
that offers the same functionality with a more minimal footprint.

Key Features:
- Complete GraphQL HTTP server (POST /graphql, GET /health)
- APQ (Automatic Persisted Queries) support
- Authentication middleware integration
- WebSocket subscriptions (graphql-ws protocol)
- CORS and security middleware
- Connection pooling and optimization

Framework-Agnostic Design:
The Starlette implementation uses the abstraction protocols defined in
fraiseql.http.interface, making it easy to swap between different HTTP
frameworks (Axum/Rust, Starlette/Python, FastAPI/Python).

Quick Start:
    from fraiseql.gql.schema_builder import build_fraiseql_schema
    from fraiseql.starlette.app import create_starlette_app

    schema = build_fraiseql_schema(database_url="postgresql://...")
    app = create_starlette_app(schema, database_url="postgresql://...")

    # Run with: uvicorn app:app

Protocols Implemented:
    - RequestParser: Converts Starlette Request to GraphQLRequest
    - ResponseFormatter: Converts GraphQLResponse to JSONResponse
    - HealthChecker: Standard /health endpoint
    - SubscriptionHandler: WebSocket subscription support (graphql-ws)

Deprecation Notice:
FastAPI-based servers are deprecated in favor of this Starlette
implementation. Migration is minimal - most user code will work with
little or no changes.

See Also:
    - fraiseql.http.interface: Abstract protocol definitions
    - fraiseql.fastapi.app: Legacy FastAPI implementation (deprecated)
    - fraiseql_rs.src.http.axum_server: Rust Axum server (recommended)
"""

from fraiseql.starlette.app import (
    StarletteRequestParser,
    StarletteResponseFormatter,
    create_starlette_app,
)

__version__ = "2.0.0"
__all__ = [
    "StarletteRequestParser",
    "StarletteResponseFormatter",
    "create_starlette_app",
]
