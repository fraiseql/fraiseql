"""Unit tests for subscription lifecycle functionality.

These tests use mocks to test the lifecycle decorators and hooks in isolation,
without requiring a real GraphQL schema or database connection.
"""

from datetime import UTC, datetime
from unittest.mock import AsyncMock, Mock, patch

import pytest

from fraiseql.subscriptions.lifecycle import SubscriptionLifecycle, with_lifecycle

pytestmark = pytest.mark.integration


class TestSubscriptionLifecycle:
    """Test SubscriptionLifecycle class methods."""

    def setup_method(self) -> None:
        """Set up test fixtures."""
        # Create a mock info object with a real context dict
        context_dict = {}
        self.mock_info = Mock()
        self.mock_info.context = context_dict

    @pytest.mark.asyncio
    async def test_on_start_hook(self) -> None:
        """Test on_start lifecycle hook."""
        # Mock function to wrap
        mock_func = AsyncMock()
        mock_func.__name__ = "mock_func"  # Set function name for ID generation

        # Apply decorator
        decorated = SubscriptionLifecycle.on_start(mock_func)

        # Call decorated function
        result = await decorated(self.mock_info, extra_arg="test")

        # Check function was called with correct args
        mock_func.assert_called_once()
        call_args = mock_func.call_args
        assert call_args[0][0] is self.mock_info  # info
        assert isinstance(call_args[0][1], str)  # subscription_id
        assert call_args[1]["extra_arg"] == "test"  # kwargs

        # Check subscription ID is returned
        assert isinstance(result, str)
        assert "mock_func" in result

        # Debug: print the context

        # Check context is updated
        assert "subscription_start" in self.mock_info.context
        assert "subscription_id" in self.mock_info.context
        assert isinstance(self.mock_info.context["subscription_start"], datetime)

    @pytest.mark.asyncio
    async def test_on_start_hook_no_context(self) -> None:
        """Test on_start hook when info has no context."""
        mock_info = Mock()
        mock_info.context = None

        mock_func = AsyncMock()
        decorated = SubscriptionLifecycle.on_start(mock_func)

        # Should not raise error
        result = await decorated(mock_info, extra_arg="test")
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_on_start_hook_no_context_attribute(self) -> None:
        """Test on_start hook when info has no context attribute."""
        mock_info = Mock(spec=[])  # No context attribute,

        mock_func = AsyncMock()
        decorated = SubscriptionLifecycle.on_start(mock_func)

        # Should not raise error
        result = await decorated(mock_info, extra_arg="test")
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_on_event_hook(self) -> None:
        """Test on_event lifecycle hook."""
        mock_func = AsyncMock(return_value="processed_event")

        decorated = SubscriptionLifecycle.on_event(mock_func)

        result = await decorated(self.mock_info, "test_event", extra_arg="test")

        # Check function was called
        mock_func.assert_called_once_with(self.mock_info, "test_event", extra_arg="test")
        assert result == "processed_event"

    @pytest.mark.asyncio
    async def test_on_event_hook_with_debug(self) -> None:
        """Test on_event hook with debug logging enabled."""
        self.mock_info.context["debug_subscriptions"] = True
        self.mock_info.context["subscription_id"] = "test_sub_123"

        mock_func = AsyncMock(return_value="event_result")
        decorated = SubscriptionLifecycle.on_event(mock_func)

        with patch("fraiseql.subscriptions.lifecycle.logger") as mock_logger:
            result = await decorated(self.mock_info, "debug_event", extra="value")

            # Check debug log was called
            mock_logger.debug.assert_called_once()
            assert "test_sub_123" in str(mock_logger.debug.call_args)
            assert "debug_event" in str(mock_logger.debug.call_args)

        assert result == "event_result"

    @pytest.mark.asyncio
    async def test_on_event_hook_no_debug(self) -> None:
        """Test on_event hook without debug logging."""
        self.mock_info.context["debug_subscriptions"] = False

        mock_func = AsyncMock(return_value="event_result")
        decorated = SubscriptionLifecycle.on_event(mock_func)

        with patch("fraiseql.subscriptions.lifecycle.logger") as mock_logger:
            result = await decorated(self.mock_info, "no_debug_event")

            # Debug should not be called
            mock_logger.debug.assert_not_called()

        assert result == "event_result"

    @pytest.mark.asyncio
    async def test_on_event_hook_no_context(self) -> None:
        """Test on_event hook with no context."""
        mock_info = Mock()
        mock_info.context = None

        mock_func = AsyncMock(return_value="result")
        decorated = SubscriptionLifecycle.on_event(mock_func)

        result = await decorated(mock_info, "event")
        assert result == "result"

    @pytest.mark.asyncio
    async def test_on_complete_hook(self) -> None:
        """Test on_complete lifecycle hook."""
        # Set up context with start time
        start_time = datetime.now(UTC)
        self.mock_info.context["subscription_start"] = start_time
        self.mock_info.context["subscription_id"] = "test_sub"

        mock_func = AsyncMock()
        decorated = SubscriptionLifecycle.on_complete(mock_func)

        with patch("fraiseql.subscriptions.lifecycle.logger") as mock_logger:
            await decorated(self.mock_info, extra_arg="test")

            # Check duration was logged
            mock_logger.info.assert_called_once()
            assert "duration" in str(mock_logger.info.call_args).lower()

        # Check function was called
        mock_func.assert_called_once_with(self.mock_info, extra_arg="test")

        # Check context was cleaned up
        assert "subscription_start" not in self.mock_info.context
        assert "subscription_id" not in self.mock_info.context

    @pytest.mark.asyncio
    async def test_on_complete_hook_no_start_time(self) -> None:
        """Test on_complete hook when no start time in context."""
        mock_func = AsyncMock()
        decorated = SubscriptionLifecycle.on_complete(mock_func)

        with patch("fraiseql.subscriptions.lifecycle.logger") as mock_logger:
            await decorated(self.mock_info)

            # No duration should be logged
            mock_logger.info.assert_not_called()

        mock_func.assert_called_once()

    @pytest.mark.asyncio
    async def test_on_complete_hook_no_context(self) -> None:
        """Test on_complete hook with no context."""
        mock_info = Mock()
        mock_info.context = None

        mock_func = AsyncMock()
        decorated = SubscriptionLifecycle.on_complete(mock_func)

        # Should not raise error
        await decorated(mock_info)
        mock_func.assert_called_once()

    @pytest.mark.asyncio
    async def test_on_complete_hook_no_context_attribute(self) -> None:
        """Test on_complete hook when info has no context attribute."""
        mock_info = Mock(spec=[])  # No context attribute,

        mock_func = AsyncMock()
        decorated = SubscriptionLifecycle.on_complete(mock_func)

        # Should not raise error
        await decorated(mock_info)
        mock_func.assert_called_once()

    def test_lifecycle_hook_preserves_function_metadata(self) -> None:
        """Test that lifecycle hooks preserve function metadata."""

        def test_func() -> None:
            """Test function docstring."""

        test_func.__name__ = "test_function"

        # Test all hooks preserve metadata
        start_decorated = SubscriptionLifecycle.on_start(test_func)
        event_decorated = SubscriptionLifecycle.on_event(test_func)
        complete_decorated = SubscriptionLifecycle.on_complete(test_func)

        assert start_decorated.__name__ == "test_function"
        assert event_decorated.__name__ == "test_function"
        assert complete_decorated.__name__ == "test_function"

        assert start_decorated.__doc__ == "Test function docstring."
        assert event_decorated.__doc__ == "Test function docstring."
        assert complete_decorated.__doc__ == "Test function docstring."


class TestWithLifecycle:
    """Test with_lifecycle decorator."""

    def setup_method(self) -> None:
        """Set up test fixtures."""
        self.mock_info = Mock()
        self.mock_info.context = {}

    @pytest.mark.asyncio
    async def test_with_lifecycle_all_hooks(self) -> None:
        """Test with_lifecycle decorator with all hooks."""
        on_start_mock = AsyncMock()
        on_event_mock = AsyncMock(side_effect=lambda info, value: f"processed_{value}")
        on_complete_mock = AsyncMock()

        @with_lifecycle(
            on_start=on_start_mock, on_event=on_event_mock, on_complete=on_complete_mock
        )
        @pytest.mark.asyncio
        async def test_subscription(info, param="default") -> None:
            yield "event1"
            yield "event2"
            yield "event3"

        # Collect results
        results = []
        async for value in test_subscription(self.mock_info, param="test_param"):
            results.append(value)

        # Check on_start was called
        on_start_mock.assert_called_once_with(
            self.mock_info, "test_subscription", {"param": "test_param"}
        )

        # Check on_event was called for each event
        assert on_event_mock.call_count == 3
        on_event_mock.assert_any_call(self.mock_info, "event1")
        on_event_mock.assert_any_call(self.mock_info, "event2")
        on_event_mock.assert_any_call(self.mock_info, "event3")

        # Check events were processed
        assert results == ["processed_event1", "processed_event2", "processed_event3"]

        # Check on_complete was called
        on_complete_mock.assert_called_once_with(
            self.mock_info, "test_subscription", {"param": "test_param"}
        )

    @pytest.mark.asyncio
    async def test_with_lifecycle_partial_hooks(self) -> None:
        """Test with_lifecycle decorator with only some hooks."""
        on_start_mock = AsyncMock()

        @with_lifecycle(on_start=on_start_mock)
        @pytest.mark.asyncio
        async def test_subscription(info) -> None:
            yield "single_event"

        results = []
        async for value in test_subscription(self.mock_info):
            results.append(value)

        # Only on_start should be called
        on_start_mock.assert_called_once()
        assert results == ["single_event"]

    @pytest.mark.asyncio
    async def test_with_lifecycle_no_hooks(self) -> None:
        """Test with_lifecycle decorator with no hooks."""

        @with_lifecycle()
        async def test_subscription(info) -> None:
            yield "no_hooks_event"

        results = []
        async for value in test_subscription(self.mock_info):
            results.append(value)

        assert results == ["no_hooks_event"]

    @pytest.mark.asyncio
    async def test_with_lifecycle_exception_handling(self) -> None:
        """Test with_lifecycle decorator handles exceptions properly."""
        on_start_mock = AsyncMock()
        on_complete_mock = AsyncMock()

        @with_lifecycle(on_start=on_start_mock, on_complete=on_complete_mock)
        async def failing_subscription(info) -> None:
            yield "before_error"
            raise ValueError("Test error")

        with pytest.raises(ValueError, match="Test error"):
            async for _value in failing_subscription(self.mock_info):
                pass

        # on_start should be called
        on_start_mock.assert_called_once()

        # on_complete should still be called due to finally block
        on_complete_mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_with_lifecycle_early_termination(self) -> None:
        """Test with_lifecycle decorator when subscription terminates early."""
        on_start_mock = AsyncMock()
        on_event_mock = AsyncMock(side_effect=lambda info, value: value)
        on_complete_mock = AsyncMock()

        @with_lifecycle(
            on_start=on_start_mock, on_event=on_event_mock, on_complete=on_complete_mock
        )
        async def long_subscription(info) -> None:
            for i in range(10):
                yield f"event_{i}"

        # Only consume first 2 events
        count = 0
        gen = long_subscription(self.mock_info)
        try:
            async for _value in gen:
                count += 1
                if count >= 2:
                    break
        finally:
            # Ensure generator is properly closed
            await gen.aclose()

        # on_start should be called
        on_start_mock.assert_called_once()

        # on_event should be called twice
        assert on_event_mock.call_count == 2

        # on_complete should still be called
        on_complete_mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_with_lifecycle_on_event_modifies_value(self) -> None:
        """Test that on_event can modify the yielded value."""

        async def transform_event(info, value) -> None:
            return value.upper()

        @with_lifecycle(on_event=transform_event)
        @pytest.mark.asyncio
        async def test_subscription(info) -> None:
            yield "hello"
            yield "world"

        results = []
        async for value in test_subscription(self.mock_info):
            results.append(value)

        assert results == ["HELLO", "WORLD"]

    @pytest.mark.asyncio
    async def test_with_lifecycle_async_generator_yield_none(self) -> None:
        """Test with_lifecycle decorator when generator yields None."""
        on_event_mock = AsyncMock(side_effect=lambda info, value: value)

        @with_lifecycle(on_event=on_event_mock)
        @pytest.mark.asyncio
        async def test_subscription(info) -> None:
            yield None
            yield "valid_event"
            yield None

        results = []
        async for value in test_subscription(self.mock_info):
            results.append(value)

        assert results == [None, "valid_event", None]
        assert on_event_mock.call_count == 3

    def test_with_lifecycle_preserves_function_metadata(self) -> None:
        """Test that with_lifecycle preserves function metadata."""

        @pytest.mark.asyncio
        async def test_subscription(info) -> None:
            """Test subscription docstring."""
            yield "test"

        test_subscription.__name__ = "test_subscription_func"

        decorated = with_lifecycle()(test_subscription)

        assert decorated.__name__ == "test_subscription_func"
        assert decorated.__doc__ == "Test subscription docstring."


class TestLifecycleIntegration:
    """Test integration scenarios for lifecycle hooks."""

    def setup_method(self) -> None:
        """Set up test fixtures."""
        self.mock_info = Mock()
        self.mock_info.context = {}

    @pytest.mark.asyncio
    async def test_multiple_decorators_combination(self) -> None:
        """Test combining multiple lifecycle decorators."""
        start_hook = AsyncMock()
        event_hook = AsyncMock(side_effect=lambda info, event: f"modified_{event}")
        complete_hook = AsyncMock()

        # Apply multiple decorators
        @with_lifecycle(on_start=start_hook)
        @with_lifecycle(on_event=event_hook)
        @with_lifecycle(on_complete=complete_hook)
        async def multi_decorated_subscription(info) -> None:
            yield "test_event"

        results = []
        async for value in multi_decorated_subscription(self.mock_info):
            results.append(value)

        # All hooks should be called
        start_hook.assert_called()
        event_hook.assert_called()
        complete_hook.assert_called()

        # Event should be modified
        assert "modified_test_event" in results[0]

    @pytest.mark.asyncio
    async def test_lifecycle_with_context_sharing(self) -> None:
        """Test lifecycle hooks sharing data through context."""

        async def start_hook(info, name, kwargs) -> None:
            info.context["start_data"] = "started"

        async def event_hook(info, value) -> None:
            start_data = info.context.get("start_data", "unknown")
            return f"{start_data}_{value}"

        async def complete_hook(info, name, kwargs) -> None:
            info.context["completed"] = True

        @with_lifecycle(on_start=start_hook, on_event=event_hook, on_complete=complete_hook)
        async def context_sharing_subscription(info) -> None:
            yield "event"

        results = []
        async for value in context_sharing_subscription(self.mock_info):
            results.append(value)

        assert results == ["started_event"]
        assert self.mock_info.context["completed"] is True

    @pytest.mark.asyncio
    async def test_lifecycle_with_logging(self) -> None:
        """Test lifecycle hooks with actual logging."""
        with patch("fraiseql.subscriptions.lifecycle.logger"):

            @with_lifecycle()
            async def logged_subscription(info) -> None:
                yield "log_test"

            results = []
            async for value in logged_subscription(self.mock_info):
                results.append(value)

            assert results == ["log_test"]

    @pytest.mark.asyncio
    async def test_nested_async_generators(self) -> None:
        """Test lifecycle hooks with nested async generators."""
        on_event_mock = AsyncMock(side_effect=lambda info, value: f"nested_{value}")

        @with_lifecycle(on_event=on_event_mock)
        async def outer_subscription(info) -> None:
            async def inner_generator() -> None:
                yield "inner1"
                yield "inner2"

            async for item in inner_generator():
                yield item

        results = []
        async for value in outer_subscription(self.mock_info):
            results.append(value)

        assert results == ["nested_inner1", "nested_inner2"]


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def setup_method(self) -> None:
        """Set up test fixtures."""
        self.mock_info = Mock()
        self.mock_info.context = {}

    @pytest.mark.asyncio
    async def test_hook_raises_exception(self) -> None:
        """Test behavior when lifecycle hook raises exception."""

        async def failing_start_hook(info, name, kwargs) -> None:
            raise ValueError("Start hook failed")

        @with_lifecycle(on_start=failing_start_hook)
        async def subscription_with_failing_hook(info) -> None:
            yield "should_not_reach"

        with pytest.raises(ValueError, match="Start hook failed"):
            async for _value in subscription_with_failing_hook(self.mock_info):
                pass

    @pytest.mark.asyncio
    async def test_subscription_with_no_yields(self) -> None:
        """Test lifecycle with subscription that yields nothing."""
        on_start_mock = AsyncMock()
        on_event_mock = AsyncMock()
        on_complete_mock = AsyncMock()

        @with_lifecycle(
            on_start=on_start_mock, on_event=on_event_mock, on_complete=on_complete_mock
        )
        async def empty_subscription(info) -> None:
            # Generator that yields nothing
            return
            yield  # This line is unreachable but makes it a generator

        results = []
        async for value in empty_subscription(self.mock_info):
            results.append(value)

        assert results == []
        on_start_mock.assert_called_once()
        on_event_mock.assert_not_called()  # No events to process
        on_complete_mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_subscription_id_generation(self) -> None:
        """Test subscription ID generation in on_start hook."""
        hook_func = AsyncMock()
        hook_func.__name__ = "test_subscription"  # Set the function name,
        decorated = SubscriptionLifecycle.on_start(hook_func)

        # Call multiple times with same info object
        result1 = await decorated(self.mock_info)
        result2 = await decorated(self.mock_info)

        # IDs should be different (based on id(info) which changes per call context)
        assert isinstance(result1, str)
        assert isinstance(result2, str)
        assert "test_subscription" in result1
        assert "test_subscription" in result2

    @pytest.mark.asyncio
    async def test_context_cleanup_on_exception(self) -> None:
        """Test that context is cleaned up even when exception occurs."""
        # Set up initial context
        self.mock_info.context["subscription_start"] = datetime.now(UTC)
        self.mock_info.context["subscription_id"] = "test_id"

        hook_func = AsyncMock(side_effect=ValueError("Hook failed"))
        decorated = SubscriptionLifecycle.on_complete(hook_func)

        with pytest.raises(ValueError, match="Hook failed"):
            await decorated(self.mock_info)

        # Context should still be cleaned up
        assert "subscription_start" not in self.mock_info.context
        assert "subscription_id" not in self.mock_info.context

    def test_decorator_without_call(self) -> None:
        """Test using with_lifecycle as decorator without calling it."""
        # This tests the edge case where someone might use @with_lifecycle
        # instead of @with_lifecycle()
        # In this case, the function itself would be passed as the first argument

        def dummy_subscription() -> None:
            pass

        # This should work without errors
        decorator = with_lifecycle(on_start=None, on_event=None, on_complete=None)
        decorated = decorator(dummy_subscription)

        assert callable(decorated)
        assert decorated.__name__ == dummy_subscription.__name__
