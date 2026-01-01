"""Python wrapper for Rust DatabasePool.

Phase 1: Production pool wrapper with backward compatibility.
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
            config: Optional pool configuration dict

        Config options (passed as kwargs to RustDatabasePool):
            - max_size: Maximum connections in pool (default: 10)
            - ssl_mode: SSL mode - "disable", "prefer", "require" (default: "prefer")
        """
        # Parse config dict into kwargs for Rust pool
        if config is not None:
            # Convert dict config to RustDatabasePool kwargs
            max_size = config.get("max_size", 10)
            ssl_mode = config.get("ssl_mode", "prefer")
            self._rust_pool = RustDatabasePool(
                url=database_url, max_size=max_size, ssl_mode=ssl_mode
            )
        else:
            # Use URL only (defaults apply)
            self._rust_pool = RustDatabasePool(url=database_url)

    def get_stats(self) -> str:
        """Get pool statistics summary.

        Returns:
            String with connection pool statistics (backward compatible format)
        """
        stats = self._rust_pool.stats()
        active = stats["active"]
        available = stats["available"]
        return f"{active} connections, {available} idle"

    def get_config_summary(self) -> str:
        """Get pool configuration summary.

        Returns:
            String with pool configuration details
        """
        stats = self._rust_pool.stats()
        max_size = stats["max_size"]
        # Assuming min_idle=1 (not exposed by current stats)
        return f"max_size={max_size}, min_idle=1"

    def stats(self) -> dict:
        """Get pool statistics (new API).

        Returns:
            Dict with keys: size, available, max_size, active
        """
        return self._rust_pool.stats()

    def __repr__(self) -> str:
        """String representation for debugging."""
        return repr(self._rust_pool)
