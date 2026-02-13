"""Tests for NATS event subscriber registry and handlers.

Tests event subscription, filtering, handler registration, and event processing.
"""

import asyncio
import pytest
from fraisier.nats.subscribers import (
    EventSubscriberRegistry,
    EventFilter,
    EventHandlers,
    get_subscriber_registry,
    reset_subscriber_registry,
)
from fraisier.nats.events import (
    NatsEvent,
    DeploymentEvents,
    HealthCheckEvents,
)


class TestEventFilter:
    """Tests for EventFilter matching."""

    def test_empty_filter_matches_all_events(self):
        """Empty filter should match any event."""
        filter = EventFilter()
        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api", "status": "running"},
        )
        assert filter.matches(event) is True

    def test_filter_by_event_type(self):
        """Filter by specific event type."""
        filter = EventFilter(event_type="deployment.started")

        event1 = NatsEvent(event_type="deployment.started", data={})
        event2 = NatsEvent(event_type="deployment.completed", data={})

        assert filter.matches(event1) is True
        assert filter.matches(event2) is False

    def test_filter_by_service(self):
        """Filter by service name."""
        filter = EventFilter(service="api")

        event1 = NatsEvent(event_type="health_check.passed", data={"service": "api"})
        event2 = NatsEvent(event_type="health_check.passed", data={"service": "worker"})

        assert filter.matches(event1) is True
        assert filter.matches(event2) is False

    def test_filter_by_region(self):
        """Filter by region."""
        filter = EventFilter(region="us-east-1")

        event1 = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
            region="us-east-1",
        )
        event2 = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
            region="us-west-2",
        )

        assert filter.matches(event1) is True
        assert filter.matches(event2) is False

    def test_filter_by_deployment_id(self):
        """Filter by deployment ID."""
        filter = EventFilter(deployment_id="deploy_123")

        event1 = NatsEvent(
            event_type="deployment.started",
            data={"service": "api", "deployment_id": "deploy_123"},
        )
        event2 = NatsEvent(
            event_type="deployment.started",
            data={"service": "api", "deployment_id": "deploy_456"},
        )

        assert filter.matches(event1) is True
        assert filter.matches(event2) is False

    def test_filter_by_tags(self):
        """Filter by custom tags."""
        filter = EventFilter(tags={"env": "production", "version": "2.0"})

        event1 = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "env": "production", "version": "2.0"},
        )
        event2 = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "env": "staging", "version": "2.0"},
        )
        event3 = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "env": "production", "version": "1.9"},
        )

        assert filter.matches(event1) is True
        assert filter.matches(event2) is False
        assert filter.matches(event3) is False

    def test_combined_filters(self):
        """Multiple filter conditions combined."""
        filter = EventFilter(
            event_type="deployment.started",
            service="api",
            region="us-east-1",
        )

        event1 = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
            region="us-east-1",
        )
        event2 = NatsEvent(
            event_type="deployment.started",
            data={"service": "worker"},
            region="us-east-1",
        )
        event3 = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api"},
            region="us-east-1",
        )

        assert filter.matches(event1) is True
        assert filter.matches(event2) is False
        assert filter.matches(event3) is False


class TestEventSubscriberRegistry:
    """Tests for EventSubscriberRegistry."""

    def test_register_sync_handler(self):
        """Register synchronous event handler."""
        registry = EventSubscriberRegistry()

        handler_called = []

        def handler(event):
            handler_called.append(event.event_type)

        sub_id = registry.register(handler, is_async=False)

        assert sub_id.startswith("sub_")
        assert registry.get_subscription_count() == 1

    def test_register_async_handler(self):
        """Register asynchronous event handler."""
        registry = EventSubscriberRegistry()

        async def handler(event):
            pass

        sub_id = registry.register(handler, is_async=True)

        assert sub_id.startswith("sub_")
        assert registry.get_subscription_count() == 1

    def test_register_multiple_handlers(self):
        """Register multiple handlers."""
        registry = EventSubscriberRegistry()

        def handler1(event):
            pass

        def handler2(event):
            pass

        async def handler3(event):
            pass

        sub_id1 = registry.register(handler1)
        sub_id2 = registry.register(handler2)
        sub_id3 = registry.register(handler3, is_async=True)

        assert registry.get_subscription_count() == 3
        assert sub_id1 != sub_id2 != sub_id3

    def test_unregister_handler(self):
        """Unregister event handler."""
        registry = EventSubscriberRegistry()

        def handler(event):
            pass

        sub_id = registry.register(handler)
        assert registry.get_subscription_count() == 1

        result = registry.unregister(sub_id)
        assert result is True
        assert registry.get_subscription_count() == 0

    def test_unregister_nonexistent_handler(self):
        """Unregister handler that doesn't exist."""
        registry = EventSubscriberRegistry()

        result = registry.unregister("nonexistent_id")
        assert result is False

    def test_register_deployment_handler(self):
        """Register deployment-specific handler."""
        registry = EventSubscriberRegistry()

        def handler(event):
            pass

        sub_id = registry.register_deployment_handler(
            handler,
            event_type="deployment.started",
            service="api",
        )

        assert registry.get_subscription_count() == 1

    def test_register_health_check_handler(self):
        """Register health check-specific handler."""
        registry = EventSubscriberRegistry()

        def handler(event):
            pass

        sub_id = registry.register_health_check_handler(
            handler,
            event_type="health_check.passed",
            service="api",
        )

        assert registry.get_subscription_count() == 1

    def test_get_subscriptions_for_event_type(self):
        """Get subscriptions for specific event type."""
        registry = EventSubscriberRegistry()

        def handler1(event):
            pass

        def handler2(event):
            pass

        registry.register(handler1, EventFilter(event_type="deployment.started"))
        registry.register(handler2, EventFilter(event_type="deployment.completed"))

        subs = registry.get_subscriptions_for_event_type("deployment.started")
        assert len(subs) == 1

    def test_clear_all_subscriptions(self):
        """Clear all registered subscriptions."""
        registry = EventSubscriberRegistry()

        def handler(event):
            pass

        registry.register(handler)
        registry.register(handler)
        registry.register(handler)

        assert registry.get_subscription_count() == 3

        count = registry.clear_all()
        assert count == 3
        assert registry.get_subscription_count() == 0

    @pytest.mark.asyncio
    async def test_handle_event_sync_handler(self):
        """Process event through sync handler."""
        registry = EventSubscriberRegistry()

        handled_events = []

        def handler(event):
            handled_events.append(event)

        registry.register(handler)

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        await registry.handle_event(event)

        assert len(handled_events) == 1
        assert handled_events[0].event_type == "deployment.started"

    @pytest.mark.asyncio
    async def test_handle_event_async_handler(self):
        """Process event through async handler."""
        registry = EventSubscriberRegistry()

        handled_events = []

        async def handler(event):
            handled_events.append(event)
            await asyncio.sleep(0.01)  # Simulate async work

        registry.register(handler, is_async=True)

        event = NatsEvent(
            event_type="health_check.passed",
            data={"service": "api", "duration_ms": 50},
        )

        await registry.handle_event(event)

        assert len(handled_events) == 1
        assert handled_events[0].event_type == "health_check.passed"

    @pytest.mark.asyncio
    async def test_handle_event_with_filter_match(self):
        """Event is handled when filter matches."""
        registry = EventSubscriberRegistry()

        handled_events = []

        def handler(event):
            handled_events.append(event)

        registry.register(
            handler,
            EventFilter(service="api"),
        )

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        await registry.handle_event(event)

        assert len(handled_events) == 1

    @pytest.mark.asyncio
    async def test_handle_event_with_filter_no_match(self):
        """Event is not handled when filter doesn't match."""
        registry = EventSubscriberRegistry()

        handled_events = []

        def handler(event):
            handled_events.append(event)

        registry.register(
            handler,
            EventFilter(service="worker"),
        )

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        await registry.handle_event(event)

        assert len(handled_events) == 0

    @pytest.mark.asyncio
    async def test_handle_event_multiple_matching_handlers(self):
        """Multiple handlers are called for same event."""
        registry = EventSubscriberRegistry()

        calls1 = []
        calls2 = []

        def handler1(event):
            calls1.append(event)

        def handler2(event):
            calls2.append(event)

        registry.register(handler1)
        registry.register(handler2)

        event = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "status": "success"},
        )

        await registry.handle_event(event)

        assert len(calls1) == 1
        assert len(calls2) == 1

    @pytest.mark.asyncio
    async def test_handle_event_with_handler_exception(self):
        """Handler exception is caught and logged."""
        registry = EventSubscriberRegistry()

        def failing_handler(event):
            raise ValueError("Handler error")

        registry.register(failing_handler, retry_on_failure=False)

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        # Should not raise exception
        await registry.handle_event(event)

    @pytest.mark.asyncio
    async def test_handle_event_with_retry(self):
        """Handler retry logic on failure."""
        registry = EventSubscriberRegistry()

        attempt_count = []

        def failing_handler(event):
            attempt_count.append(1)
            if len(attempt_count) < 3:
                raise ValueError("Try again")
            # Success on 3rd attempt

        registry.register(
            failing_handler,
            retry_on_failure=True,
            retry_count=3,
            retry_delay=0.01,
        )

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        await registry.handle_event(event)

        # Handler should have been called 3 times
        assert len(attempt_count) == 3

    @pytest.mark.asyncio
    async def test_handle_event_no_matching_handlers(self):
        """Event handling succeeds even with no matching handlers."""
        registry = EventSubscriberRegistry()

        def handler(event):
            pass

        # Register handler that won't match
        registry.register(
            handler,
            EventFilter(service="other_service"),
        )

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        # Should not raise exception
        await registry.handle_event(event)


class TestEventHandlers:
    """Tests for pre-built event handlers."""

    def test_log_event_handler(self):
        """Log event handler works."""
        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api", "status": "running"},
        )

        # Should not raise exception
        EventHandlers.log_event(event)

    @pytest.mark.asyncio
    async def test_log_event_async_handler(self):
        """Log event async handler works."""
        event = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "status": "success"},
        )

        # Should not raise exception
        await EventHandlers.log_event_async(event)

    def test_count_events_handler(self):
        """Count events handler works."""
        event = NatsEvent(
            event_type="health_check.passed",
            data={"service": "api"},
        )

        # Should not raise exception
        EventHandlers.count_events(event)

    @pytest.mark.asyncio
    async def test_notify_on_failure_handler(self):
        """Notify on failure handler works."""
        event1 = NatsEvent(
            event_type="deployment.failed",
            data={"service": "api", "error": "Connection timeout"},
        )
        event2 = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "status": "success"},
        )

        # Should not raise exception
        await EventHandlers.notify_on_failure(event1)
        await EventHandlers.notify_on_failure(event2)

    def test_trigger_webhook_handler(self):
        """Create webhook handler."""
        handler = EventHandlers.trigger_webhook("http://example.com/webhook")

        event = NatsEvent(
            event_type="deployment.started",
            data={"service": "api"},
        )

        # Should not raise exception
        handler(event)

    def test_create_metric_recorder_handler(self):
        """Create metric recorder handler."""
        handler = EventHandlers.create_metric_recorder("deployment.duration")

        event = NatsEvent(
            event_type="deployment.completed",
            data={"service": "api", "duration_ms": 5000},
        )

        # Should not raise exception
        handler(event)


class TestGlobalRegistry:
    """Tests for global subscriber registry."""

    def test_get_subscriber_registry(self):
        """Get global subscriber registry."""
        reset_subscriber_registry()
        registry = get_subscriber_registry()

        assert registry is not None
        assert isinstance(registry, EventSubscriberRegistry)

    def test_global_registry_is_singleton(self):
        """Global registry is singleton."""
        reset_subscriber_registry()
        registry1 = get_subscriber_registry()
        registry2 = get_subscriber_registry()

        assert registry1 is registry2

    def test_reset_subscriber_registry(self):
        """Reset global subscriber registry."""
        registry = get_subscriber_registry()

        def handler(event):
            pass

        registry.register(handler)
        assert registry.get_subscription_count() == 1

        reset_subscriber_registry()

        new_registry = get_subscriber_registry()
        assert new_registry.get_subscription_count() == 0
