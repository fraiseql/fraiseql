"""DataLoader implementation for batch loading and caching."""

import asyncio
import logging
from abc import ABC, abstractmethod
from collections.abc import Callable, Hashable
from contextlib import asynccontextmanager
from typing import (
    Any,
    Generic,
    TypeVar,
)

K = TypeVar("K", bound=Hashable)
V = TypeVar("V")

logger = logging.getLogger(__name__)


class DataLoader(Generic[K, V], ABC):
    """Base class for batch loading and caching data.

    Prevents N+1 queries by batching and caching loads.
    """

    def __init__(
        self,
        batch_load_fn: Callable | None = None,
        max_batch_size: int = 1000,
        cache: bool = True,
        context: dict[str, Any] | None = None,
    ) -> None:
        self._batch_load_fn = batch_load_fn
        self._max_batch_size = max_batch_size
        self._cache_enabled = cache

        # Per-request state
        self._cache: dict[K, V] = {}
        self._queue: list[K] = []
        self._batch_promise: asyncio.Future | None = None
        self._dispatch_scheduled = False

    @abstractmethod
    async def batch_load(self, keys: list[K]) -> list[V | None]:
        """Load multiple keys in a single batch.

        Must return results in the same order as keys.
        Missing values should be None.
        """
        if self._batch_load_fn:
            return await self._batch_load_fn(keys)
        msg = "Must implement batch_load method"
        raise NotImplementedError(msg)

    async def load(self, key: K) -> V | None:
        """Load a single key, batching with other loads."""
        # Check cache first
        if self._cache_enabled and key in self._cache:
            return self._cache[key]

        # Add to queue
        self._queue.append(key)

        # Schedule batch dispatch
        if not self._dispatch_scheduled:
            self._dispatch_scheduled = True
            _ = asyncio.create_task(self._dispatch_batch())  # noqa: RUF006

        # Wait for batch to complete
        if not self._batch_promise:
            self._batch_promise = asyncio.Future()

        await self._batch_promise

        # Return from cache
        return self._cache.get(key)

    async def load_many(self, keys: list[K]) -> list[V | None]:
        """Load multiple keys."""
        tasks = [self.load(key) for key in keys]
        return await asyncio.gather(*tasks)

    async def prime(self, key: K, value: V) -> None:
        """Pre-populate cache with a known value."""
        if self._cache_enabled:
            self._cache[key] = value

    def clear(self, key: K | None = None) -> None:
        """Clear cache for a key or all keys."""
        if key is not None:
            self._cache.pop(key, None)
        else:
            self._cache.clear()

    async def _dispatch_batch(self) -> None:
        """Dispatch queued keys as a batch."""
        # CRITICAL FIX: Replace dangerous asyncio.sleep(0) with proper event loop yield
        # asyncio.sleep(0) can cause race conditions in high-concurrency scenarios
        try:
            # Use create_task + immediate await for safer context switching
            await asyncio.create_task(asyncio.sleep(0))
        except Exception:
            # Fallback: direct yield to event loop
            await asyncio.sleep(0.001)  # Minimum safe sleep

        # CRITICAL: Protect queue access with proper state management
        if not self._queue:
            self._dispatch_scheduled = False
            return

        # Get unique keys from queue atomically
        batch_keys = list(dict.fromkeys(self._queue))
        self._queue.clear()

        # Split into smaller batches if needed
        batches = [
            batch_keys[i : i + self._max_batch_size]
            for i in range(0, len(batch_keys), self._max_batch_size)
        ]

        try:
            # Load all batches
            all_results = []
            for batch in batches:
                results = await self.batch_load(batch)

                # Validate results
                if len(results) != len(batch):
                    msg = f"batch_load must return {len(batch)} results, got {len(results)}"
                    raise ValueError(
                        msg,
                    )

                # Cache results
                for key, value in zip(batch, results, strict=False):
                    if value is not None and self._cache_enabled:
                        self._cache[key] = value

                all_results.extend(results)

            # Resolve promise
            if self._batch_promise:
                self._batch_promise.set_result(None)

        except Exception as e:
            # CRITICAL: Properly handle exceptions to prevent information leakage
            # Log the actual error for debugging but don't expose internals
            logger.exception("DataLoader batch_load failed: %s", type(e).__name__)

            # Create safe exception for public consumption
            safe_exception = RuntimeError(
                "DataLoader batch operation failed. Check logs for details.",
            )

            # Reject promise with safe exception
            if self._batch_promise:
                self._batch_promise.set_exception(safe_exception)

        finally:
            # Reset state
            self._batch_promise = None
            self._dispatch_scheduled = False

    def sort_by_keys(
        self,
        items: list[dict[str, Any]],
        keys: list[K],
        key_field: str = "id",
    ) -> list[V | None]:
        """Helper to sort results to match key order."""
        # Create lookup map
        item_map = {item[key_field]: item for item in items}

        # Return in key order
        return [item_map.get(key) for key in keys]  # type: ignore[return-value]


@asynccontextmanager
async def dataloader_context():
    """Context manager for DataLoader usage.

    Example:
        async with dataloader_context() as ctx:
            loader = UserLoader(context=ctx)
            user = await loader.load(user_id)
    """
    # For now, just yield an empty dict as context
    # In a real app, this would integrate with request context
    context = {}
    try:
        yield context
    finally:
        # Cleanup if needed
        pass
