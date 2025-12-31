"""Python wrapper for Rust DatabasePool.

Phase 1: Basic synchronous wrapper. Async support in Phase 1.5.
"""

from typing import Optional

from fraiseql._fraiseql_rs import DatabasePool as RustDatabasePool


class DatabasePool:
    """PostgreSQL connection pool with Rust backend.

    Provides a Python interface to the high-performance Rust connection pool.
    """

    def __init__(self, database_url: str, config: Optional[dict] = None) -> None:
        """Create a new database connection pool.

        Args:
            database_url: PostgreSQL connection URL (postgresql://user:pass@host:port/db)
            config: Optional pool configuration dict (Phase 1.5 enhancement)

        Config options:
            - max_size: Maximum connections in pool (default: 10)
            - min_idle: Minimum idle connections (default: 1)
            - connection_timeout: Connection acquisition timeout in seconds (default: 30)
            - idle_timeout: Idle connection timeout in seconds (default: 300)
            - max_lifetime: Maximum connection lifetime in seconds (default: 3600)
            - reap_frequency: Connection reaping frequency in seconds (default: 60)
        """
        if config is not None:
            self._rust_pool = RustDatabasePool(database_url, config)
        else:
            self._rust_pool = RustDatabasePool(database_url)

    def get_stats(self) -> str:
        """Get pool statistics summary.

        Returns:
            String with connection pool statistics
        """
        return self._rust_pool.get_stats()

    def get_config_summary(self) -> str:
        """Get pool configuration summary.

        Returns:
            String with pool configuration details
        """
        return self._rust_pool.get_config_summary()

    def __repr__(self) -> str:
        """String representation for debugging."""
        return str(self._rust_pool)
