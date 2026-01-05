"""Async batch execution engine for DataLoader.

Manages batch execution windows and context for efficient
grouped query execution. Provides context managers for
automatic batch flushing at request boundaries.

Example:
    >>> executor = BatchExecutor(batch_window_ms=1.0)
    >>> loader = EntityDataLoader(resolver, db_pool)
    >>> async with executor.batch_context(loader):
    ...     # All loader.load() calls batch until context exit
    ...     user = await loader.load("User", "id", "user-1")
    ...     post = await loader.load("Post", "id", "post-1")
    ...     # Batch flushes automatically on exit
"""

import asyncio
import contextvars
from contextlib import asynccontextmanager
from typing import Any

from fraiseql.federation.dataloader import EntityDataLoader

# Context variable to track active batches
_active_loader: contextvars.ContextVar[EntityDataLoader | None] = contextvars.ContextVar(
    "_active_loader",
    default=None,
)


class BatchExecutor:
    """Manages async batch execution windows for DataLoader.

    Provides context managers and utilities for grouping requests
    into batch windows for efficient execution.

    Attributes:
        batch_window_ms: Time window (ms) for collecting requests
        max_batch_size: Maximum requests per batch (optional)
    """

    def __init__(self, batch_window_ms: float = 1.0, max_batch_size: int | None = None):
        """Initialize the batch executor.

        Args:
            batch_window_ms: Batch window in milliseconds (default 1.0ms).
                            Must be positive. Typical values: 1.0-100.0ms
            max_batch_size: Maximum requests per batch (optional).
                           If specified, must be positive. No limit if None.

        Raises:
            ValueError: If batch_window_ms <= 0
            ValueError: If max_batch_size is not None and <= 0
        """
        if batch_window_ms <= 0:
            raise ValueError(f"batch_window_ms must be positive, got {batch_window_ms}ms")
        if max_batch_size is not None and max_batch_size <= 0:
            raise ValueError(f"max_batch_size must be positive, got {max_batch_size}")

        self.batch_window_ms = batch_window_ms
        self.max_batch_size = max_batch_size

    async def batch_execute(
        self,
        requests: list[tuple],
        resolver: Any,
        db_pool: Any,
    ) -> list[Any | None]:
        """Execute a list of entity resolution requests as a single batch.

        Args:
            requests: List of (typename, key_field, key_value) tuples
            resolver: EntitiesResolver instance
            db_pool: Database connection pool

        Returns:
            List of resolved entities in the same order as requests
        """
        loader = EntityDataLoader(resolver, db_pool, batch_window_ms=self.batch_window_ms)

        try:
            # Load all requests
            results = await loader.load_many(requests)
            return results
        finally:
            await loader.close()

    @asynccontextmanager
    async def batch_context(self, loader: EntityDataLoader) -> Any:
        """Async context manager for batch execution.

        Automatically flushes loader on context exit.

        Example:
            >>> async with executor.batch_context(loader):
            ...     user = await loader.load("User", "id", "1")
            ...     post = await loader.load("Post", "id", "1")
            ...     # Batch flushes on exit

        Args:
            loader: EntityDataLoader instance
        """
        token = _active_loader.set(loader)
        try:
            yield loader
        finally:
            await loader.flush()
            _active_loader.reset(token)

    @staticmethod
    def get_active_loader() -> EntityDataLoader | None:
        """Get the currently active DataLoader in this context.

        Returns:
            Active EntityDataLoader or None if not in batch context
        """
        return _active_loader.get()

    @staticmethod
    async def flush_active() -> None:
        """Flush the currently active DataLoader if one exists."""
        loader = _active_loader.get()
        if loader:
            await loader.flush()


class PerRequestBatchExecutor(BatchExecutor):
    """Batch executor with per-request DataLoader lifecycle.

    Creates a new DataLoader for each request and flushes it
    automatically. Useful for HTTP request handlers.

    Example:
        >>> executor = PerRequestBatchExecutor()
        >>> async def handle_graphql_request(request):
        ...     results = await executor.execute_request(
        ...         request_handler,
        ...         resolver,
        ...         db_pool
        ...     )
    """

    async def execute_request(
        self,
        request_handler: Any,
        resolver: Any,
        db_pool: Any,
    ) -> Any:
        """Execute a request with automatic DataLoader lifecycle.

        Creates a DataLoader, executes the request handler with it in context,
        and automatically flushes on completion.

        Args:
            request_handler: Async callable that uses the loader
            resolver: EntitiesResolver instance
            db_pool: Database connection pool

        Returns:
            Result from request_handler
        """
        loader = EntityDataLoader(resolver, db_pool, batch_window_ms=self.batch_window_ms)

        async with self.batch_context(loader):
            return await request_handler(loader)


class ConcurrentBatchExecutor(BatchExecutor):
    """Batch executor that supports multiple concurrent batches.

    Useful for handling multiple concurrent requests each with
    their own batch window.

    Example:
        >>> executor = ConcurrentBatchExecutor()
        >>> tasks = [
        ...     executor.batch_execute(requests1, resolver, db_pool),
        ...     executor.batch_execute(requests2, resolver, db_pool),
        ...     executor.batch_execute(requests3, resolver, db_pool),
        ... ]
        >>> results = await asyncio.gather(*tasks)
    """

    async def execute_concurrent(
        self,
        request_groups: list[list[tuple]],
        resolver: Any,
        db_pool: Any,
    ) -> list[list[Any | None]]:
        """Execute multiple request groups concurrently.

        Each group is executed in its own batch with its own DataLoader.

        Args:
            request_groups: List of request lists
            resolver: EntitiesResolver instance
            db_pool: Database connection pool

        Returns:
            List of result lists in the same order as request groups
        """
        tasks = [self.batch_execute(requests, resolver, db_pool) for requests in request_groups]
        return await asyncio.gather(*tasks)

    async def execute_grouped(
        self,
        requests: list[tuple],
        resolver: Any,
        db_pool: Any,
        group_by: str = "typename",
    ) -> list[Any | None]:
        """Execute requests grouped by a criterion.

        Groups requests (e.g., by typename) and executes each group
        concurrently for better parallelization.

        Args:
            requests: List of (typename, key_field, key_value) tuples
            resolver: EntitiesResolver instance
            db_pool: Database connection pool
            group_by: Grouping criterion ("typename" or "key_field")

        Returns:
            List of resolved entities in original order
        """
        if group_by == "typename":
            # Group by typename
            groups = {}
            indices = {}
            for i, (typename, key_field, key_value) in enumerate(requests):
                if typename not in groups:
                    groups[typename] = []
                    indices[typename] = []
                groups[typename].append((typename, key_field, key_value))
                indices[typename].append(i)

            # Execute all groups concurrently
            group_results = await self.execute_concurrent(list(groups.values()), resolver, db_pool)

            # Reconstruct original order
            results = [None] * len(requests)
            for group_typename, group_res in zip(groups.keys(), group_results, strict=True):
                for idx, res in zip(indices[group_typename], group_res, strict=True):
                    results[idx] = res

            return results
        # Fall back to single batch for other grouping criteria
        return await self.batch_execute(requests, resolver, db_pool)
