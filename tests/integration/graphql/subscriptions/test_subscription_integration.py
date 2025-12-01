"""Integration tests for GraphQL subscriptions."""

import asyncio
from collections.abc import AsyncGenerator
from uuid import UUID, uuid4

import pytest
from graphql import parse, subscribe

import fraiseql
from fraiseql import subscription
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.subscriptions import cache, complexity
from fraiseql.subscriptions import filter as sub_filter

pytestmark = pytest.mark.integration


# Define test types
@fraiseql.type
class Message:
    id: UUID
    text: str
    channel: str
    timestamp: float


@fraiseql.type
class Channel:
    id: UUID
    name: str
    active_users: int


# Define a simple query to satisfy GraphQL requirement
@fraiseql.query
async def ping(info) -> str:
    """Health check query."""
    return "pong"


# Define subscriptions
@subscription
@complexity(score=5)
@sub_filter("channel in info.context.get('allowed_channels', [])")
async def message_stream(info, channel: str) -> AsyncGenerator[Message]:
    """Subscribe to messages in a channel."""
    # Simulate real-time messages
    for i in range(3):
        await asyncio.sleep(0.1)
        yield Message(
            id=uuid4(),
            text=f"Message {i} in {channel}",
            channel=channel,
            timestamp=asyncio.get_event_loop().time(),
        )


@subscription
@cache(ttl=5.0)
async def channel_stats(info, channelId: UUID) -> AsyncGenerator[Channel]:
    """Subscribe to channel statistics."""
    # Simulate periodic updates
    for i in range(2):
        await asyncio.sleep(0.1)
        yield Channel(id=channelId, name=f"Channel {channelId}", active_users=10 + i)


@pytest.mark.asyncio
class TestSubscriptionIntegration:
    """Test subscription integration with GraphQL."""

    @pytest.fixture(autouse=True)
    def setup(self) -> None:
        """Set up test environment."""
        registry = SchemaRegistry.get_instance()
        registry.clear()
        # Re-register query and subscriptions after clearing
        registry.register_query(ping)
        registry.register_subscription(message_stream)
        registry.register_subscription(channel_stats)
        yield
        registry.clear()

    @pytest.mark.asyncio
    async def test_subscription_execution(self) -> None:
        """Test that subscriptions execute correctly."""
        # Build schema
        registry = SchemaRegistry.get_instance()
        schema = registry.build_schema()

        # Test message stream subscription
        subscription_query = """
            subscription MessageStream($channel: String!) {
                messageStream(channel: $channel) {
                    id
                    text
                    channel
                    timestamp
                }
            }
        """
        context = {"allowed_channels": ["general", "random"]}

        # Subscribe and collect results
        results = []
        subscription_result = await subscribe(
            schema,
            parse(subscription_query),
            variable_values={"channel": "general"},
            context_value=context,
        )

        # Check if it's an ExecutionResult (error) or AsyncIterator (success)
        if hasattr(subscription_result, "errors"):
            # It's an ExecutionResult with errors
            msg = f"Subscription failed: {subscription_result.errors}"
            raise AssertionError(msg)

        # It's an AsyncIterator, we can iterate over it
        async for result in subscription_result:
            if not result.errors and result.data:
                results.append(result.data["messageStream"])

        assert len(results) == 3
        assert all(r["channel"] == "general" for r in results)
        assert all("Message" in r["text"] for r in results)

    @pytest.mark.asyncio
    async def test_subscription_filtering(self) -> None:
        """Test that subscription filtering works."""
        # Build schema
        registry = SchemaRegistry.get_instance()
        schema = registry.build_schema()

        subscription_query = """
            subscription {
                messageStream(channel: "private") {
                    id
                    text
                }
            }
        """
        # Context without "private" in allowed channels
        context = {"allowed_channels": ["general"]}

        # Should get permission error
        subscription_result = await subscribe(
            schema, parse(subscription_query), context_value=context
        )

        # Check if it's an ExecutionResult with errors
        if hasattr(subscription_result, "errors") and subscription_result.errors:
            # Check for permission error in the errors
            error_messages = [str(e) for e in subscription_result.errors]
            if not any("Filter condition not met" in msg for msg in error_messages):
                msg = f"Expected 'Filter condition not met' in errors, got: {error_messages}"
                raise AssertionError(msg)
        else:
            # It's an AsyncIterator, try to get the first result which should fail
            try:
                await subscription_result.__anext__()
                msg = "Expected PermissionError but subscription succeeded"
                raise AssertionError(msg)
            except PermissionError as e:
                # Verify the error message contains expected text
                if "Filter condition not met" not in str(e):
                    msg = f"Expected 'Filter condition not met' in error, got: {e}"
                    raise AssertionError(msg) from e

    @pytest.mark.asyncio
    async def test_multiple_subscriptions(self) -> None:
        """Test multiple concurrent subscriptions."""
        # Build schema
        registry = SchemaRegistry.get_instance()
        schema = registry.build_schema()

        # Subscribe to both message stream and channel stats
        message_sub = """
            subscription {
                messageStream(channel: "general") {
                    id
                    text
                }
            }
        """
        channel_sub = """
            subscription {
                channelStats(channelId: "123e4567-e89b-12d3-a456-426614174000") {
                    id
                    name
                    activeUsers
                }
            }
        """
        context = {"allowed_channels": ["general"]}

        # Run both subscriptions concurrently
        async def collect_messages() -> None:
            results = []
            subscription_result = await subscribe(schema, parse(message_sub), context_value=context)
            if hasattr(subscription_result, "errors"):
                msg = f"Message subscription failed: {subscription_result.errors}"
                raise AssertionError(msg)
            async for result in subscription_result:
                if not result.errors:
                    results.append(result.data["messageStream"])
            return results

        async def collect_stats() -> None:
            results = []
            subscription_result = await subscribe(schema, parse(channel_sub), context_value=context)
            if hasattr(subscription_result, "errors"):
                msg = f"Stats subscription failed: {subscription_result.errors}"
                raise AssertionError(msg)
            async for result in subscription_result:
                if not result.errors:
                    results.append(result.data["channelStats"])
            return results

        # Run concurrently
        messages, stats = await asyncio.gather(collect_messages(), collect_stats())

        assert len(messages) == 3
        assert len(stats) == 2
        assert stats[0]["activeUsers"] == 10
        assert stats[1]["activeUsers"] == 11

    @pytest.mark.asyncio
    async def test_subscription_error_handling(self) -> None:
        """Test subscription error handling."""

        @subscription
        async def failing_subscription(info) -> AsyncGenerator[str]:
            """A subscription that fails."""
            yield "first"
            msg = "Subscription error!"
            raise ValueError(msg)

        # Register the failing subscription
        registry = SchemaRegistry.get_instance()
        registry.register_subscription(failing_subscription)

        # Build schema
        schema = registry.build_schema()

        subscription_query = """
            subscription {
                failingSubscription
            }
        """
        # Collect results
        results = []
        got_error = False

        try:
            subscription_result = await subscribe(schema, parse(subscription_query))
            if hasattr(subscription_result, "errors"):
                msg = f"Subscription failed: {subscription_result.errors}"
                raise AssertionError(msg)

            async for result in subscription_result:
                if result.data:
                    results.append(result.data["failingSubscription"])
        except ValueError as e:
            got_error = True
            # Verify the error message contains expected text
            if "Subscription error!" not in str(e):
                msg = f"Expected 'Subscription error!' in error, got: {e}"
                raise AssertionError(msg) from e

        # Should get first result then error
        assert len(results) == 1
        assert results[0] == "first"
        assert got_error
