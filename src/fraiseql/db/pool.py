"""Database connection pool factory functions for FraiseQL.

This module provides three types of database pools for different use cases:

1. **Production Pool** - Rust-based with SSL/TLS support (optimal for production)
2. **Prototype Pool** - Rust-based async bridge (optimal for development/testing)
3. **Legacy Pool** - Pure Python with psycopg3 (optimal for compatibility)

Each pool is optimized for its specific use case while maintaining a consistent interface.
"""

import logging
import os
from typing import Any

from psycopg import AsyncConnection
from psycopg_pool import AsyncConnectionPool

logger = logging.getLogger(__name__)

# Phase 1: Production pool feature flag
USE_PRODUCTION_POOL = os.environ.get("FRAISEQL_PRODUCTION_POOL", "false").lower() == "true"

# Pool availability checks
try:
    if USE_PRODUCTION_POOL:
        from fraiseql._fraiseql_rs import DatabasePool as RustProductionPool

        HAS_PRODUCTION_POOL = True
    else:
        HAS_PRODUCTION_POOL = False
except ImportError:
    HAS_PRODUCTION_POOL = False

try:
    from fraiseql._fraiseql_rs import PrototypePool as RustPrototypePool

    HAS_PROTOTYPE_POOL = True
except ImportError:
    HAS_PROTOTYPE_POOL = False


async def create_production_pool(
    database: str,
    *,
    host: str = "localhost",
    port: int = 5432,
    user: str | None = None,
    password: str | None = None,
    ssl_mode: str = "prefer",
    **kwargs: Any,
) -> Any:
    """Create a production-grade database pool with SSL/TLS support.

    This pool uses the Rust-based DatabasePool from fraiseql_rs for optimal
    performance with built-in connection health checks and TLS support.

    **Best for**: Production deployments, high-traffic applications, SSL-required environments.

    Args:
        database: Database name
        host: PostgreSQL host (default: "localhost")
        port: PostgreSQL port (default: 5432)
        user: Database user (default: None, uses system defaults)
        password: Database password (default: None)
        ssl_mode: SSL mode - "disable", "allow", "prefer", "require" (default: "prefer")
        **kwargs: Additional parameters passed to Rust DatabasePool

    Returns:
        Rust DatabasePool instance with production features

    Raises:
        ImportError: If Rust extension (fraiseql_rs) is not available.
        RuntimeError: If FRAISEQL_PRODUCTION_POOL is not enabled.

    Example:
        ```python
        pool = await create_production_pool(
            "mydb",
            host="db.example.com",
            user="appuser",
            password="secure_password",
            ssl_mode="require"
        )
        ```
    """
    if not HAS_PRODUCTION_POOL:
        msg = (
            "Production pool not available. "
            "Set FRAISEQL_PRODUCTION_POOL=true environment variable "
            "and ensure fraiseql_rs is installed."
        )
        raise ImportError(msg)

    return RustProductionPool(
        database=database,
        host=host,
        port=port,
        user=user,
        password=password,
        ssl_mode=ssl_mode,
        **kwargs,
    )


async def create_prototype_pool(
    database: str,
    *,
    host: str = "localhost",
    port: int = 5432,
    user: str | None = None,
    password: str | None = None,
    **kwargs: Any,
) -> Any:
    """Create a prototype database pool for development and testing.

    This pool uses the Rust-based PrototypePool from fraiseql_rs for high
    performance without SSL/TLS overhead. Optimal for local development.

    **Best for**: Development environments, CI/CD testing, prototyping.

    Args:
        database: Database name
        host: PostgreSQL host (default: "localhost")
        port: PostgreSQL port (default: 5432)
        user: Database user (default: None, uses system defaults)
        password: Database password (default: None)
        **kwargs: Additional parameters passed to Rust PrototypePool

    Returns:
        Rust PrototypePool instance with minimal overhead

    Raises:
        ImportError: If Rust extension (fraiseql_rs) is not available.

    Example:
        ```python
        # Development pool (default)
        pool = await create_prototype_pool("mydb")

        # With custom credentials
        pool = await create_prototype_pool(
            "mydb",
            host="localhost",
            user="dev_user",
            password="dev_pass"
        )
        ```
    """
    if not HAS_PROTOTYPE_POOL:
        msg = (
            "Prototype pool not available. "
            "Ensure fraiseql_rs is installed with: pip install fraiseql[rust]"
        )
        raise ImportError(msg)

    return RustPrototypePool(
        database=database,
        host=host,
        port=port,
        user=user,
        password=password,
        **kwargs,
    )


async def create_legacy_pool(
    database_url: str,
    **pool_kwargs: Any,
) -> AsyncConnectionPool:
    """Create a legacy Python database pool using psycopg3.

    This pool is implemented in pure Python using psycopg_pool.AsyncConnectionPool
    for maximum compatibility when Rust extensions are unavailable.

    **Best for**: Compatibility with pure-Python deployments, debugging, system integration.

    Args:
        database_url: Full PostgreSQL connection URL
                     (e.g., "postgresql://user:pass@localhost/mydb")
        **pool_kwargs: Additional parameters for AsyncConnectionPool:
                      - min_size: Minimum connections (default: 1)
                      - max_size: Maximum connections (default: 10)
                      - timeout: Connection timeout in seconds
                      - command_timeout: Query timeout in seconds
                      - check_interval: Health check interval

    Returns:
        AsyncConnectionPool instance from psycopg_pool

    Raises:
        psycopg_pool.PoolError: If pool creation fails.
        psycopg.OperationalError: If database connection fails.

    Example:
        ```python
        # Basic usage
        pool = await create_legacy_pool("postgresql://localhost/mydb")

        # With custom parameters
        pool = await create_legacy_pool(
            "postgresql://user:pass@db.example.com/mydb",
            min_size=5,
            max_size=20,
            timeout=30
        )
        ```
    """

    # Configure type handling and connection validation
    async def configure_types(conn: AsyncConnection) -> None:
        """Configure type adapters to keep dates as strings."""
        from psycopg.adapt import Loader

        class TextLoader(Loader):
            def load(self, data: Any) -> Any:
                return data.decode("utf-8") if isinstance(data, bytes) else data

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

    pool = AsyncConnectionPool(
        database_url,
        configure=configure_types,
        check=check_connection,
        open=False,
        **pool_kwargs,
    )

    await pool.open()
    return pool
