"""Database adapter factory and configuration.

Provides environment-driven database adapter selection and initialization.
Follows factory pattern aligned with FraiseQL implementations.
"""

import os
from typing import Any

from .adapter import DatabaseType, FraiserDatabaseAdapter
from .sqlite_adapter import SqliteAdapter


class DatabaseConfig:
    """Database configuration from environment variables.

    Priority order:
    1. Explicit parameter values
    2. Environment variables
    3. Hardcoded defaults

    Environment variables:
    - FRAISIER_DB_TYPE: Database type (sqlite, postgresql, mysql)
    - FRAISIER_DB_URL: Full connection string
    - FRAISIER_DB_PATH: SQLite file path (for sqlite type)
    - FRAISIER_DB_POOL_MIN: Minimum pool size (postgres/mysql)
    - FRAISIER_DB_POOL_MAX: Maximum pool size (postgres/mysql)
    """

    def __init__(
        self,
        db_type: str | None = None,
        db_url: str | None = None,
        db_path: str | None = None,
        pool_min_size: int | None = None,
        pool_max_size: int | None = None,
    ):
        """Initialize configuration from parameters and environment.

        Args:
            db_type: Database type override
            db_url: Connection URL override
            db_path: SQLite path override
            pool_min_size: Pool min size override
            pool_max_size: Pool max size override
        """
        # Database type (default: sqlite for dev)
        self.db_type = db_type or os.getenv("FRAISIER_DB_TYPE", "sqlite")

        # Connection URL
        if db_url:
            self.db_url = db_url
        else:
            self.db_url = os.getenv(
                "FRAISIER_DB_URL",
                None,
            )

        # SQLite path (used if db_type is sqlite and no URL provided)
        self.db_path = db_path or os.getenv(
            "FRAISIER_DB_PATH",
            ":memory:",  # Default to in-memory for safety
        )

        # Pool configuration
        self.pool_min_size = pool_min_size or int(
            os.getenv("FRAISIER_DB_POOL_MIN", "1")
        )
        self.pool_max_size = pool_max_size or int(
            os.getenv("FRAISIER_DB_POOL_MAX", "10")
        )

        self._validate()

    def _validate(self) -> None:
        """Validate configuration values.

        Raises:
            ValueError: If configuration is invalid
        """
        valid_types = {db_type.value for db_type in DatabaseType}
        if self.db_type not in valid_types:
            raise ValueError(
                f"Invalid FRAISIER_DB_TYPE: {self.db_type}. "
                f"Valid values: {', '.join(sorted(valid_types))}"
            )

        if self.pool_min_size < 1:
            raise ValueError("FRAISIER_DB_POOL_MIN must be >= 1")

        if self.pool_max_size < self.pool_min_size:
            raise ValueError(
                "FRAISIER_DB_POOL_MAX must be >= FRAISIER_DB_POOL_MIN"
            )


async def create_adapter_from_url(
    connection_url: str,
    pool_min_size: int = 1,
    pool_max_size: int = 10,
) -> FraiserDatabaseAdapter:
    """Create and connect a database adapter from connection URL.

    Infers database type from URL scheme:
    - sqlite:// → SQLiteAdapter
    - postgresql:// or postgres:// → PostgresAdapter
    - mysql:// or mysql+aiomysql:// → MysqlAdapter

    Args:
        connection_url: Full connection URL
        pool_min_size: Minimum pool size (for postgres/mysql)
        pool_max_size: Maximum pool size (for postgres/mysql)

    Returns:
        Connected FraiserDatabaseAdapter instance

    Raises:
        ValueError: If URL scheme is unsupported
        ConnectionError: If adapter cannot connect
    """
    # Parse URL scheme
    scheme = connection_url.split("://")[0] if "://" in connection_url else ""

    if not scheme:
        raise ValueError(f"Invalid connection URL: {connection_url}")

    # Create appropriate adapter (lazy import optional databases)
    if scheme == "sqlite":
        # Extract path from sqlite:///path or sqlite:///:memory:
        path = connection_url.replace("sqlite://", "")
        if not path:
            path = ":memory:"
        adapter = SqliteAdapter(path)
    elif scheme in ("postgresql", "postgres"):
        try:
            from .postgres_adapter import PostgresAdapter
        except ImportError as e:
            raise ImportError(
                "PostgreSQL support requires 'psycopg[binary]>=3.1.0'. "
                "Install with: pip install 'psycopg[binary]' or pip install fraisier[postgres]"
            ) from e

        adapter = PostgresAdapter(
            connection_url,
            pool_min_size=pool_min_size,
            pool_max_size=pool_max_size,
        )
    elif scheme in ("mysql", "mysql+aiomysql"):
        try:
            from .mysql_adapter import MysqlAdapter
        except ImportError as e:
            raise ImportError(
                "MySQL support requires 'aiomysql>=0.2.0'. "
                "Install with: pip install aiomysql or pip install fraisier[mysql]"
            ) from e

        adapter = MysqlAdapter(
            connection_url,
            pool_min_size=pool_min_size,
            pool_max_size=pool_max_size,
        )
    else:
        raise ValueError(
            f"Unsupported database scheme: {scheme}. "
            f"Supported: sqlite, postgresql, mysql"
        )

    # Connect adapter
    await adapter.connect()
    return adapter


async def get_database_adapter(
    config: DatabaseConfig | None = None,
) -> FraiserDatabaseAdapter:
    """Create and connect database adapter from configuration.

    Uses DatabaseConfig defaults and environment variables if not provided.

    Args:
        config: DatabaseConfig instance (created from env if not provided)

    Returns:
        Connected FraiserDatabaseAdapter instance

    Raises:
        ValueError: If configuration is invalid
        ConnectionError: If adapter cannot connect
    """
    if config is None:
        config = DatabaseConfig()

    # Build connection URL if not provided
    if config.db_url:
        connection_url = config.db_url
    elif config.db_type == "sqlite":
        connection_url = f"sqlite://{config.db_path}"
    else:
        raise ValueError(
            f"No connection URL provided for {config.db_type}. "
            f"Set FRAISIER_DB_URL environment variable"
        )

    return await create_adapter_from_url(
        connection_url,
        pool_min_size=config.pool_min_size,
        pool_max_size=config.pool_max_size,
    )


# Global adapter instance
_default_adapter: FraiserDatabaseAdapter | None = None


async def get_default_adapter() -> FraiserDatabaseAdapter:
    """Get or create default database adapter.

    Uses environment-based configuration.
    Creates adapter on first call, returns cached instance on subsequent calls.

    Returns:
        Connected FraiserDatabaseAdapter instance
    """
    global _default_adapter
    if _default_adapter is None:
        _default_adapter = await get_database_adapter()
    return _default_adapter


async def set_default_adapter(adapter: FraiserDatabaseAdapter | None) -> None:
    """Set or reset default database adapter.

    Useful for testing and explicit configuration.

    Args:
        adapter: New default adapter (or None to reset)
    """
    global _default_adapter
    _default_adapter = adapter


__all__ = [
    "DatabaseConfig",
    "create_adapter_from_url",
    "get_database_adapter",
    "get_default_adapter",
    "set_default_adapter",
]
