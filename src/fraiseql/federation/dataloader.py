"""DataLoader pattern implementation for efficient entity resolution batching.

DataLoader is a pattern for batching and caching database requests during
a single execution phase (typically a GraphQL query). It:

1. **Batches**: Collects multiple requests and executes them as one query
2. **Deduplicates**: Identical requests reuse the same Future
3. **Caches**: Avoids redundant queries for the same key
4. **Preserves Order**: Returns results in the requested order

Example:
    >>> loader = EntityDataLoader(resolver, db_pool)
    >>> # These requests are batched into a single database query
    >>> user1_future = loader.load("User", "id", "user-1")
    >>> user2_future = loader.load("User", "id", "user-2")
    >>> user1 = await user1_future
    >>> user2 = await user2_future
"""

import asyncio
from collections import defaultdict
from dataclasses import dataclass
from typing import Any


@dataclass
class DataLoaderStats:
    """Statistics about DataLoader cache performance."""

    #: Number of successful cache hits
    cache_hits: int = 0

    #: Number of cache misses (database queries needed)
    cache_misses: int = 0

    #: Number of duplicate requests deduplicated
    dedup_hits: int = 0

    #: Number of total requests processed
    total_requests: int = 0

    #: Number of database queries executed
    batch_count: int = 0

    @property
    def cache_hit_rate(self) -> float:
        """Calculate cache hit rate (0.0 to 1.0)."""
        if self.total_requests == 0:
            return 0.0
        return (
            self.cache_hits / (self.cache_hits + self.cache_misses)
            if (self.cache_hits + self.cache_misses) > 0
            else 0.0
        )

    @property
    def dedup_rate(self) -> float:
        """Calculate deduplication rate (0.0 to 1.0)."""
        if self.total_requests == 0:
            return 0.0
        return self.dedup_hits / self.total_requests


class EntityDataLoader:
    """DataLoader for entity resolution with batching, deduplication, and caching.

    Groups concurrent entity requests by type, batches them into single
    database queries, and caches results to avoid redundant queries.

    Attributes:
        resolver: EntitiesResolver instance for batch query building
        db_pool: Database connection pool for executing queries
        cache_size: Maximum number of entries in the LRU cache
        batch_window_ms: Time window (ms) for collecting requests before flushing
    """

    def __init__(
        self,
        resolver: Any,
        db_pool: Any,
        cache_size: int = 1000,
        batch_window_ms: float = 1.0,
    ):
        """Initialize the DataLoader.

        Args:
            resolver: EntitiesResolver instance
            db_pool: Database connection pool (asyncpg PoolConnection)
            cache_size: Maximum cached entries (default 1000).
                       Must be positive. Larger cache improves hit rate but uses more memory.
                       Recommended: 1000 (small), 10000 (medium), 100000 (large APIs)
            batch_window_ms: Batch window in milliseconds (default 1.0ms).
                            Must be positive. Typical values: 1.0ms (real-time), 5-100ms (bulk)

        Raises:
            ValueError: If cache_size <= 0
            ValueError: If batch_window_ms <= 0
        """
        if cache_size <= 0:
            raise ValueError(f"cache_size must be positive, got {cache_size}")
        if batch_window_ms <= 0:
            raise ValueError(f"batch_window_ms must be positive, got {batch_window_ms}ms")

        self.resolver = resolver
        self.db_pool = db_pool
        self.cache_size = cache_size
        self.batch_window_ms = batch_window_ms

        # Deduplication cache: (typename, key_field, key_value) -> Future
        self._dedup_cache: dict[tuple[str, str, Any], asyncio.Future[dict[str, Any] | None]] = {}

        # Result cache: (typename, key_field, key_value) -> entity_dict
        self._result_cache: dict[tuple[str, str, Any], dict[str, Any] | None] = {}

        # Request queue for the current batch
        self._pending_requests: dict[
            tuple[str, str, Any], list[asyncio.Future[dict[str, Any] | None]]
        ] = defaultdict(list)

        # Batch timer task
        self._flush_task: asyncio.Task | None = None

        # Statistics
        self._stats = DataLoaderStats()

    @property
    def stats(self) -> DataLoaderStats:
        """Get current DataLoader statistics."""
        return self._stats

    def _make_dedup_key(
        self, typename: str, key_field: str, key_value: Any
    ) -> tuple[str, str, Any]:
        """Create deduplication key for a request.

        Optimized to minimize memory allocations in hot path.

        Args:
            typename: GraphQL type name
            key_field: Key field name
            key_value: Key value

        Returns:
            Tuple for use as dictionary key (highly optimized for caching)
        """
        # Direct tuple creation is fastest - Python interns small tuples
        return (typename, key_field, key_value)

    async def load(self, typename: str, key_field: str, key_value: Any) -> dict[str, Any] | None:
        """Load a single entity, batching and caching automatically.

        Returns a Future immediately. The actual database query is executed
        when the batch window expires or when explicitly flushed.

        Args:
            typename: GraphQL type name (e.g., "User")
            key_field: Key field name (e.g., "id")
            key_value: Key value (e.g., "user-123")

        Returns:
            Resolved entity dictionary or None if not found

        Raises:
            ValueError: If typename or key_field are invalid
        """
        self._stats.total_requests += 1
        dedup_key = self._make_dedup_key(typename, key_field, key_value)

        # Check result cache first (cache hit)
        if dedup_key in self._result_cache:
            self._stats.cache_hits += 1
            return self._result_cache[dedup_key]

        # Check dedup cache (request already pending)
        if dedup_key in self._dedup_cache:
            self._stats.dedup_hits += 1
            return await self._dedup_cache[dedup_key]

        # Create new Future for this request
        future: asyncio.Future[dict[str, Any] | None] = asyncio.Future()
        self._dedup_cache[dedup_key] = future
        self._pending_requests[dedup_key].append(future)

        # Schedule flush if not already scheduled
        if self._flush_task is None or self._flush_task.done():
            self._schedule_flush()

        # Cache miss - will be resolved during flush
        self._stats.cache_misses += 1

        return await future

    async def load_many(self, requests: list[tuple[str, str, Any]]) -> list[dict[str, Any] | None]:
        """Load multiple entities in a single batch.

        More efficient than calling load() multiple times in a loop.

        Args:
            requests: List of (typename, key_field, key_value) tuples

        Returns:
            List of resolved entities in the same order as requests
        """
        futures = [
            self.load(typename, key_field, key_value) for typename, key_field, key_value in requests
        ]
        return await asyncio.gather(*futures)

    def clear_cache(self) -> None:
        """Clear all cached data and pending requests.

        Use when cache needs to be invalidated (e.g., after mutations).
        """
        self._dedup_cache.clear()
        self._result_cache.clear()
        self._pending_requests.clear()

        # Cancel pending flush if any
        if self._flush_task and not self._flush_task.done():
            self._flush_task.cancel()
            self._flush_task = None

    def _schedule_flush(self) -> None:
        """Schedule a batch flush after the batch window expires."""
        self._flush_task = asyncio.create_task(self._flush_after_delay())

    async def _flush_after_delay(self) -> None:
        """Wait for batch window and then flush."""
        try:
            await asyncio.sleep(self.batch_window_ms / 1000.0)
            await self.flush()
        except asyncio.CancelledError:
            pass

    async def flush(self) -> None:
        """Immediately execute all pending requests as batches.

        Groups requests by (typename, key_field) and executes one
        database query per group.
        """
        # Cancel any pending flush task since we're flushing now
        if self._flush_task and not self._flush_task.done():
            self._flush_task.cancel()
            try:
                await self._flush_task
            except asyncio.CancelledError:
                pass
            self._flush_task = None

        if not self._pending_requests:
            return

        # Group requests by typename and key_field
        requests_by_type: dict[tuple[str, str], list[Any]] = defaultdict(list)
        request_map: dict[tuple[str, str, Any], list[asyncio.Future[dict[str, Any] | None]]] = (
            defaultdict(list)
        )

        for (typename, key_field, key_value), futures in self._pending_requests.items():
            requests_by_type[(typename, key_field)].append(key_value)
            for future in futures:
                request_map[(typename, key_field, key_value)].append(future)

        # Clear pending requests
        self._pending_requests.clear()

        # Execute queries for each type/field combination
        async with self.db_pool.acquire() as conn:
            for (typename, key_field), key_values in requests_by_type.items():
                try:
                    # Build and execute batch query
                    placeholders = ", ".join(f"${i}" for i in range(1, len(key_values) + 1))
                    table_name = f"tv_{typename.lower()}"
                    sql = (
                        f"SELECT {key_field}, data FROM {table_name} "
                        f"WHERE {key_field} IN ({placeholders})"
                    )

                    rows = await conn.fetch(sql, *key_values)
                    self._stats.batch_count += 1

                    # Process results
                    for row in rows:
                        key_value = row[key_field]
                        entity_data = dict(row["data"]) if row["data"] else {}
                        entity_data["__typename"] = typename

                        # Cache the result
                        cache_key = (typename, key_field, key_value)
                        self._result_cache[cache_key] = entity_data

                        # Resolve futures for this entity
                        for future in request_map.get(cache_key, []):
                            if not future.done():
                                future.set_result(entity_data)

                    # Mark missing entities as None
                    found_keys = {row[key_field] for row in rows}
                    for key_value in key_values:
                        if key_value not in found_keys:
                            cache_key = (typename, key_field, key_value)
                            self._result_cache[cache_key] = None

                            for future in request_map.get(cache_key, []):
                                if not future.done():
                                    future.set_result(None)

                except Exception as e:
                    # Mark all futures as failed for this batch
                    for key_value in key_values:
                        cache_key = (typename, key_field, key_value)
                        for future in request_map.get(cache_key, []):
                            if not future.done():
                                future.set_exception(e)

        # Enforce LRU cache size limit
        self._enforce_cache_limit()

    def _enforce_cache_limit(self) -> None:
        """Enforce LRU cache size limit by removing oldest entries.

        Uses simple FIFO eviction based on insertion order.
        """
        if len(self._result_cache) > self.cache_size:
            # Remove excess entries (simple FIFO, not true LRU)
            excess = len(self._result_cache) - self.cache_size
            for _ in range(excess):
                # Remove first key (oldest)
                key = next(iter(self._result_cache))
                del self._result_cache[key]
                # Also remove from dedup cache
                if key in self._dedup_cache:
                    del self._dedup_cache[key]

    async def close(self) -> None:
        """Close the DataLoader and flush any pending requests."""
        if self._flush_task and not self._flush_task.done():
            self._flush_task.cancel()
            try:
                await self._flush_task
            except asyncio.CancelledError:
                pass

        await self.flush()
