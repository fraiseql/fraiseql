"""Redis cache backend for FraiseQL.

This module provides a Redis-based cache backend implementation
with proper error handling and connection management.
"""

import json
from typing import Any

from redis.asyncio import Redis
from redis.exceptions import ConnectionError as RedisConnectionErrorBase


class RedisConnectionError(Exception):
    """Raised when Redis connection fails."""


class RedisCache:
    """Redis-based cache backend."""

    def __init__(self, redis_client: Redis) -> None:
        """Initialize Redis cache.

        Args:
            redis_client: Redis async client instance
        """
        self.redis = redis_client

    async def get(self, key: str) -> Any | None:
        """Get value from cache.

        Args:
            key: Cache key

        Returns:
            Cached value or None if not found

        Raises:
            RedisConnectionError: If Redis connection fails
        """
        try:
            value = await self.redis.get(key)
            if value is None:
                return None
            return json.loads(value)
        except RedisConnectionErrorBase as e:
            raise RedisConnectionError(f"Failed to connect to Redis: {e}") from e
        except json.JSONDecodeError:
            # Corrupted cache entry, return None
            return None

    async def set(self, key: str, value: Any, ttl: int) -> None:
        """Set value in cache with TTL.

        Args:
            key: Cache key
            value: Value to cache
            ttl: Time-to-live in seconds

        Raises:
            ValueError: If value cannot be serialized
            RedisConnectionError: If Redis connection fails
        """
        try:
            # Don't use default=str to catch non-serializable objects
            serialized = json.dumps(value)
        except (TypeError, ValueError) as e:
            raise ValueError(f"Failed to serialize value: {e}") from e

        try:
            await self.redis.setex(key, ttl, serialized)
        except RedisConnectionErrorBase as e:
            raise RedisConnectionError(f"Failed to connect to Redis: {e}") from e

    async def delete(self, key: str) -> bool:
        """Delete a key from cache.

        Args:
            key: Cache key

        Returns:
            True if key was deleted, False otherwise

        Raises:
            RedisConnectionError: If Redis connection fails
        """
        try:
            result = await self.redis.delete(key)
            return result > 0
        except RedisConnectionErrorBase as e:
            raise RedisConnectionError(f"Failed to connect to Redis: {e}") from e

    async def delete_pattern(self, pattern: str) -> int:
        """Delete all keys matching a pattern.

        Args:
            pattern: Pattern to match (e.g., "user:*")

        Returns:
            Number of keys deleted

        Raises:
            RedisConnectionError: If Redis connection fails
        """
        try:
            keys = []
            async for key in self.redis.scan_iter(match=pattern):
                keys.append(key)

            if keys:
                result = await self.redis.delete(*keys)
                return result
            return 0
        except RedisConnectionErrorBase as e:
            raise RedisConnectionError(f"Failed to connect to Redis: {e}") from e

    async def exists(self, key: str) -> bool:
        """Check if key exists in cache.

        Args:
            key: Cache key

        Returns:
            True if key exists, False otherwise

        Raises:
            RedisConnectionError: If Redis connection fails
        """
        try:
            return await self.redis.exists(key) > 0
        except RedisConnectionErrorBase as e:
            raise RedisConnectionError(f"Failed to connect to Redis: {e}") from e

    async def ping(self) -> bool:
        """Check if Redis connection is alive.

        Returns:
            True if connection is alive

        Raises:
            RedisConnectionError: If Redis connection fails
        """
        try:
            return await self.redis.ping()
        except RedisConnectionErrorBase as e:
            raise RedisConnectionError(f"Failed to connect to Redis: {e}") from e
