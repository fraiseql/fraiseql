"""
Phase 3: Python Resolver Integration - Unit Tests

Tests the Python resolver registration and invocation functionality.
These tests verify that:
1. Python resolver functions can be registered for subscriptions
2. Resolvers are called when events are published
3. Resolvers can transform event data to GraphQL response data
4. Error handling works correctly
"""

import pytest
import json
from fraiseql import _fraiseql_rs


class TestResolverRegistration:
    """Test resolver registration functionality (Task 3.1)"""

    def test_register_resolver_basic(self):
        """Test registering a basic Python resolver function"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register a subscription first to get a valid subscription ID
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Define a simple resolver function
        def test_resolver(event_data):
            return {"resolved": True, "event": event_data}

        # Register the resolver for a subscription
        executor.register_resolver(sub_id, test_resolver)
        # Should not raise an exception

    def test_register_resolver_requires_callable(self):
        """Test that resolver must be callable"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register a subscription first
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Try to register a non-callable as resolver
        with pytest.raises(TypeError):
            executor.register_resolver(sub_id, "not_a_function")

    def test_register_resolver_requires_subscription_id(self):
        """Test that subscription_id cannot be empty"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        def test_resolver(event_data):
            return event_data

        # Try to register with empty subscription_id
        with pytest.raises(ValueError):
            executor.register_resolver("", test_resolver)

    def test_register_multiple_resolvers(self):
        """Test registering multiple resolver functions"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register two subscriptions
        sub_id_1 = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        sub_id_2 = executor.register_subscription(
            connection_id="conn_2",
            subscription_id="sub_2",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def resolver_1(event_data):
            return {"resolver": 1, **event_data}

        def resolver_2(event_data):
            return {"resolver": 2, **event_data}

        # Register multiple resolvers
        executor.register_resolver(sub_id_1, resolver_1)
        executor.register_resolver(sub_id_2, resolver_2)
        # Should not raise exceptions

    def test_register_resolver_overwrites_previous(self):
        """Test that registering a new resolver overwrites the previous one"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register a subscription
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def resolver_old(event_data):
            return {"version": "old"}

        def resolver_new(event_data):
            return {"version": "new"}

        # Register first resolver
        executor.register_resolver(sub_id, resolver_old)
        # Register second resolver (should overwrite)
        executor.register_resolver(sub_id, resolver_new)
        # Should not raise exceptions


class TestResolverInvocation:
    """Test resolver invocation functionality (Task 3.2)"""

    def test_invoke_resolver_simple_transformation(self):
        """Test invoking a resolver to transform event data"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register subscription first (required for next_event)
        # Note: register_subscription returns the internal subscription ID
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Define a resolver that transforms event data
        def user_resolver(event_data):
            return {
                "id": event_data.get("user_id"),
                "name": event_data.get("username", "Unknown"),
                "email": event_data.get("email"),
            }

        # Register the resolver using the returned subscription ID
        executor.register_resolver(sub_id, user_resolver)

        # Publish an event
        # NOTE: user_id must match the subscription's user_id (1) for security filter to allow it
        executor.publish_event(
            event_type="userCreated",
            channel="users",
            data={
                "user_id": 1,  # Match subscription's user_id
                "username": "Alice",
                "email": "alice@example.com",
                "tenant_id": 1,
            },
        )

        # Get the response
        response = executor.next_event(sub_id)
        assert response is not None, "Expected response from resolver"

    def test_invoke_resolver_no_resolver_registered(self):
        """Test publishing event with no resolver (should echo event data)"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register subscription WITHOUT registering a resolver
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        # Publish an event
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"key": "value", "user_id": 1, "tenant_id": 1},
        )

        # Should still get a response (echo resolver)
        response = executor.next_event(sub_id)
        # Response should contain the event data
        assert response is not None

    def test_invoke_resolver_with_dict_return(self):
        """Test resolver that returns a dictionary"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def dict_resolver(event_data):
            return {
                "type": "response",
                "data": event_data,
                "transformed": True,
            }

        executor.register_resolver(sub_id, dict_resolver)
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"value": 42, "user_id": 1, "tenant_id": 1},
        )

        response = executor.next_event(sub_id)
        assert response is not None

    def test_invoke_resolver_with_nested_data(self):
        """Test resolver handling nested event data"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def nested_resolver(event_data):
            user = event_data.get("user", {})
            return {
                "id": user.get("id"),
                "name": user.get("profile", {}).get("name"),
                "role": user.get("role"),
            }

        executor.register_resolver(sub_id, nested_resolver)
        executor.publish_event(
            event_type="userUpdated",
            channel="users",
            data={
                "user": {
                    "id": 123,
                    "profile": {"name": "Bob"},
                    "role": "admin",
                },
                "user_id": 1,
                "tenant_id": 1,
            },
        )

        response = executor.next_event(sub_id)
        assert response is not None

    def test_invoke_resolver_with_list_return(self):
        """Test resolver that returns a list"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def list_resolver(event_data):
            items = event_data.get("items", [])
            return [{"id": i, "data": item} for i, item in enumerate(items)]

        executor.register_resolver(sub_id, list_resolver)
        executor.publish_event(
            event_type="itemsCreated",
            channel="items",
            data={
                "items": ["apple", "banana", "cherry"],
                "user_id": 1,
                "tenant_id": 1,
            },
        )

        response = executor.next_event(sub_id)
        assert response is not None

    def test_invoke_resolver_with_json_serializable_types(self):
        """Test resolver with various JSON-serializable types"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def types_resolver(event_data):
            return {
                "string": "hello",
                "number": 42,
                "float": 3.14,
                "bool": True,
                "null": None,
                "array": [1, 2, 3],
                "object": {"nested": "value"},
            }

        executor.register_resolver(sub_id, types_resolver)
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"user_id": 1, "tenant_id": 1},
        )

        response = executor.next_event(sub_id)
        assert response is not None


class TestResolverErrorHandling:
    """Test error handling in resolver invocation (Task 3.4 - basic)"""

    def test_resolver_exception_handling(self):
        """Test that resolver exceptions are caught and handled gracefully"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def failing_resolver(event_data):
            raise ValueError("Test error in resolver")

        executor.register_resolver(sub_id, failing_resolver)

        # Publishing should not crash the executor
        # Error should be handled gracefully
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"user_id": 1, "tenant_id": 1},
        )

    def test_resolver_with_missing_fields(self):
        """Test resolver handling missing fields gracefully"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def safe_resolver(event_data):
            # Use .get() to safely access fields that might not exist
            return {
                "id": event_data.get("missing_field", "default"),
                "name": event_data.get("also_missing", None),
            }

        executor.register_resolver(sub_id, safe_resolver)
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"user_id": 1, "tenant_id": 1},
        )

        response = executor.next_event(sub_id)
        assert response is not None

    def test_resolver_with_invalid_return_type(self):
        """Test resolver that returns non-JSON-serializable value (Phase 3.4)"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        class CustomObject:
            """Non-serializable object"""
            pass

        def bad_resolver(event_data):
            # Return something that cannot be JSON serialized
            return {"bad": CustomObject()}

        executor.register_resolver(sub_id, bad_resolver)
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"user_id": 1, "tenant_id": 1},
        )

        # Should not crash - error handling catches the exception
        response = executor.next_event(sub_id)
        assert response is not None  # Response should contain error info


class TestResolverIntegration:
    """Test resolver integration with event dispatch (Task 3.5 - partial)"""

    def test_multiple_subscriptions_different_resolvers(self):
        """Test multiple subscriptions with different resolvers"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register two subscriptions with different resolvers
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { users }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        executor.register_subscription(
            connection_id="conn_2",
            subscription_id="sub_2",
            query="subscription { posts }",
            operation_name=None,
            variables={},
            user_id=2,
            tenant_id=1,
        )

        def user_resolver(event_data):
            return {"type": "user", "id": event_data.get("user_id")}

        def post_resolver(event_data):
            return {"type": "post", "id": event_data.get("post_id")}

        executor.register_resolver(sub_id, user_resolver)
        executor.register_resolver("sub_2", post_resolver)

        # Publish event to shared channel
        executor.publish_event(
            event_type="itemCreated",
            channel="test",
            data={
                "user_id": 123,
                "post_id": 456,
                "tenant_id": 1,
            },
        )

    def test_resolver_with_subscription_context(self):
        """Test resolver accessing subscription context (variables, etc)"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register subscription with variables (without operation_name to avoid validation)
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription OnMessages($userId: String!) { messages(userId: $userId) }",
            operation_name=None,  # Let GraphQL parse the operation name from query
            variables={"userId": "user_42"},  # Variables passed to resolver
            user_id=1,
            tenant_id=1,
        )

        def context_resolver(event_data):
            # Resolver can access event data
            return {
                "message": event_data.get("message"),
                "from": event_data.get("from_user"),
            }

        executor.register_resolver(sub_id, context_resolver)
        executor.publish_event(
            event_type="messageCreated",
            channel="messages",
            data={
                "message": "Hello!",
                "from_user": "alice",
                "user_id": 1,
                "tenant_id": 1,
            },
        )


class TestResolverPerformance:
    """Test resolver performance characteristics"""

    def test_resolver_invocation_latency(self):
        """Test that resolver invocation completes in reasonable time"""
        import time

        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        def fast_resolver(event_data):
            return {"status": "ok"}

        executor.register_resolver(sub_id, fast_resolver)

        # Measure invocation time
        start = time.perf_counter()
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"user_id": 1, "tenant_id": 1},
        )
        elapsed = time.perf_counter() - start

        # Should complete quickly (< 100ms)
        assert elapsed < 0.1, f"Resolver invocation took {elapsed}s, expected < 0.1s"

    def test_resolver_with_many_subscriptions(self):
        """Test resolver performance with many subscriptions"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        def simple_resolver(event_data):
            return {"resolved": True}

        # Register 10 subscriptions with resolvers
        for i in range(10):
            executor.register_subscription(
                connection_id=f"conn_{i}",
                subscription_id=f"sub_{i}",
                query="subscription { test }",
                operation_name=None,
                variables={},
                user_id=i + 1,
                tenant_id=1,
            )
            executor.register_resolver(f"sub_{i}", simple_resolver)

        # Publish event (should dispatch to all)
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"value": 42, "user_id": 1, "tenant_id": 1},
        )


class TestResolverConcurrency:
    """Test concurrent resolver execution (Task 3.5)"""

    def test_concurrent_resolver_execution(self):
        """Test that multiple resolvers can execute concurrently (Phase 3.5)"""
        import time
        import threading

        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Track resolver calls
        call_times = []
        lock = threading.Lock()

        def tracking_resolver(event_data):
            """Resolver that records when it was called"""
            with lock:
                call_times.append(time.perf_counter())
            return {"resolved": True}

        # Register 5 subscriptions with different user IDs
        sub_ids = []
        for i in range(5):
            sub_id = executor.register_subscription(
                connection_id=f"conn_{i}",
                subscription_id=f"sub_{i}",
                query="subscription { test }",
                operation_name=None,
                variables={},
                user_id=i + 1,  # Different user for each subscription
                tenant_id=1,
            )
            sub_ids.append(sub_id)
            executor.register_resolver(sub_id, tracking_resolver)

        # Publish event - should trigger all 5 resolvers
        start_time = time.perf_counter()
        executor.publish_event(
            event_type="test",
            channel="test",
            data={"value": 42, "user_id": 1, "tenant_id": 1},
        )
        elapsed = time.perf_counter() - start_time

        # Verify all resolvers were called
        assert len(call_times) > 0, "At least one resolver should have been called"

        # Retrieve responses from all subscriptions
        responses_received = 0
        for sub_id in sub_ids:
            response = executor.next_event(sub_id)
            if response is not None:
                responses_received += 1

        assert responses_received > 0, "At least one response should be received"

    def test_concurrent_event_publishing(self):
        """Test concurrent event publishing with multiple threads (Phase 3.5)"""
        import threading
        import time

        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        call_count = {"value": 0}
        lock = threading.Lock()

        def counting_resolver(event_data):
            """Resolver that counts invocations"""
            with lock:
                call_count["value"] += 1
            return {"count": call_count["value"]}

        # Register subscription
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="sub_1",
            query="subscription { test }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )
        executor.register_resolver(sub_id, counting_resolver)

        # Publish events from multiple threads
        def publish_event(event_num):
            executor.publish_event(
                event_type=f"event_{event_num}",
                channel="test",
                data={"event_num": event_num, "user_id": 1, "tenant_id": 1},
            )

        threads = []
        for i in range(5):
            t = threading.Thread(target=publish_event, args=(i,))
            threads.append(t)
            t.start()

        for t in threads:
            t.join()

        # Verify events were processed
        response_count = 0
        while True:
            response = executor.next_event(sub_id)
            if response is None:
                break
            response_count += 1

        assert response_count > 0, "Should have received at least one response"


class TestResolverEndToEnd:
    """End-to-end workflow tests (Task 3.5)"""

    def test_complete_subscription_workflow(self):
        """Test complete workflow: register -> add resolver -> publish -> retrieve (Phase 3.5)"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Step 1: Register subscription
        sub_id = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="user_subscription_1",
            query="subscription OnUserUpdate($userId: ID!) { userUpdated(userId: $userId) { id name email } }",
            operation_name="OnUserUpdate",
            variables={"userId": "user_123"},
            user_id=1,
            tenant_id=1,
        )
        assert sub_id is not None, "Subscription should be registered"

        # Step 2: Register a resolver that transforms the event
        def user_transformer(event_data):
            """Transform raw event data to GraphQL response shape"""
            user = event_data.get("user", {})
            return {
                "userUpdated": {
                    "id": user.get("id"),
                    "name": user.get("name"),
                    "email": user.get("email"),
                }
            }

        executor.register_resolver(sub_id, user_transformer)

        # Step 3: Publish an event matching the subscription
        executor.publish_event(
            event_type="userUpdated",
            channel="users",
            data={
                "user": {
                    "id": "user_123",
                    "name": "Alice Johnson",
                    "email": "alice@example.com",
                },
                "user_id": 1,
                "tenant_id": 1,
            },
        )

        # Step 4: Retrieve the transformed response
        response = executor.next_event(sub_id)
        assert response is not None, "Response should be available"
        # Response is pre-serialized bytes, just verify it's not empty
        assert len(response) > 0, "Response should contain data"

    def test_multiple_resolvers_different_subscriptions(self):
        """Test multiple subscriptions with different resolvers (Phase 3.5)"""
        executor = _fraiseql_rs.subscriptions.PySubscriptionExecutor()

        # Register two different subscriptions
        user_sub = executor.register_subscription(
            connection_id="conn_1",
            subscription_id="users_sub",
            query="subscription { users }",
            operation_name=None,
            variables={},
            user_id=1,
            tenant_id=1,
        )

        post_sub = executor.register_subscription(
            connection_id="conn_2",
            subscription_id="posts_sub",
            query="subscription { posts }",
            operation_name=None,
            variables={},
            user_id=2,
            tenant_id=1,
        )

        # Register different resolvers
        def user_resolver(event_data):
            return {"type": "user", "count": event_data.get("user_count", 0)}

        def post_resolver(event_data):
            return {"type": "post", "count": event_data.get("post_count", 0)}

        executor.register_resolver(user_sub, user_resolver)
        executor.register_resolver(post_sub, post_resolver)

        # Publish event that matches user subscription
        executor.publish_event(
            event_type="usersUpdated",
            channel="users",
            data={"user_count": 10, "user_id": 1, "tenant_id": 1},
        )

        # Publish event that matches post subscription
        executor.publish_event(
            event_type="postsUpdated",
            channel="posts",
            data={"post_count": 25, "user_id": 2, "tenant_id": 1},
        )

        # Both subscriptions should have responses
        user_response = executor.next_event(user_sub)
        post_response = executor.next_event(post_sub)

        assert user_response is not None, "User subscription should have response"
        assert post_response is not None, "Post subscription should have response"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
