#!/usr/bin/env python3
"""FraiseQL Axum HTTP Server - Quick Start Example.

This example demonstrates creating a high-performance GraphQL API using
FraiseQL's Axum HTTP server integration (7-10x faster than FastAPI).

Example Usage (blocking):
    python examples/axum_quickstart.py

Example Usage (async):
    python examples/axum_quickstart_async.py

Example Usage (testing with context manager):
    pytest tests/test_axum_quickstart.py
"""

import asyncio
import logging
from typing import Any

from fraiseql import create_axum_fraiseql_app, fraise_type

# Enable logging to see server startup messages
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


# ===== GraphQL Types =====

@fraise_type
class User:
    """A user in the system."""

    id: str
    name: str
    email: str
    age: int | None = None


@fraise_type
class Post:
    """A blog post."""

    id: str
    title: str
    content: str
    author_id: str


# ===== GraphQL Mutations (example structure) =====

@fraise_type
class CreateUserInput:
    """Input for creating a user."""

    name: str
    email: str
    age: int | None = None


@fraise_type
class CreateUserResult:
    """Result of creating a user."""

    success: bool
    user: User | None = None
    error: str | None = None


# ===== Example: Blocking Server Start =====

def example_blocking_start() -> None:
    """Start Axum server and handle requests (blocking).

    This example demonstrates:
    1. Creating an Axum app
    2. Starting the server (blocking call)
    3. Accepting HTTP requests
    4. Server continues until interrupted (Ctrl+C)
    """
    logger.info("Starting FraiseQL Axum server (blocking mode)")

    # Create app with types
    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User, Post, CreateUserInput, CreateUserResult],
        title="FraiseQL Axum API",
        description="High-performance GraphQL API",
        version="1.0.0",
    )

    # Log configuration
    config = app.get_config()
    logger.info(f"Configuration: {config}")
    logger.info(f"Server URL: {config.server_url}")
    logger.info(f"GraphQL Endpoint: {config.server_url}/graphql")
    logger.info(f"Metrics Endpoint: {config.server_url}/metrics")
    logger.info(f"Registered types: {', '.join(app.registered_types())}")

    # Start server (blocking - server runs until Ctrl+C)
    try:
        app.start(host=config.axum_host, port=config.axum_port)
    except KeyboardInterrupt:
        logger.info("Shutting down...")


# ===== Example: Async Server Start =====

async def example_async_start() -> None:
    """Start Axum server asynchronously (non-blocking).

    This example demonstrates:
    1. Creating an Axum app
    2. Starting the server asynchronously
    3. Server runs in background
    4. Can perform other async operations
    5. Graceful shutdown
    """
    logger.info("Starting FraiseQL Axum server (async mode)")

    # Create app
    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User, Post],
    )

    # Start server asynchronously
    await app.start_async(host="0.0.0.0", port=8000)

    logger.info(f"Server running at {app.get_config().server_url}")
    logger.info("Performing other async operations...")

    # Server is now running in background
    # Can do other async operations
    for i in range(5):
        logger.info(f"  Doing work... ({i + 1}/5)")
        await asyncio.sleep(1)

    # Gracefully shutdown
    logger.info("Shutting down server...")
    await app.shutdown()
    logger.info("Server stopped")


# ===== Example: Direct Query Execution =====

def example_direct_query_execution() -> None:
    """Execute GraphQL queries directly (without HTTP).

    This example demonstrates:
    1. Creating an app
    2. Executing queries directly (no HTTP server)
    3. Useful for testing and background jobs
    """
    logger.info("Executing GraphQL queries directly (no HTTP server)")

    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User, Post],
    )

    # Execute query directly
    query = """
    query GetUser($id: ID!) {
        user(id: $id) {
            id
            name
            email
        }
    }
    """

    variables = {"id": "user-123"}

    logger.info(f"Query: {query}")
    logger.info(f"Variables: {variables}")

    # Execute directly (synchronous)
    result = app.execute_query(
        query=query,
        variables=variables,
    )

    logger.info(f"Result: {result}")


# ===== Example: Context Manager =====

def example_context_manager() -> None:
    """Use server with context manager for testing.

    This example demonstrates:
    1. Using 'with' statement for automatic startup/shutdown
    2. Server starts at entry, stops at exit
    3. Useful for integration tests
    """
    logger.info("Using server with context manager")

    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User, Post],
    )

    # Server starts on context entry, stops on context exit
    with app.running(host="127.0.0.1", port=8000):
        logger.info("Server is running inside context manager")
        logger.info(f"Server URL: {app.get_config().server_url}")
        logger.info(f"Registered types: {app.registered_types()}")
        logger.info("Perform HTTP requests or direct queries here")

        # Example: Direct query
        result = app.execute_query("{ users { id name } }")
        logger.info(f"Query result: {result}")

    logger.info("Server stopped (exited context manager)")


# ===== Example: Async Context Manager =====

async def example_async_context_manager() -> None:
    """Use server with async context manager.

    This example demonstrates:
    1. Non-blocking server lifecycle
    2. Async entry/exit
    3. Server runs in background
    """
    logger.info("Using server with async context manager")

    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User, Post],
    )

    async with app.running_async(host="127.0.0.1", port=8000):
        logger.info("Server is running (non-blocking)")
        logger.info(f"Server URL: {app.get_config().server_url}")

        # Do async work while server is running
        await asyncio.sleep(2)

        logger.info("Server is still running...")
        await asyncio.sleep(2)

    logger.info("Server stopped (exited async context manager)")


# ===== Example: Configuration from Environment =====

def example_config_from_env() -> None:
    """Create app configuration from environment variables.

    Environment variables:
        FRAISEQL_DATABASE_URL: PostgreSQL URL (required)
        FRAISEQL_HOST: Bind address (default: 127.0.0.1)
        FRAISEQL_PORT: Bind port (default: 8000)
        FRAISEQL_ENV: Environment (default: development)
        FRAISEQL_PRODUCTION: Enable production mode (default: false)
        FRAISEQL_AUTH_ENABLED: Enable auth (default: false)
    """
    import os

    logger.info("Creating config from environment variables")

    # Set environment variable
    os.environ["FRAISEQL_DATABASE_URL"] = "postgresql://localhost/fraiseql_test"
    os.environ["FRAISEQL_HOST"] = "0.0.0.0"
    os.environ["FRAISEQL_PORT"] = "8000"

    # Create config from environment
    from fraiseql.axum import AxumFraiseQLConfig

    config = AxumFraiseQLConfig.from_env()
    logger.info(f"Config from env: {config}")

    # Create app with config
    app = create_axum_fraiseql_app(
        config=config,
        types=[User, Post],
    )

    logger.info(f"App created: {app}")


# ===== Example: Production Configuration =====

def example_production_config() -> None:
    """Create production-optimized server.

    Production defaults:
    - Error details hidden
    - Query caching enabled
    - Introspection disabled
    - Playground disabled
    - Production mode enabled
    """
    from fraiseql.axum.app import create_production_app

    logger.info("Creating production-optimized server")

    app = create_production_app(
        database_url="postgresql://prod-db/app",
        types=[User, Post],
        cors_origins=["https://example.com"],
    )

    config = app.get_config()
    logger.info(f"Production config:")
    logger.info(f"  Environment: {config.environment}")
    logger.info(f"  Production mode: {config.production_mode}")
    logger.info(f"  Hide errors: {config.hide_error_details}")
    logger.info(f"  Caching enabled: {config.enable_query_caching}")
    logger.info(f"  Introspection: {config.enable_introspection}")


# ===== Main Entry Point =====

if __name__ == "__main__":
    import sys

    # Choose example to run
    if len(sys.argv) > 1:
        example = sys.argv[1]
    else:
        example = "context_manager"

    examples = {
        "blocking": example_blocking_start,
        "async": lambda: asyncio.run(example_async_start()),
        "direct_query": example_direct_query_execution,
        "context_manager": example_context_manager,
        "async_context_manager": lambda: asyncio.run(example_async_context_manager()),
        "env_config": example_config_from_env,
        "production": example_production_config,
    }

    if example in examples:
        logger.info(f"\n{'=' * 60}")
        logger.info(f"Running example: {example}")
        logger.info(f"{'=' * 60}\n")
        examples[example]()
    else:
        print(f"Unknown example: {example}")
        print(f"Available examples: {', '.join(examples.keys())}")
        sys.exit(1)
