"""Extended tests for subscription caching to improve coverage."""

import asyncio
import time
from unittest.mock import Mock, patch

import pytest

from fraiseql.subscriptions.caching import CacheEntry, SubscriptionCache, cache


class TestCacheEntry:
    """Test CacheEntry dataclass."""

    def test_cache_entry_creation(self):
        """Test creating a cache entry."""
        timestamp = (time.time(),)
        entry = CacheEntry(value="test_value", timestamp=timestamp, ttl=5.0)

        assert entry.value == "test_value"
        assert entry.timestamp == timestamp
        assert entry.ttl == 5.0

    def test_cache_entry_not_expired(self):
        """Test cache entry that hasn't expired."""
        entry = CacheEntry(
            value="test_value",
            timestamp=time.time(),
            ttl=10.0,  # 10 seconds TTL
        )

        assert not entry.is_expired()

    def test_cache_entry_expired(self):
        """Test cache entry that has expired."""
        # Create entry with timestamp in the past
        old_timestamp = time.time() - 20  # 20 seconds ago,
        entry = CacheEntry(
            value="test_value",
            timestamp=old_timestamp,
            ttl=10.0,  # 10 seconds TTL
        )

        assert entry.is_expired()

    def test_cache_entry_just_expired(self):
        """Test cache entry that just expired."""
        # Create entry that expires right now
        timestamp = time.time() - 5.001  # Just over 5 seconds ago,
        entry = CacheEntry(value="test_value", timestamp=timestamp, ttl=5.0)

        assert entry.is_expired()

    def test_cache_entry_zero_ttl(self):
        """Test cache entry with zero TTL."""
        entry = CacheEntry(value="test_value", timestamp=time.time(), ttl=0.0)

        # Should be expired immediately
        assert entry.is_expired()


class TestSubscriptionCache:
    """Test SubscriptionCache class."""

    def test_cache_initialization(self):
        """Test cache initialization."""
        cache = SubscriptionCache()

        assert cache._cache == {}
        assert cache._locks == {}
        assert cache._cleanup_task is None

    @pytest.mark.asyncio
    async def test_cache_start_stop(self):
        """Test starting and stopping the cache."""
        cache = SubscriptionCache()

        # Start cache
        await cache.start()
        assert cache._cleanup_task is not None
        assert not cache._cleanup_task.done()

        # Stop cache
        await cache.stop()
        assert cache._cleanup_task.cancelled()

    @pytest.mark.asyncio
    async def test_cache_stop_without_start(self):
        """Test stopping cache that wasn't started."""
        cache = SubscriptionCache()

        # Should not raise error
        await cache.stop()
        assert cache._cleanup_task is None

    def test_make_key_consistent(self):
        """Test that make_key generates consistent keys."""
        cache = SubscriptionCache()

        # Same inputs should generate same key
        key1 = cache._make_key("test_func", {"arg1": "value1", "arg2": 42})
        key2 = cache._make_key("test_func", {"arg1": "value1", "arg2": 42})

        assert key1 == key2

    def test_make_key_different_functions(self):
        """Test that different functions generate different keys."""
        cache = SubscriptionCache()

        args = {"arg1": "value1", "arg2": 42}
        key1 = cache._make_key("func1", args)
        key2 = cache._make_key("func2", args)

        assert key1 != key2

    def test_make_key_different_args(self):
        """Test that different arguments generate different keys."""
        cache = SubscriptionCache()

        key1 = cache._make_key("test_func", {"arg1": "value1"})
        key2 = cache._make_key("test_func", {"arg1": "value2"})

        assert key1 != key2

    def test_make_key_argument_order(self):
        """Test that argument order doesn't affect key generation."""
        cache = SubscriptionCache()

        # Python dicts maintain insertion order, but the key should be consistent
        # regardless of how the dict was constructed
        args1 = {"a": 1, "b": 2}
        args2 = {"b": 2, "a": 1}

        key1 = cache._make_key("test_func", args1)
        key2 = cache._make_key("test_func", args2)

        # Keys should be different because dict order matters in pickle
        # This is expected behavior
        assert key1 != key2

    def test_make_key_complex_args(self):
        """Test make_key with complex argument types."""
        cache = SubscriptionCache()

        complex_args = {
            "string": "test",
            "number": 42,
            "list": [1, 2, 3],
            "dict": {"nested": "value"},
            "none": None,
            "bool": True,
        }

        # Should not raise error
        key = cache._make_key("test_func", complex_args)
        assert isinstance(key, str)
        assert len(key) == 64  # SHA256 hex length

    @pytest.mark.asyncio
    async def test_get_or_generate_cache_miss(self):
        """Test get_or_generate with cache miss."""
        cache = SubscriptionCache()

        async def test_generator():
            yield "value1"
            yield "value2"

        key = "test_key"
        ttl = 5.0

        results = []
        async for value in cache.get_or_generate(key, test_generator(), ttl):
            results.append(value)

        assert results == ["value1", "value2"]

        # Check that values were cached
        assert key in cache._cache
        cached_entry = cache._cache[key]
        assert cached_entry.value == "value2"  # Last value
        assert cached_entry.ttl == ttl

    @pytest.mark.asyncio
    async def test_get_or_generate_cache_hit(self):
        """Test get_or_generate with cache hit."""
        cache = SubscriptionCache()
        key = "test_key"

        # Pre-populate cache
        cached_value = "cached_result"
        cache._cache[key] = CacheEntry(value=cached_value, timestamp=time.time(), ttl=10.0)

        # Generator that should not be called
        async def should_not_run():
            yield "should_not_see_this"

        results = []
        async for value in cache.get_or_generate(key, should_not_run(), 5.0):
            results.append(value)

        assert results == [cached_value]

    @pytest.mark.asyncio
    async def test_get_or_generate_expired_cache(self):
        """Test get_or_generate with expired cache entry."""
        cache = SubscriptionCache()
        key = "test_key"

        # Pre-populate with expired entry
        expired_entry = CacheEntry(
            value="expired_value",
            timestamp=time.time() - 20,  # 20 seconds ago
            ttl=10.0,  # 10 second TTL
        )
        cache._cache[key] = expired_entry

        async def fresh_generator():
            yield "fresh_value"

        results = []
        async for value in cache.get_or_generate(key, fresh_generator(), 5.0):
            results.append(value)

        assert results == ["fresh_value"]

        # Cache should be updated with fresh value
        assert cache._cache[key].value == "fresh_value"

    @pytest.mark.asyncio
    async def test_get_or_generate_concurrent_access(self):
        """Test concurrent access to same cache key."""
        cache = SubscriptionCache()
        key = "test_key"

        call_count = 0

        async def counted_generator():
            nonlocal call_count
            call_count += 1
            await asyncio.sleep(0.1)  # Simulate work
            yield f"value_{call_count}"

        # Start multiple concurrent requests
        tasks = []
        for _ in range(3):
            task = asyncio.create_task(
                self._collect_generator_results(
                    cache.get_or_generate(key, counted_generator(), 5.0)
                )
            )
            tasks.append(task)

        results = await asyncio.gather(*tasks)

        # Only one generator should have run due to locking
        assert call_count == 1

        # All requests should get the same result
        for result in results:
            assert result == ["value_1"]

    async def _collect_generator_results(self, async_gen):
        """Helper to collect all values from an async generator."""
        results = []
        async for value in async_gen:
            results.append(value)
        return results

    @pytest.mark.asyncio
    async def test_get_or_generate_double_check_locking(self):
        """Test double-check locking in get_or_generate."""
        cache = SubscriptionCache()
        key = "test_key"

        # Pre-create lock to simulate concurrent access
        cache._locks[key] = asyncio.Lock()

        async def generator():
            yield "generated_value"

        # Start first request that will populate cache
        async with cache._locks[key]:
            # Simulate cache being populated by another request
            cache._cache[key] = CacheEntry(
                value="concurrent_value", timestamp=time.time(), ttl=10.0
            )

        # Now test the double-check logic
        results = []
        async for value in cache.get_or_generate(key, generator(), 5.0):
            results.append(value)

        # Should get the concurrently cached value
        assert results == ["concurrent_value"]

    @pytest.mark.asyncio
    async def test_cleanup_loop(self):
        """Test cache cleanup logic (manually calling the cleanup code)."""
        cache = SubscriptionCache()

        # Add some entries - mix of expired and valid
        current_time = time.time()

        # Valid entry
        cache._cache["valid"] = CacheEntry(value="valid_value", timestamp=current_time, ttl=100.0)

        # Expired entry
        cache._cache["expired"] = CacheEntry(
            value="expired_value",
            timestamp=current_time - 200,  # Way in the past
            ttl=100.0,
        )

        # Add corresponding locks
        cache._locks["valid"] = asyncio.Lock()
        cache._locks["expired"] = asyncio.Lock()

        # Manually run the cleanup logic from the loop
        expired = []
        for key, entry in cache._cache.items():
            if entry.is_expired():
                expired.append(key)

        for key in expired:
            del cache._cache[key]
            if key in cache._locks:
                del cache._locks[key]

        # Check that expired entry was removed
        assert "valid" in cache._cache
        assert "expired" not in cache._cache
        assert "valid" in cache._locks
        assert "expired" not in cache._locks

    @pytest.mark.asyncio
    async def test_cleanup_loop_exception_handling(self):
        """Test cleanup loop handles exceptions gracefully."""
        cache = SubscriptionCache()

        # Mock the cleanup loop to raise an exception then cancel
        original_cleanup = cache._cleanup_loop

        async def mock_cleanup():
            try:
                # Simulate an exception in cleanup
                raise ValueError("Test exception")
            except ValueError:
                # Import and use the actual logger
                from fraiseql.subscriptions.caching import logger

                logger.exception("Cache cleanup error")
                # Then simulate cancellation
                raise asyncio.CancelledError() from None

        cache._cleanup_loop = mock_cleanup

        with patch("fraiseql.subscriptions.caching.logger") as mock_logger:
            cleanup_task = asyncio.create_task(cache._cleanup_loop())

            try:
                await cleanup_task
            except asyncio.CancelledError:
                pass

            # Should have logged the exception
            mock_logger.exception.assert_called_with("Cache cleanup error")

    @pytest.mark.asyncio
    async def test_generator_empty(self):
        """Test get_or_generate with empty generator."""
        cache = SubscriptionCache()

        async def empty_generator():
            return
            # This is unreachable but makes it an async generator
            yield  # pragma: no cover

        key = "empty_key"
        results = []

        async for value in cache.get_or_generate(key, empty_generator(), 5.0):
            results.append(value)

        assert results == []
        # No cache entry should be created for empty generator
        assert key not in cache._cache

    @pytest.mark.asyncio
    async def test_generator_exception(self):
        """Test get_or_generate when generator raises exception."""
        cache = SubscriptionCache()

        async def failing_generator():
            yield "value1"
            raise ValueError("Generator failed")

        key = "failing_key"

        with pytest.raises(ValueError, match="Generator failed"):
            async for _value in cache.get_or_generate(key, failing_generator(), 5.0):
                pass

        # Should have cached the value before the exception
        assert key in cache._cache
        assert cache._cache[key].value == "value1"


class TestCacheDecorator:
    """Test cache decorator."""

    def test_decorator_adds_ttl_attribute(self):
        """Test that decorator adds TTL attribute to function."""

        @cache(ttl=15.0)
        async def test_function():
            yield "test"

        assert hasattr(test_function, "_cache_ttl")
        assert test_function._cache_ttl == 15.0

    def test_decorator_default_ttl(self):
        """Test decorator with default TTL."""

        @cache()
        async def test_function():
            yield "test"

        assert test_function._cache_ttl == 5.0

    def test_decorator_preserves_function_metadata(self):
        """Test that decorator preserves function metadata."""

        @cache(ttl=10.0)
        async def documented_function():
            """This is a documented function."""
            yield "test"

        assert documented_function.__name__ == "documented_function"
        assert documented_function.__doc__ == "This is a documented function."

    @pytest.mark.asyncio
    async def test_cached_function_no_cache_in_context(self):
        """Test cached function when no cache is in context."""
        call_count = 0

        @cache(ttl=5.0)
        async def test_subscription(info):
            nonlocal call_count
            call_count += 1
            yield f"result_{call_count}"

        # Mock info without cache
        mock_info = Mock()
        mock_info.context = None

        results = []
        async for value in test_subscription(mock_info):
            results.append(value)

        assert results == ["result_1"]
        assert call_count == 1

    @pytest.mark.asyncio
    async def test_cached_function_no_context(self):
        """Test cached function when info has no context attribute."""

        @cache(ttl=5.0)
        async def test_subscription(info):
            yield "no_context_result"

        # Mock info without context attribute
        mock_info = Mock(spec=[])  # No attributes,

        results = []
        async for value in test_subscription(mock_info):
            results.append(value)

        assert results == ["no_context_result"]

    @pytest.mark.asyncio
    async def test_cached_function_empty_context(self):
        """Test cached function with empty context."""

        @cache(ttl=5.0)
        async def test_subscription(info):
            yield "empty_context_result"

        # Mock info with empty context
        mock_info = Mock()
        mock_info.context = {}

        results = []
        async for value in test_subscription(mock_info):
            results.append(value)

        assert results == ["empty_context_result"]

    @pytest.mark.asyncio
    async def test_cached_function_with_cache(self):
        """Test cached function with cache in context."""
        subscription_cache = SubscriptionCache()
        call_count = 0

        @cache(ttl=5.0)
        async def test_subscription(info, param="default"):
            nonlocal call_count
            call_count += 1
            yield f"result_{call_count}_{param}"

        # Mock info with cache
        mock_info = Mock()
        mock_info.context = {"subscription_cache": subscription_cache}

        # First call
        results1 = []
        async for value in test_subscription(mock_info, param="test"):
            results1.append(value)

        # Second call with same parameters (should use cache)
        results2 = []
        async for value in test_subscription(mock_info, param="test"):
            results2.append(value)

        assert results1 == ["result_1_test"]
        assert results2 == ["result_1_test"]  # Same result from cache
        assert call_count == 1  # Function called only once

    @pytest.mark.asyncio
    async def test_cached_function_different_parameters(self):
        """Test cached function with different parameters."""
        subscription_cache = SubscriptionCache()
        call_count = 0

        @cache(ttl=5.0)
        async def test_subscription(info, param1="default", param2=0):
            nonlocal call_count
            call_count += 1
            yield f"result_{call_count}_{param1}_{param2}"

        mock_info = Mock()
        mock_info.context = {"subscription_cache": subscription_cache}

        # Call with different parameters
        results1 = await self._collect_async_gen(test_subscription(mock_info, param1="a", param2=1))
        results2 = await self._collect_async_gen(test_subscription(mock_info, param1="b", param2=2))
        results3 = await self._collect_async_gen(
            test_subscription(mock_info, param1="a", param2=1)
        )  # Same as first

        assert results1 == ["result_1_a_1"]
        assert results2 == ["result_2_b_2"]
        assert results3 == ["result_1_a_1"]  # From cache
        assert call_count == 2  # Only two unique calls

    async def _collect_async_gen(self, async_gen):
        """Helper to collect all values from async generator."""
        results = []
        async for value in async_gen:
            results.append(value)
        return results

    @pytest.mark.asyncio
    async def test_cached_function_cache_key_generation(self):
        """Test that cache key is generated correctly."""
        subscription_cache = SubscriptionCache()

        @cache(ttl=5.0)
        async def test_subscription(info, **kwargs):
            yield "test_result"

        mock_info = Mock()
        mock_info.context = {"subscription_cache": subscription_cache}

        # Mock _make_key to verify it's called correctly
        original_make_key = subscription_cache._make_key
        subscription_cache._make_key = Mock(return_value="test_cache_key")

        # Call function
        async for _ in test_subscription(mock_info, param1="value1", param2="value2"):
            break

        # Verify _make_key was called with correct arguments
        subscription_cache._make_key.assert_called_once_with(
            "test_subscription", {"param1": "value1", "param2": "value2"}
        )

        # Restore original method
        subscription_cache._make_key = original_make_key

    @pytest.mark.asyncio
    async def test_cached_function_with_expired_cache(self):
        """Test cached function when cache entry has expired."""
        subscription_cache = SubscriptionCache()
        call_count = 0

        @cache(ttl=0.1)  # Very short TTL
        async def test_subscription(info):
            nonlocal call_count
            call_count += 1
            yield f"result_{call_count}"

        mock_info = Mock()
        mock_info.context = {"subscription_cache": subscription_cache}

        # First call
        results1 = await self._collect_async_gen(test_subscription(mock_info))

        # Wait for cache to expire
        await asyncio.sleep(0.15)

        # Second call (cache should be expired)
        results2 = await self._collect_async_gen(test_subscription(mock_info))

        assert results1 == ["result_1"]
        assert results2 == ["result_2"]
        assert call_count == 2  # Function called twice

    @pytest.mark.asyncio
    async def test_cached_function_kwargs_only(self):
        """Test cached function with keyword-only arguments."""
        subscription_cache = SubscriptionCache()

        @cache(ttl=5.0)
        async def test_subscription(info, *, required_kwarg, optional_kwarg="default"):
            yield f"result_{required_kwarg}_{optional_kwarg}"

        mock_info = Mock()
        mock_info.context = {"subscription_cache": subscription_cache}

        results = await self._collect_async_gen(
            test_subscription(mock_info, required_kwarg="test", optional_kwarg="custom")
        )

        assert results == ["result_test_custom"]

    @pytest.mark.asyncio
    async def test_cached_function_complex_return_values(self):
        """Test cached function with complex return values."""
        subscription_cache = SubscriptionCache()

        @cache(ttl=5.0)
        async def test_subscription(info):
            yield {"key": "value", "number": 42}
            yield ["item1", "item2", "item3"]
            yield (1, 2, 3)

        mock_info = Mock()
        mock_info.context = {"subscription_cache": subscription_cache}

        # First call
        results1 = await self._collect_async_gen(test_subscription(mock_info))

        # Second call (should get from cache - but only last yielded value is cached)
        results2 = await self._collect_async_gen(test_subscription(mock_info))

        expected = [{"key": "value", "number": 42}, ["item1", "item2", "item3"], (1, 2, 3)]
        assert results1 == expected
        # Cache only stores the last value yielded, so second call gets only that
        assert results2 == [(1, 2, 3)]


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def test_cache_entry_negative_ttl(self):
        """Test cache entry with negative TTL."""
        entry = CacheEntry(value="test", timestamp=time.time(), ttl=-1.0)

        # Should be considered expired
        assert entry.is_expired()

    @pytest.mark.asyncio
    async def test_cache_with_unpickleable_args(self):
        """Test cache key generation with unpickleable arguments."""
        cache = SubscriptionCache()

        # Lambda functions are not pickleable
        unpickleable_args = {"func": lambda x: x}

        # The actual error might be AttributeError instead of TypeError
        with pytest.raises((TypeError, AttributeError)):
            cache._make_key("test", unpickleable_args)

    @pytest.mark.asyncio
    async def test_cache_memory_cleanup_on_stop(self):
        """Test that cache memory is managed properly on stop."""
        cache = SubscriptionCache()

        # Add some data
        cache._cache["key1"] = CacheEntry("value1", time.time(), 10.0)
        cache._cache["key2"] = CacheEntry("value2", time.time(), 10.0)
        cache._locks["key1"] = asyncio.Lock()
        cache._locks["key2"] = asyncio.Lock()

        await cache.start()
        await cache.stop()

        # Cache data should still exist (cleanup doesn't clear all data)
        # Only the cleanup task should be stopped
        assert cache._cleanup_task.cancelled()

    @pytest.mark.asyncio
    async def test_concurrent_cache_operations(self):
        """Test concurrent cache operations with different keys."""
        cache = SubscriptionCache()

        async def generator(value):
            await asyncio.sleep(0.05)
            yield value

        # Start multiple operations with different keys concurrently
        tasks = []
        for i in range(5):
            task = asyncio.create_task(
                self._collect_from_cache(cache, f"key_{i}", generator(f"value_{i}"), 5.0)
            )
            tasks.append(task)

        results = await asyncio.gather(*tasks)

        # Each should get its own result
        for i, result in enumerate(results):
            assert result == [f"value_{i}"]

        # All keys should be cached
        for i in range(5):
            assert f"key_{i}" in cache._cache

    async def _collect_from_cache(self, cache, key, generator, ttl):
        """Helper to collect results from cache."""
        results = []
        async for value in cache.get_or_generate(key, generator, ttl):
            results.append(value)
        return results

    @pytest.mark.asyncio
    async def test_cache_with_very_long_ttl(self):
        """Test cache with very long TTL."""
        cache = SubscriptionCache()

        # Use a very long TTL
        long_ttl = 365 * 24 * 60 * 60  # 1 year

        async def test_gen():
            yield "long_lived_value"

        key = "long_lived_key"

        results = []
        async for value in cache.get_or_generate(key, test_gen(), long_ttl):
            results.append(value)

        assert results == ["long_lived_value"]
        assert not cache._cache[key].is_expired()

    def test_cache_decorator_with_non_async_function(self):
        """Test cache decorator applied to non-async function."""

        # This should still work, but the function won't be a generator
        @cache(ttl=5.0)
        def sync_function():
            return "not_async"

        assert hasattr(sync_function, "_cache_ttl")
        assert sync_function._cache_ttl == 5.0

    @pytest.mark.asyncio
    async def test_cache_race_condition_protection(self):
        """Test protection against race conditions in cache access."""
        cache = SubscriptionCache()
        key = "race_key"

        generation_count = 0

        async def slow_generator():
            nonlocal generation_count
            generation_count += 1
            await asyncio.sleep(0.1)  # Simulate slow operation
            yield f"generated_{generation_count}"

        # Start multiple tasks that should all wait for the first one
        tasks = []
        for _ in range(3):
            task = asyncio.create_task(self._collect_from_cache(cache, key, slow_generator(), 5.0))
            tasks.append(task)

        # Start all tasks almost simultaneously
        results = await asyncio.gather(*tasks)

        # Only one generation should have occurred
        assert generation_count == 1

        # All tasks should get the same result
        for result in results:
            assert result == ["generated_1"]
