"""Test DataLoader implementation."""

import asyncio
from typing import Never
from unittest.mock import AsyncMock, Mock

import pytest

from fraiseql.optimization import DataLoader, LoaderRegistry, get_loader
from fraiseql.optimization.loaders import UserLoader

pytestmark = pytest.mark.integration


class TestDataLoader:
    """Test DataLoader functionality."""

    @pytest.mark.asyncio
    async def test_basic_batching(self) -> None:
        """Test that multiple loads are batched."""
        batch_fn = AsyncMock(return_value=["a", "b", "c"])

        class TestLoader(DataLoader):
            async def batch_load(self, keys) -> None:
                return await batch_fn(keys)

        loader = TestLoader()

        # Load three keys concurrently
        results = await asyncio.gather(loader.load(1), loader.load(2), loader.load(3))

        # Should batch into single call
        assert batch_fn.call_count == 1
        assert batch_fn.call_args[0][0] == [1, 2, 3]
        assert results == ["a", "b", "c"]

    @pytest.mark.asyncio
    async def test_caching(self) -> None:
        """Test that results are cached."""
        call_count = 0

        class TestLoader(DataLoader):
            async def batch_load(self, keys) -> None:
                nonlocal call_count
                call_count += 1
                return [f"value_{k}" for k in keys]

        loader = TestLoader()

        # First load
        result1 = await loader.load(1)
        assert result1 == "value_1"
        assert call_count == 1

        # Second load should use cache
        result2 = await loader.load(1)
        assert result2 == "value_1"
        assert call_count == 1  # No additional call

    @pytest.mark.asyncio
    async def test_deduplication(self) -> None:
        """Test that duplicate keys are deduplicated."""
        batch_fn = AsyncMock(return_value=["a", "b"])

        class TestLoader(DataLoader):
            async def batch_load(self, keys) -> None:
                return await batch_fn(keys)

        loader = TestLoader()

        # Load same key multiple times
        results = await asyncio.gather(
            loader.load(1),
            loader.load(2),
            loader.load(1),  # Duplicate
            loader.load(2),  # Duplicate
        )

        # Should only request unique keys
        assert batch_fn.call_count == 1
        assert set(batch_fn.call_args[0][0]) == {1, 2}
        assert results == ["a", "b", "a", "b"]

    @pytest.mark.asyncio
    async def test_error_handling(self) -> None:
        """Test error propagation."""

        class TestLoader(DataLoader):
            async def batch_load(self, keys) -> Never:
                msg = "Batch load failed"
                raise ValueError(msg)

        loader = TestLoader()

        # All loads should fail with sanitized error for security
        with pytest.raises(RuntimeError, match="DataLoader batch operation failed"):
            await asyncio.gather(loader.load(1), loader.load(2))

    @pytest.mark.asyncio
    async def test_max_batch_size(self) -> None:
        """Test batch size limiting."""
        batches_called = []

        class TestLoader(DataLoader):
            def __init__(self) -> None:
                super().__init__(max_batch_size=2)

            async def batch_load(self, keys) -> None:
                batches_called.append(keys)
                return [f"value_{k}" for k in keys]

        loader = TestLoader()

        # Load 5 items
        await asyncio.gather(*[loader.load(i) for i in range(5)])

        # Should split into 3 batches
        assert len(batches_called) == 3
        assert len(batches_called[0]) <= 2
        assert len(batches_called[1]) <= 2
        assert len(batches_called[2]) <= 2


class TestLoaderRegistry:
    """Test DataLoader registry."""

    def test_registry_creation(self) -> None:
        """Test creating loader registry."""
        db = Mock()
        registry = LoaderRegistry(db)

        assert registry.db == db
        assert len(registry._loaders) == 0

    def test_get_loader(self) -> None:
        """Test getting loader from registry."""
        db = Mock()
        registry = LoaderRegistry(db)

        # Get loader
        loader = registry.get_loader(UserLoader)

        assert isinstance(loader, UserLoader)
        assert loader.db == db

        # Get same loader again
        loader2 = registry.get_loader(UserLoader)
        assert loader2 is loader  # Same instance

    def test_context_var(self) -> None:
        """Test context variable for registry."""
        db = Mock()
        registry = LoaderRegistry(db)

        # Set current
        LoaderRegistry.set_current(registry)

        # Get current
        current = LoaderRegistry.get_current()
        assert current is registry

        # Test helper function
        loader = get_loader(UserLoader)
        assert isinstance(loader, UserLoader)
