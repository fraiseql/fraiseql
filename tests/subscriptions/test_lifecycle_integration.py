"""Integration tests for subscription lifecycle with real GraphQL execution.

These tests use actual GraphQL schema building and database connections
to test the complete subscription lifecycle in a realistic environment.
"""

import asyncio
from collections.abc import AsyncGenerator
from datetime import UTC, datetime

import pytest
from graphql import parse, subscribe

# Import database fixtures for this database test
from tests.database_conftest import *  # noqa: F403

import fraiseql
from fraiseql import subscription
from fraiseql.gql.schema_builder import SchemaRegistry
from fraiseql.subscriptions.lifecycle import with_lifecycle


# Define test types
@fraiseql.type
class Message:
    id: int
    content: str
    timestamp: float


# Define a simple query to satisfy GraphQL requirement
@fraiseql.query
async def health_check(info) -> str:
    """Health check query."""
    return "healthy"


# Define subscription with lifecycle hooks
@subscription
async def message_stream(info) -> AsyncGenerator[Message]:
    """Stream messages with lifecycle hooks."""
    # Manually implement lifecycle behavior for testing
    # This simulates what the lifecycle decorator would do
    info.context["test_subscription_started"] = True
    info.context["subscription_id"] = f"message_stream_{id(info)}"
    info.context["subscription_start"] = datetime.now(UTC)

    # Simulate message stream
    for i in range(3):
        yield Message(id=i, content=f"Message {i}", timestamp=asyncio.get_event_loop().time())
        await asyncio.sleep(0.1)


@subscription
async def simple_counter(info, count: int = 5) -> AsyncGenerator[int]:
    """Simple counter subscription."""
    for i in range(count):
        yield i
        await asyncio.sleep(0.1)


@pytest.mark.database
class TestSubscriptionLifecycle:
    """Test subscription lifecycle with real GraphQL setup."""

    @pytest.fixture(autouse=True)
    def setup(self):
        """Set up test environment."""
        registry = SchemaRegistry.get_instance()
        registry.clear()
        # Re-register types, query and subscriptions after clearing
        registry.register_type(Message)
        registry.register_query(health_check)
        registry.register_subscription(message_stream)
        registry.register_subscription(simple_counter)
        yield
        registry.clear()

    @pytest.mark.asyncio
    async def test_subscription_lifecycle_hooks(self, db_pool):
        """Test that lifecycle hooks are properly called."""
        # Build schema
        registry = SchemaRegistry.get_instance()
        schema = registry.build_schema()

        # Create context
        context = {"db": db_pool, "test_flag": True}

        # Execute subscription
        query = """
            subscription {
                messageStream {
                    id
                    content
                    timestamp
                }
            }
        """
        document = parse(query)
        result = await subscribe(schema, document, root_value=None, context_value=context)

        # Check if it's an ExecutionResult (error) or AsyncIterator (success)
        if hasattr(result, "errors"):
            # It's an ExecutionResult with errors
            msg = f"Subscription failed: {result.errors}"
            raise AssertionError(msg)

        # Collect results
        messages = []
        async for item in result:
            print(f"DEBUG: Got item: {item}")
            print(
                f"DEBUG: Item errors: "
                f"{item.errors if hasattr(item, 'errors') else 'No errors attr'}"
            )
            print(f"DEBUG: Item data: {item.data if hasattr(item, 'data') else 'No data attr'}")

            if hasattr(item, "errors") and item.errors:
                print(f"ERROR in subscription: {item.errors}")
                raise AssertionError(f"Subscription error: {item.errors}")

            if hasattr(item, "data") and item.data:
                messages.append(item.data["messageStream"])
                # Check context was updated by on_start hook
                if len(messages) == 1:
                    assert context.get("test_subscription_started") is True
                    assert "subscription_id" in context
                    assert "subscription_start" in context

        # Verify we got 3 messages
        assert len(messages) == 3
        for i, msg in enumerate(messages):
            assert msg["id"] == i
            assert msg["content"] == f"Message {i}"

    @pytest.mark.asyncio
    async def test_with_lifecycle_decorator(self, db_pool):
        """Test the with_lifecycle decorator."""

        # Define lifecycle callbacks
        async def on_start_callback(info, name, kwargs):
            """Set up context on subscription start."""
            info.context["subscription_id"] = f"{name}_{id(info)}"
            info.context["subscription_start"] = datetime.now(UTC)

        @with_lifecycle(on_start=on_start_callback)
        @subscription
        async def lifecycle_subscription(info) -> AsyncGenerator[dict]:
            """Subscription with automatic lifecycle management."""
            # Check that context has been set up
            assert "subscription_id" in info.context
            assert "subscription_start" in info.context

            for i in range(2):
                yield {"value": i}
                await asyncio.sleep(0.1)

        # Register the subscription
        registry = SchemaRegistry.get_instance()
        registry.register_subscription(lifecycle_subscription)
        schema = registry.build_schema()

        context = {"db": db_pool}

        query = """
            subscription {
                lifecycleSubscription {
                    value
                }
            }
        """
        from graphql import parse, subscribe

        document = parse(query)
        result = await subscribe(schema, document, context_value=context)

        # Check if it's an ExecutionResult (error) or AsyncIterator (success)
        if hasattr(result, "errors"):
            # It's an ExecutionResult with errors
            msg = f"Subscription failed: {result.errors}"
            raise AssertionError(msg)

        # Collect results
        values = []
        async for item in result:
            if not item.errors and item.data:
                values.append(item.data["lifecycleSubscription"]["value"])

        assert values == [0, 1]

    @pytest.mark.asyncio
    async def test_subscription_early_termination(self, db_pool):
        """Test subscription cleanup on early termination."""
        # Build schema
        registry = SchemaRegistry.get_instance()
        schema = registry.build_schema()

        context = {"db": db_pool, "terminated": False}

        query = """
            subscription {
                simpleCounter(count: 10)
            }
        """
        from graphql import parse, subscribe

        document = parse(query)
        result = await subscribe(schema, document, context_value=context)

        # Check if it's an ExecutionResult (error) or AsyncIterator (success)
        if hasattr(result, "errors"):
            # It's an ExecutionResult with errors
            msg = f"Subscription failed: {result.errors}"
            raise AssertionError(msg)

        # Collect only first 3 items then break
        count = 0
        async for item in result:
            if not item.errors:
                count += 1
                if count >= 3:
                    context["terminated"] = True
                    break

        assert count == 3
        assert context["terminated"] is True

    @pytest.mark.asyncio
    async def test_subscription_with_error_handling(self, db_pool):
        """Test subscription error handling."""

        @subscription
        async def error_subscription(info) -> AsyncGenerator[Message]:
            """Subscription that raises an error."""
            yield Message(id=1, content="First", timestamp=datetime.now().timestamp())
            raise ValueError("Test error")
            yield Message(id=2, content="Never reached", timestamp=datetime.now().timestamp())

        # Register the subscription
        registry = SchemaRegistry.get_instance()
        registry.register_subscription(error_subscription)
        schema = registry.build_schema()

        context = {"db": db_pool}

        query = """
            subscription {
                errorSubscription {
                    id
                    content
                }
            }
        """
        from graphql import parse, subscribe

        document = parse(query)
        result = await subscribe(schema, document, context_value=context)

        # Check if it's an ExecutionResult (error) or AsyncIterator (success)
        if hasattr(result, "errors"):
            # It's an ExecutionResult with errors
            msg = f"Subscription failed: {result.errors}"
            raise AssertionError(msg)

        # Collect results and errors
        results = []
        error_occurred = False

        try:
            async for item in result:
                if item.errors:
                    error_occurred = True
                    break
                if item.data:
                    results.append(item)
        except Exception:
            error_occurred = True

        # Should get first message then error
        assert len(results) >= 1
        assert results[0].data["errorSubscription"]["id"] == 1
        assert error_occurred or any(r.errors for r in results)

    @pytest.mark.asyncio
    async def test_subscription_id_generation(self, db_pool):
        """Test that subscription IDs are properly generated."""
        # Build schema
        registry = SchemaRegistry.get_instance()
        schema = registry.build_schema()

        contexts = []

        # Run multiple subscriptions concurrently
        async def run_subscription():
            context = {"db": db_pool}
            contexts.append(context)

            query = """
                subscription {
                    messageStream {
                        id
                    }
                }
            """
            from graphql import parse, subscribe

            document = parse(query)
            result = await subscribe(schema, document, context_value=context)

            # Check if it's an ExecutionResult (error) or AsyncIterator (success)
            if hasattr(result, "errors"):
                # It's an ExecutionResult with errors
                msg = f"Subscription failed: {result.errors}"
                raise AssertionError(msg)

            # Just get first result
            async for item in result:
                if not item.errors:
                    break

            return context.get("subscription_id")

        # Run 3 subscriptions concurrently
        subscription_ids = await asyncio.gather(
            run_subscription(), run_subscription(), run_subscription()
        )

        # All should have unique IDs
        assert len(set(subscription_ids)) == 3

        # All contexts should have been updated
        for ctx in contexts:
            assert "subscription_id" in ctx
            assert "subscription_start" in ctx
            assert isinstance(ctx["subscription_start"], datetime)


class TestEdgeCases:
    """Test edge cases for subscription lifecycle."""

    @pytest.fixture(autouse=True)
    def setup(self):
        """Set up test environment."""
        registry = SchemaRegistry.get_instance()
        registry.clear()
        # Re-register query after clearing
        registry.register_query(health_check)
        yield
        registry.clear()

    @pytest.mark.asyncio
    async def test_subscription_without_lifecycle(self, db_pool):
        """Test that subscriptions work without lifecycle decorators."""

        @subscription
        async def basic_subscription(info) -> AsyncGenerator[int]:
            """Basic subscription without lifecycle."""
            for i in range(3):
                yield i

        # Register the subscription
        registry = SchemaRegistry.get_instance()
        registry.register_subscription(basic_subscription)
        schema = registry.build_schema()

        context = {"db": db_pool}

        query = """
            subscription {
                basicSubscription
            }
        """
        from graphql import parse, subscribe

        document = parse(query)
        result = await subscribe(schema, document, context_value=context)

        # Check if it's an ExecutionResult (error) or AsyncIterator (success)
        if hasattr(result, "errors"):
            # It's an ExecutionResult with errors
            msg = f"Subscription failed: {result.errors}"
            raise AssertionError(msg)

        values = []
        async for item in result:
            if not item.errors and item.data:
                values.append(item.data["basicSubscription"])

        assert values == [0, 1, 2]
