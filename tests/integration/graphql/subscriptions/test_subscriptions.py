"""Test GraphQL subscriptions functionality."""

import asyncio
from collections.abc import AsyncGenerator

import pytest

from fraiseql import subscription
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.subscriptions import cache, complexity
from fraiseql.subscriptions import filter as sub_filter

pytestmark = pytest.mark.integration


class TestSubscriptionDecorator:
    """Test subscription decorator functionality."""

    def test_subscription_registration(self) -> None:
        """Test that subscriptions are registered correctly."""
        registry = SchemaRegistry.get_instance()
        registry.clear()

        @subscription
        @pytest.mark.asyncio
        async def test_subscription(info) -> AsyncGenerator[str]:
            """Test subscription."""
            yield "test"

        assert "test_subscription" in registry._subscriptions
        assert registry._subscriptions["test_subscription"] == test_subscription

    def test_non_async_generator_error(self) -> None:
        """Test error when subscription is not an async generator."""
        with pytest.raises(TypeError, match="must be an async generator"):

            @subscription
            def not_async_gen(info) -> str:
                return "test"

    @pytest.mark.asyncio
    async def test_subscription_execution(self) -> None:
        """Test subscription execution."""

        @subscription
        async def countdown(info, start: int = 3) -> AsyncGenerator[int]:
            """Count down from start."""
            for i in range(start, 0, -1):
                yield i
                await asyncio.sleep(0.01)

        # Collect results
        results = []
        async for value in countdown(None, start=3):
            results.append(value)

        assert results == [3, 2, 1]


class TestSubscriptionComplexity:
    """Test subscription complexity analysis."""

    @pytest.mark.asyncio
    async def test_complexity_decorator(self) -> None:
        """Test complexity limiting."""

        @subscription
        @complexity(score=10, max_depth=3)
        async def limited_subscription(info) -> AsyncGenerator[str]:
            yield "allowed"

        # Should work within limits
        async for _ in limited_subscription(None):
            pass  # Should not raise

    def test_complexity_calculation(self) -> None:
        """Test complexity score calculation."""
        from fraiseql.subscriptions.complexity import SubscriptionComplexityAnalyzer

        analyzer = SubscriptionComplexityAnalyzer()

        # Mock GraphQL info
        class MockInfo:
            class MockField:
                selection_set = None

            field_nodes = [MockField()]
            fragments = {}

        score = analyzer.calculate_complexity(MockInfo(), "test_field", {"first": 10})

        assert score == 10  # Base cost * limit


class TestSubscriptionFiltering:
    """Test subscription filtering."""

    @pytest.mark.asyncio
    async def test_filter_decorator_success(self) -> None:
        """Test filter allowing access."""

        @subscription
        @sub_filter("True")  # Always allow
        async def filtered_sub(info) -> AsyncGenerator[str]:
            yield "allowed"

        # Should allow access
        results = []
        async for value in filtered_sub(None):
            results.append(value)

        assert results == ["allowed"]

    @pytest.mark.asyncio
    async def test_filter_decorator_denied(self) -> None:
        """Test filter denying access."""

        @subscription
        @sub_filter("False")  # Always deny
        async def denied_sub(info) -> AsyncGenerator[str]:
            yield "denied"

        # Should raise permission error
        with pytest.raises(PermissionError):
            async for _ in denied_sub(None):
                pass


class TestSubscriptionCaching:
    """Test subscription result caching."""

    @pytest.mark.asyncio
    async def test_cache_decorator(self) -> None:
        """Test caching of subscription results."""
        call_count = 0

        @subscription
        @cache(ttl=1.0)
        async def cached_sub(info) -> AsyncGenerator[int]:
            nonlocal call_count
            call_count += 1
            yield call_count

        # Mock context with cache
        class MockInfo:
            context = {"subscription_cache": None}  # No cache for test

        # First call
        results1 = []
        async for value in cached_sub(MockInfo()):
            results1.append(value)

        assert results1 == [1]
        assert call_count == 1
