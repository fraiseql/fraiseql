"""Integration tests for NATS event bus with deployment providers.

Tests end-to-end event publishing, subscription, and handling workflows
with deployment lifecycle events.
"""

import asyncio
import pytest
from unittest.mock import Mock, AsyncMock, patch, MagicMock

from fraisier.nats.events import NatsEvent, DeploymentEvents, HealthCheckEvents
from fraisier.nats.subscribers import (
    EventSubscriberRegistry,
    EventFilter,
    EventHandlers,
)
from fraisier.providers.base import HealthCheck, HealthCheckType
from fraisier.nats.provider import NatsEventProvider


class MockEventBus:
    """Mock NATS event bus for testing."""

    def __init__(self):
        """Initialize mock event bus."""
        self.published_events = []
        self.subscribers = []

    async def publish_deployment_event(self, event_type, deployment_id, data, trace_id=None, region=None):
        """Mock publish deployment event."""
        event = NatsEvent(
            event_type=event_type,
            data=data,
            region=region,
            trace_id=trace_id,
        )
        self.published_events.append(event)

    async def publish_health_check_event(self, event_type, service_name, data, trace_id=None):
        """Mock publish health check event."""
        event = NatsEvent(
            event_type=event_type,
            data=data,
            trace_id=trace_id,
        )
        self.published_events.append(event)


class MockProvider(NatsEventProvider):
    """Mock deployment provider with NATS integration."""

    def __init__(self, event_bus=None, region=None):
        """Initialize mock provider."""
        self.event_bus = event_bus
        self.region = region


class TestNatsProviderIntegration:
    """Tests for NATS event provider mixin integration."""

    @pytest.mark.asyncio
    async def test_emit_deployment_started_event(self):
        """Provider emits deployment started event."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus, region="us-east-1")

        await provider.emit_deployment_started(
            deployment_id="deploy_123",
            service_name="api",
            version="2.0.0",
            strategy="rolling",
        )

        assert len(mock_bus.published_events) == 1
        event = mock_bus.published_events[0]
        assert event.event_type == DeploymentEvents.STARTED
        assert event.data["service"] == "api"
        assert event.data["version"] == "2.0.0"
        assert event.region == "us-east-1"

    @pytest.mark.asyncio
    async def test_emit_deployment_completed_event(self):
        """Provider emits deployment completed event."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus)

        await provider.emit_deployment_completed(
            deployment_id="deploy_456",
            service_name="worker",
            status="success",
            duration_seconds=120.5,
            version="1.5.0",
        )

        assert len(mock_bus.published_events) == 1
        event = mock_bus.published_events[0]
        assert event.event_type == DeploymentEvents.COMPLETED
        assert event.data["status"] == "success"
        assert event.data["duration_seconds"] == 120.5

    @pytest.mark.asyncio
    async def test_emit_health_check_events(self):
        """Provider emits health check lifecycle events."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus, region="default")

        # Started
        await provider.emit_health_check_started(
            service_name="api",
            check_type="http",
            endpoint="http://localhost:8000/health",
        )

        # Passed
        await provider.emit_health_check_passed(
            service_name="api",
            check_type="http",
            duration_ms=50,
        )

        assert len(mock_bus.published_events) == 2
        assert mock_bus.published_events[0].event_type == HealthCheckEvents.CHECK_STARTED
        assert mock_bus.published_events[1].event_type == HealthCheckEvents.CHECK_PASSED

    @pytest.mark.asyncio
    async def test_emit_health_check_failed_event(self):
        """Provider emits health check failed event."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus)

        await provider.emit_health_check_failed(
            service_name="database",
            check_type="tcp",
            reason="Connection timeout",
            duration_ms=5000,
        )

        assert len(mock_bus.published_events) == 1
        event = mock_bus.published_events[0]
        assert event.event_type == HealthCheckEvents.CHECK_FAILED
        assert event.data["reason"] == "Connection timeout"

    @pytest.mark.asyncio
    async def test_emit_deployment_rolled_back_event(self):
        """Provider emits deployment rolled back event."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus)

        await provider.emit_deployment_rolled_back(
            deployment_id="deploy_789",
            service_name="api",
            from_version="2.0.0",
            to_version="1.9.0",
            reason="High error rate detected",
            duration_seconds=45.0,
        )

        assert len(mock_bus.published_events) == 1
        event = mock_bus.published_events[0]
        assert event.event_type == DeploymentEvents.ROLLED_BACK
        assert event.data["from_version"] == "2.0.0"
        assert event.data["to_version"] == "1.9.0"

    @pytest.mark.asyncio
    async def test_emit_metrics_event(self):
        """Provider emits metrics event."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus)

        metrics = {
            "error_rate": 0.01,
            "latency_ms": 150,
            "cpu_usage": 45.2,
            "memory_usage": 256,
        }

        await provider.emit_metrics(
            deployment_id="deploy_123",
            metrics=metrics,
        )

        assert len(mock_bus.published_events) == 1
        event = mock_bus.published_events[0]
        assert event.data["metrics"] == metrics

    @pytest.mark.asyncio
    async def test_events_without_event_bus(self):
        """Provider handles missing event bus gracefully."""
        provider = MockProvider(event_bus=None)

        # Should not raise exceptions
        await provider.emit_deployment_started(
            deployment_id="deploy_123",
            service_name="api",
        )

        await provider.emit_health_check_passed(
            service_name="api",
            check_type="http",
            duration_ms=100,
        )


class TestSubscriberEventIntegration:
    """Tests for subscriber registry with events."""

    @pytest.mark.asyncio
    async def test_subscriber_receives_deployment_event(self):
        """Subscriber receives and processes deployment event."""
        registry = EventSubscriberRegistry()
        received_events = []

        def handler(event):
            received_events.append(event)

        registry.register(handler)

        event = NatsEvent(
            event_type=DeploymentEvents.STARTED,
            data={"service": "api", "version": "2.0.0"},
        )

        await registry.handle_event(event)

        assert len(received_events) == 1
        assert received_events[0].event_type == DeploymentEvents.STARTED

    @pytest.mark.asyncio
    async def test_multiple_subscribers_receive_same_event(self):
        """Multiple subscribers receive the same event."""
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
            event_type=HealthCheckEvents.CHECK_PASSED,
            data={"service": "api", "duration_ms": 100},
        )

        await registry.handle_event(event)

        assert len(calls1) == 1
        assert len(calls2) == 1
        assert calls1[0].event_type == HealthCheckEvents.CHECK_PASSED

    @pytest.mark.asyncio
    async def test_filtered_subscriber_receives_matching_event(self):
        """Filtered subscriber receives matching event."""
        registry = EventSubscriberRegistry()
        api_events = []
        worker_events = []

        def api_handler(event):
            api_events.append(event)

        def worker_handler(event):
            worker_events.append(event)

        registry.register(api_handler, EventFilter(service="api"))
        registry.register(worker_handler, EventFilter(service="worker"))

        api_event = NatsEvent(
            event_type=DeploymentEvents.STARTED,
            data={"service": "api"},
        )
        worker_event = NatsEvent(
            event_type=DeploymentEvents.STARTED,
            data={"service": "worker"},
        )

        await registry.handle_event(api_event)
        await registry.handle_event(worker_event)

        assert len(api_events) == 1
        assert len(worker_events) == 1

    @pytest.mark.asyncio
    async def test_pre_built_event_handlers(self):
        """Test pre-built event handlers work correctly."""
        registry = EventSubscriberRegistry()

        # Add multiple pre-built handlers
        registry.register(EventHandlers.log_event)
        registry.register(EventHandlers.count_events)

        event = NatsEvent(
            event_type=DeploymentEvents.COMPLETED,
            data={"service": "api", "status": "success"},
        )

        # Should not raise exceptions
        await registry.handle_event(event)

    @pytest.mark.asyncio
    async def test_async_and_sync_handlers_mixed(self):
        """Test registry handles mixed sync/async handlers."""
        registry = EventSubscriberRegistry()
        sync_calls = []
        async_calls = []

        def sync_handler(event):
            sync_calls.append(event)

        async def async_handler(event):
            async_calls.append(event)
            await asyncio.sleep(0.01)

        registry.register(sync_handler, is_async=False)
        registry.register(async_handler, is_async=True)

        event = NatsEvent(
            event_type=HealthCheckEvents.CHECK_FAILED,
            data={"service": "database", "reason": "Timeout"},
        )

        await registry.handle_event(event)

        assert len(sync_calls) == 1
        assert len(async_calls) == 1

    @pytest.mark.asyncio
    async def test_regional_event_filtering(self):
        """Test regional filtering of events."""
        registry = EventSubscriberRegistry()
        us_events = []
        eu_events = []

        def us_handler(event):
            us_events.append(event)

        def eu_handler(event):
            eu_events.append(event)

        registry.register(us_handler, EventFilter(region="us-east-1"))
        registry.register(eu_handler, EventFilter(region="eu-west-1"))

        us_event = NatsEvent(
            event_type=DeploymentEvents.STARTED,
            data={"service": "api"},
            region="us-east-1",
        )
        eu_event = NatsEvent(
            event_type=DeploymentEvents.STARTED,
            data={"service": "api"},
            region="eu-west-1",
        )

        await registry.handle_event(us_event)
        await registry.handle_event(eu_event)

        assert len(us_events) == 1
        assert len(eu_events) == 1


class TestEndToEndEventWorkflow:
    """End-to-end tests for complete event workflows."""

    @pytest.mark.asyncio
    async def test_deployment_lifecycle_event_flow(self):
        """Test complete deployment lifecycle event flow."""
        # Setup
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus, region="us-east-1")
        registry = EventSubscriberRegistry()

        lifecycle_events = []

        def lifecycle_tracker(event):
            lifecycle_events.append(event)

        # Register handler for all deployment events
        registry.register(lifecycle_tracker)

        # Emit deployment lifecycle events
        await provider.emit_deployment_started(
            deployment_id="deploy_1",
            service_name="api",
            version="2.0.0",
        )

        await provider.emit_health_check_started(
            service_name="api",
            check_type="http",
        )

        await provider.emit_health_check_passed(
            service_name="api",
            check_type="http",
            duration_ms=100,
        )

        await provider.emit_deployment_completed(
            deployment_id="deploy_1",
            service_name="api",
            status="success",
            duration_seconds=300.0,
        )

        # Process events through registry
        for event in mock_bus.published_events:
            await registry.handle_event(event)

        # Verify complete lifecycle
        assert len(lifecycle_events) == 4
        assert lifecycle_events[0].event_type == DeploymentEvents.STARTED
        assert lifecycle_events[1].event_type == HealthCheckEvents.CHECK_STARTED
        assert lifecycle_events[2].event_type == HealthCheckEvents.CHECK_PASSED
        assert lifecycle_events[3].event_type == DeploymentEvents.COMPLETED

    @pytest.mark.asyncio
    async def test_failure_recovery_event_flow(self):
        """Test failure detection and recovery event flow."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus)
        registry = EventSubscriberRegistry()

        failure_events = []
        recovery_events = []

        def failure_handler(event):
            failure_events.append(event)

        def recovery_handler(event):
            recovery_events.append(event)

        registry.register(
            failure_handler,
            EventFilter(event_type=HealthCheckEvents.CHECK_FAILED)
        )
        registry.register(
            recovery_handler,
            EventFilter(event_type=HealthCheckEvents.CHECK_PASSED)
        )

        # Health check fails
        await provider.emit_health_check_failed(
            service_name="api",
            check_type="tcp",
            reason="Connection refused",
            duration_ms=5000,
        )

        # After remediation, health check passes
        await provider.emit_health_check_passed(
            service_name="api",
            check_type="tcp",
            duration_ms=50,
        )

        # Process events
        for event in mock_bus.published_events:
            await registry.handle_event(event)

        assert len(failure_events) == 1
        assert failure_events[0].data["reason"] == "Connection refused"
        assert len(recovery_events) == 1

    @pytest.mark.asyncio
    async def test_multi_service_deployment_events(self):
        """Test events from multiple services in single deployment."""
        mock_bus = MockEventBus()
        provider = MockProvider(event_bus=mock_bus)
        registry = EventSubscriberRegistry()

        service_events = {}

        def service_tracker(event):
            service = event.data.get("service", "unknown")
            if service not in service_events:
                service_events[service] = []
            service_events[service].append(event)

        registry.register(service_tracker)

        # Deploy multiple services
        for service in ["api", "worker", "database"]:
            await provider.emit_deployment_started(
                deployment_id="deploy_multi_1",
                service_name=service,
            )

        # Process events
        for event in mock_bus.published_events:
            await registry.handle_event(event)

        assert len(service_events) == 3
        assert "api" in service_events
        assert "worker" in service_events
        assert "database" in service_events
