"""Tests for NATS client wrapper.

Tests connection, publish, subscribe, and request/reply functionality.
"""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch
from datetime import datetime, timezone

from fraisier.nats.client import NatsClient, NatsEventBus
from fraisier.nats.events import (
    NatsEvent,
    DeploymentEvents,
    HealthCheckEvents,
    DatabaseEvents,
)


class TestNatsClient:
    """Test NatsClient connection and basic operations."""

    @pytest.fixture
    def client(self):
        """Create a NatsClient instance."""
        return NatsClient(servers=["nats://localhost:4222"])

    @pytest.mark.asyncio
    async def test_client_initialization(self, client):
        """Test client initialization."""
        assert client.servers == ["nats://localhost:4222"]
        assert client.timeout == 5.0
        assert not client.connected

    @pytest.mark.asyncio
    async def test_connect_fails_without_server(self):
        """Test connection failure when server unavailable."""
        client = NatsClient(servers=["nats://invalid-host:9999"], timeout=1.0)

        with pytest.raises(Exception):  # NatsError
            await client.connect()

    @pytest.mark.asyncio
    async def test_servers_from_string(self):
        """Test server initialization from string."""
        client = NatsClient(servers="nats://localhost:4222")
        assert client.servers == ["nats://localhost:4222"]

    @pytest.mark.asyncio
    async def test_servers_from_list(self):
        """Test server initialization from list."""
        servers = ["nats://localhost:4222", "nats://backup:4222"]
        client = NatsClient(servers=servers)
        assert client.servers == servers

    @pytest.mark.asyncio
    async def test_publish_without_connection(self):
        """Test publish fails when not connected."""
        client = NatsClient()

        with pytest.raises(RuntimeError, match="Not connected"):
            await client.publish("test.subject", b"data")

    @pytest.mark.asyncio
    async def test_subscribe_without_connection(self):
        """Test subscribe fails when not connected."""
        client = NatsClient()

        async def callback(data):
            pass

        with pytest.raises(RuntimeError, match="Not connected"):
            await client.subscribe("test.subject", callback)

    @pytest.mark.asyncio
    async def test_request_without_connection(self):
        """Test request fails when not connected."""
        client = NatsClient()

        with pytest.raises(RuntimeError, match="Not connected"):
            await client.request("test.subject", b"data")

    @pytest.mark.asyncio
    async def test_publish_jetstream_without_connection(self):
        """Test JetStream publish fails when not connected."""
        client = NatsClient()

        with pytest.raises(RuntimeError, match="Not connected"):
            await client.publish_to_jetstream("test.subject", b"data")

    @pytest.mark.asyncio
    async def test_subscribe_jetstream_without_connection(self):
        """Test JetStream subscribe fails when not connected."""
        client = NatsClient()

        async def callback(msg):
            pass

        with pytest.raises(RuntimeError, match="Not connected"):
            await client.subscribe_jetstream("test.subject", callback)

    @pytest.mark.asyncio
    async def test_ensure_stream_without_connection(self):
        """Test stream creation fails when not connected."""
        client = NatsClient()

        with pytest.raises(RuntimeError, match="Not connected"):
            await client.ensure_stream("STREAM", ["subject.>"])


class TestNatsEventBus:
    """Test NatsEventBus high-level interface."""

    @pytest.fixture
    def event_bus(self):
        """Create event bus with disabled NATS."""
        mock_client = AsyncMock()
        return NatsEventBus(client=mock_client, enabled=False)

    @pytest.fixture
    def enabled_event_bus(self):
        """Create event bus with enabled NATS."""
        mock_client = AsyncMock()
        mock_client.connect = AsyncMock()
        mock_client.disconnect = AsyncMock()
        mock_client.ensure_stream = AsyncMock()
        mock_client.publish_to_jetstream = AsyncMock(return_value="1")
        return NatsEventBus(client=mock_client, enabled=True)

    @pytest.mark.asyncio
    async def test_event_bus_disabled(self, event_bus):
        """Test event bus behavior when disabled."""
        assert not event_bus.enabled
        assert not event_bus._initialized

    @pytest.mark.asyncio
    async def test_event_bus_initialization(self, enabled_event_bus):
        """Test event bus initialization."""
        await enabled_event_bus.initialize()

        assert enabled_event_bus._initialized
        enabled_event_bus.client.connect.assert_called_once()
        assert enabled_event_bus.client.ensure_stream.call_count == 4

    @pytest.mark.asyncio
    async def test_event_bus_shutdown(self, enabled_event_bus):
        """Test event bus shutdown."""
        enabled_event_bus.client.connected = True
        await enabled_event_bus.shutdown()

        enabled_event_bus.client.disconnect.assert_called_once()

    @pytest.mark.asyncio
    async def test_publish_deployment_event(self, enabled_event_bus):
        """Test publishing deployment event."""
        enabled_event_bus._initialized = True

        await enabled_event_bus.publish_deployment_event(
            event_type=DeploymentEvents.STARTED,
            deployment_id="deploy-123",
            data={"service": "api", "provider": "bare_metal"},
            trace_id="trace-abc",
            region="us-east-1",
        )

        enabled_event_bus.client.publish_to_jetstream.assert_called_once()
        call_args = enabled_event_bus.client.publish_to_jetstream.call_args

        assert "fraisier.deployments" in call_args[0][0]
        assert call_args[1]["headers"]["trace_id"] == "trace-abc"

    @pytest.mark.asyncio
    async def test_publish_health_check_event(self, enabled_event_bus):
        """Test publishing health check event."""
        enabled_event_bus._initialized = True

        await enabled_event_bus.publish_health_check_event(
            event_type=HealthCheckEvents.CHECK_PASSED,
            service_name="api",
            data={"check_type": "http", "duration_ms": 150},
            trace_id="trace-xyz",
        )

        enabled_event_bus.client.publish_to_jetstream.assert_called_once()
        call_args = enabled_event_bus.client.publish_to_jetstream.call_args

        assert "fraisier.health_checks" in call_args[0][0]

    @pytest.mark.asyncio
    async def test_publish_database_event(self, enabled_event_bus):
        """Test publishing database event."""
        enabled_event_bus._initialized = True

        await enabled_event_bus.publish_database_event(
            event_type=DatabaseEvents.SCHEMA_CHANGED,
            database_type="postgresql",
            table="deployments",
            data={"changes": {"added_columns": ["created_at"]}},
        )

        enabled_event_bus.client.publish_to_jetstream.assert_called_once()
        call_args = enabled_event_bus.client.publish_to_jetstream.call_args

        subject = call_args[0][0]
        assert "fraisier.databases.postgresql" in subject
        assert "deployments" in subject

    @pytest.mark.asyncio
    async def test_publish_metrics_event(self, enabled_event_bus):
        """Test publishing metrics event."""
        enabled_event_bus._initialized = True

        await enabled_event_bus.publish_metrics_event(
            event_type="deployment",
            data={"duration_seconds": 45.2, "error_rate": 0.0},
        )

        enabled_event_bus.client.publish_to_jetstream.assert_called_once()
        call_args = enabled_event_bus.client.publish_to_jetstream.call_args

        assert "fraisier.metrics" in call_args[0][0]

    @pytest.mark.asyncio
    async def test_publish_event_when_disabled(self, event_bus):
        """Test event publishing is skipped when event bus disabled."""
        event_bus._initialized = True

        event = NatsEvent(
            subject="test.subject",
            event_type="test",
            correlation_id="123",
            trace_id="trace",
            timestamp=datetime.now(timezone.utc),
            region=None,
            source="test",
            data={},
        )

        await event_bus.publish_event(event)
        # Should not raise error, just skip publish


class TestNatsEvent:
    """Test NatsEvent dataclass."""

    @pytest.fixture
    def event(self):
        """Create a test event."""
        return NatsEvent(
            subject="fraisier.deployments.started",
            event_type="deployment.started",
            correlation_id="deploy-123",
            trace_id="trace-abc",
            timestamp=datetime(2026, 1, 22, 12, 0, 0, tzinfo=timezone.utc),
            region="us-east-1",
            source="provider.bare_metal",
            data={"service": "api", "version": "2.0.0"},
        )

    def test_event_creation(self, event):
        """Test event creation."""
        assert event.subject == "fraisier.deployments.started"
        assert event.event_type == "deployment.started"
        assert event.correlation_id == "deploy-123"
        assert event.trace_id == "trace-abc"
        assert event.region == "us-east-1"

    def test_event_to_dict(self, event):
        """Test converting event to dictionary."""
        data = event.to_dict()

        assert data["subject"] == "fraisier.deployments.started"
        assert data["correlation_id"] == "deploy-123"
        assert "2026-01-22" in data["timestamp"]
        assert data["data"]["service"] == "api"

    def test_event_to_json(self, event):
        """Test converting event to JSON."""
        json_str = event.to_json()

        assert isinstance(json_str, str)
        assert "fraisier.deployments.started" in json_str
        assert "deploy-123" in json_str

    def test_event_from_json(self, event):
        """Test deserializing event from JSON."""
        json_str = event.to_json()
        restored = NatsEvent.from_json(json_str)

        assert restored.subject == event.subject
        assert restored.correlation_id == event.correlation_id
        assert restored.data == event.data

    def test_event_from_dict(self, event):
        """Test deserializing event from dictionary."""
        data = event.to_dict()
        restored = NatsEvent.from_dict(data)

        assert restored.subject == event.subject
        assert restored.trace_id == event.trace_id

    def test_event_from_dict_missing_fields(self):
        """Test event creation fails with missing fields."""
        incomplete_data = {
            "subject": "test.subject",
            "event_type": "test",
        }

        with pytest.raises(ValueError, match="Missing required fields"):
            NatsEvent.from_dict(incomplete_data)

    def test_event_timestamp_parsing(self):
        """Test timestamp parsing from ISO string."""
        data = {
            "subject": "test.subject",
            "event_type": "test",
            "correlation_id": "123",
            "trace_id": "abc",
            "timestamp": "2026-01-22T12:00:00+00:00",
            "region": None,
            "source": "test",
            "data": {},
        }

        event = NatsEvent.from_dict(data)
        assert event.timestamp.year == 2026


class TestEventTypes:
    """Test event type constants."""

    def test_deployment_event_types(self):
        """Test deployment event types are defined."""
        types = DeploymentEvents.all_types()
        assert DeploymentEvents.STARTED in types
        assert DeploymentEvents.COMPLETED in types
        assert DeploymentEvents.FAILED in types
        assert len(types) >= 9

    def test_health_check_event_types(self):
        """Test health check event types."""
        types = HealthCheckEvents.all_types()
        assert HealthCheckEvents.CHECK_PASSED in types
        assert HealthCheckEvents.CHECK_FAILED in types
        assert len(types) >= 5

    def test_database_event_types(self):
        """Test database event types."""
        types = DatabaseEvents.all_types()
        assert DatabaseEvents.SCHEMA_CHANGED in types
        assert DatabaseEvents.MIGRATION_STARTED in types
        assert len(types) >= 9

    def test_event_type_values_are_strings(self):
        """Test all event type values are strings."""
        for event_type in DeploymentEvents.all_types():
            assert isinstance(event_type, str)

        for event_type in HealthCheckEvents.all_types():
            assert isinstance(event_type, str)

        for event_type in DatabaseEvents.all_types():
            assert isinstance(event_type, str)
