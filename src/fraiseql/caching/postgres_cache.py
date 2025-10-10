"""PostgreSQL cache backend for FraiseQL.

This module provides a PostgreSQL-based cache backend implementation
using UNLOGGED tables for high-performance caching without WAL overhead.
"""

import json
import logging
from datetime import UTC, datetime, timedelta
from typing import Any

import psycopg

logger = logging.getLogger(__name__)


class PostgresCacheError(Exception):
    """Raised when PostgreSQL cache operation fails."""


class PostgresCache:
    """PostgreSQL-based cache backend using UNLOGGED tables.

    Uses UNLOGGED tables for maximum performance - data is not written to WAL,
    making cache operations as fast as in-memory solutions while providing
    persistence and shared access across multiple instances.

    Note: UNLOGGED tables are cleared on crash/restart, which is acceptable
    for cache data that can be regenerated.
    """

    def __init__(
        self,
        connection_pool,
        table_name: str = "fraiseql_cache",
        auto_initialize: bool = True,
    ) -> None:
        """Initialize PostgreSQL cache.

        Args:
            connection_pool: psycopg connection pool
            table_name: Name of the cache table (default: fraiseql_cache)
            auto_initialize: Whether to automatically create table if missing
        """
        self.pool = connection_pool
        self.table_name = table_name
        self._initialized = False

        if auto_initialize:
            # Note: Initialization should be done async, but we defer to first operation
            pass

    async def _ensure_initialized(self) -> None:
        """Ensure cache table exists."""
        if self._initialized:
            return

        async with self.pool.connection() as conn, conn.cursor() as cur:
            # Create UNLOGGED table for cache
            # UNLOGGED = no WAL = faster writes, but data lost on crash (acceptable for cache)
            await cur.execute(f"""
                CREATE UNLOGGED TABLE IF NOT EXISTS {self.table_name} (
                    cache_key TEXT PRIMARY KEY,
                    cache_value JSONB NOT NULL,
                    expires_at TIMESTAMPTZ NOT NULL
                )
            """)

            # Index on expiry for efficient cleanup
            await cur.execute(f"""
                CREATE INDEX IF NOT EXISTS {self.table_name}_expires_idx
                ON {self.table_name} (expires_at)
            """)

            await conn.commit()

        self._initialized = True
        logger.info("PostgreSQL cache table '%s' initialized", self.table_name)

    async def get(self, key: str) -> Any | None:
        """Get value from cache.

        Args:
            key: Cache key

        Returns:
            Cached value or None if not found or expired

        Raises:
            PostgresCacheError: If database operation fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                # Get value and check expiry in one query
                await cur.execute(
                    f"""
                    SELECT cache_value
                    FROM {self.table_name}
                    WHERE cache_key = %s
                      AND expires_at > NOW()
                    """,
                    (key,),
                )

                result = await cur.fetchone()
                if result is None:
                    return None

                return result[0]  # JSONB is automatically deserialized

        except psycopg.Error as e:
            logger.error("Failed to get cache key '%s': %s", key, e)
            raise PostgresCacheError(f"Failed to get cache key: {e}") from e

    async def set(self, key: str, value: Any, ttl: int) -> None:
        """Set value in cache with TTL.

        Args:
            key: Cache key
            value: Value to cache (must be JSON-serializable)
            ttl: Time-to-live in seconds

        Raises:
            ValueError: If value cannot be serialized
            PostgresCacheError: If database operation fails
        """
        try:
            # Validate that value is JSON-serializable
            try:
                json.dumps(value)
            except (TypeError, ValueError) as e:
                raise ValueError(f"Failed to serialize value: {e}") from e

            await self._ensure_initialized()

            expires_at = datetime.now(UTC) + timedelta(seconds=ttl)

            async with self.pool.connection() as conn, conn.cursor() as cur:
                # UPSERT using ON CONFLICT
                await cur.execute(
                    f"""
                    INSERT INTO {self.table_name} (cache_key, cache_value, expires_at)
                    VALUES (%s, %s, %s)
                    ON CONFLICT (cache_key)
                    DO UPDATE SET
                        cache_value = EXCLUDED.cache_value,
                        expires_at = EXCLUDED.expires_at
                    """,
                    (key, json.dumps(value), expires_at),
                )
                await conn.commit()

        except psycopg.Error as e:
            logger.error("Failed to set cache key '%s': %s", key, e)
            raise PostgresCacheError(f"Failed to set cache key: {e}") from e

    async def delete(self, key: str) -> bool:
        """Delete a key from cache.

        Args:
            key: Cache key

        Returns:
            True if key was deleted, False if key didn't exist

        Raises:
            PostgresCacheError: If database operation fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute(
                    f"DELETE FROM {self.table_name} WHERE cache_key = %s",
                    (key,),
                )
                await conn.commit()
                return cur.rowcount > 0

        except psycopg.Error as e:
            logger.error("Failed to delete cache key '%s': %s", key, e)
            raise PostgresCacheError(f"Failed to delete cache key: {e}") from e

    async def delete_pattern(self, pattern: str) -> int:
        """Delete all keys matching a pattern.

        Args:
            pattern: SQL LIKE pattern (e.g., "user:%")

        Returns:
            Number of keys deleted

        Raises:
            PostgresCacheError: If database operation fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                # Convert Redis-style pattern to SQL LIKE pattern
                # Redis uses * for wildcard, SQL uses %
                sql_pattern = pattern.replace("*", "%")

                await cur.execute(
                    f"DELETE FROM {self.table_name} WHERE cache_key LIKE %s",
                    (sql_pattern,),
                )
                await conn.commit()
                return cur.rowcount

        except psycopg.Error as e:
            logger.error("Failed to delete pattern '%s': %s", pattern, e)
            raise PostgresCacheError(f"Failed to delete pattern: {e}") from e

    async def exists(self, key: str) -> bool:
        """Check if key exists in cache and is not expired.

        Args:
            key: Cache key

        Returns:
            True if key exists and is not expired, False otherwise

        Raises:
            PostgresCacheError: If database operation fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute(
                    f"""
                    SELECT 1
                    FROM {self.table_name}
                    WHERE cache_key = %s
                      AND expires_at > NOW()
                    """,
                    (key,),
                )

                return await cur.fetchone() is not None

        except psycopg.Error as e:
            logger.error("Failed to check cache key '%s': %s", key, e)
            raise PostgresCacheError(f"Failed to check cache key: {e}") from e

    async def ping(self) -> bool:
        """Check if PostgreSQL connection is alive.

        Returns:
            True if connection is alive

        Raises:
            PostgresCacheError: If connection check fails
        """
        try:
            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute("SELECT 1")
                result = await cur.fetchone()
                return result is not None

        except psycopg.Error as e:
            logger.error("Failed to ping PostgreSQL: %s", e)
            raise PostgresCacheError(f"Failed to ping PostgreSQL: {e}") from e

    async def cleanup_expired(self) -> int:
        """Remove expired cache entries.

        This should be called periodically (e.g., via a background task)
        to prevent the cache table from growing indefinitely.

        Returns:
            Number of expired entries removed

        Raises:
            PostgresCacheError: If cleanup operation fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute(
                    f"DELETE FROM {self.table_name} WHERE expires_at <= NOW()",
                )
                await conn.commit()
                cleaned = cur.rowcount

                if cleaned > 0:
                    logger.info("Cleaned %s expired cache entries", cleaned)

                return cleaned

        except psycopg.Error as e:
            logger.error("Failed to cleanup expired entries: %s", e)
            raise PostgresCacheError(f"Failed to cleanup expired entries: {e}") from e

    async def clear_all(self) -> int:
        """Clear all cache entries.

        Warning: This removes ALL cached data.

        Returns:
            Number of entries removed

        Raises:
            PostgresCacheError: If clear operation fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                await cur.execute(f"DELETE FROM {self.table_name}")
                await conn.commit()
                return cur.rowcount

        except psycopg.Error as e:
            logger.error("Failed to clear cache: %s", e)
            raise PostgresCacheError(f"Failed to clear cache: {e}") from e

    async def get_stats(self) -> dict[str, Any]:
        """Get cache statistics.

        Returns:
            Dictionary with cache stats (total_entries, expired_entries, table_size_bytes)

        Raises:
            PostgresCacheError: If stats query fails
        """
        try:
            await self._ensure_initialized()

            async with self.pool.connection() as conn, conn.cursor() as cur:
                # Get total entries
                await cur.execute(
                    f"SELECT COUNT(*) FROM {self.table_name}",
                )
                total = (await cur.fetchone())[0]

                # Get expired entries (not yet cleaned)
                await cur.execute(
                    f"SELECT COUNT(*) FROM {self.table_name} WHERE expires_at <= NOW()",
                )
                expired = (await cur.fetchone())[0]

                # Get table size
                await cur.execute(
                    """
                    SELECT pg_total_relation_size(%s)
                    """,
                    (self.table_name,),
                )
                size_bytes = (await cur.fetchone())[0]

                return {
                    "total_entries": total,
                    "expired_entries": expired,
                    "active_entries": total - expired,
                    "table_size_bytes": size_bytes,
                }

        except psycopg.Error as e:
            logger.error("Failed to get cache stats: %s", e)
            raise PostgresCacheError(f"Failed to get cache stats: {e}") from e
