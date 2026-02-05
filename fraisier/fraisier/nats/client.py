"""NATS client wrapper with connection pooling and event bus interface.

Provides high-level abstractions over nats-py for publishing and subscribing
to events with automatic trace ID propagation and error handling.
"""

import json
import logging
import os
from collections.abc import Callable
from dataclasses import asdict
from typing import Any, TYPE_CHECKING

import nats
from nats.errors import Error as NatsError
from nats.errors import ConnectionClosedError
from nats.js.api import DeliverPolicy, RetentionPolicy

from fraisier.logging import get_contextual_logger
from fraisier.nats.events import NatsEvent

if TYPE_CHECKING:
    from nats.msg import Msg

logger = get_contextual_logger(__name__)


class NatsClient:
    """Low-level NATS client wrapper.

    Handles connection pooling, reconnection logic, and basic
    pub/sub operations with retry support.
    """

    def __init__(
        self,
        servers: list[str] | str | None = None,
        username: str | None = None,
        password: str | None = None,
        timeout: float = 5.0,
        max_reconnect_attempts: int = 60,
        reconnect_time_wait: float = 2.0,
    ):
        """Initialize NATS client.

        Args:
            servers: NATS server URLs (default: localhost:4222)
            username: Optional NATS username
            password: Optional NATS password
            timeout: Connection timeout in seconds
            max_reconnect_attempts: Max reconnection attempts
            reconnect_time_wait: Seconds between reconnection attempts
        """
        if servers is None:
            servers = ["nats://localhost:4222"]
        elif isinstance(servers, str):
            servers = [servers]

        self.servers = servers
        self.username = username
        self.password = password
        self.timeout = timeout
        self.max_reconnect_attempts = max_reconnect_attempts
        self.reconnect_time_wait = reconnect_time_wait

        self.nc: nats.NATS | None = None
        self.js: nats.js.JetStreamContext | None = None
        self._connected = False

    async def connect(self) -> None:
        """Connect to NATS cluster.

        Raises:
            NatsError: If connection fails after all retries
        """
        if self._connected:
            logger.info("Already connected to NATS")
            return

        try:
            self.nc = await nats.connect(
                servers=self.servers,
                user=self.username,
                password=self.password,
                connect_timeout=self.timeout,
                max_reconnect_attempts=self.max_reconnect_attempts,
                reconnect_time_wait=self.reconnect_time_wait,
                name="fraisier",
            )

            # Initialize JetStream context for event sourcing
            self.js = self.nc.jetstream()

            self._connected = True
            logger.info(f"Connected to NATS cluster: {', '.join(self.servers)}")

        except NatsError as e:
            logger.error(f"Failed to connect to NATS: {e}")
            raise

    async def disconnect(self) -> None:
        """Close NATS connection gracefully."""
        if self.nc is not None:
            try:
                await self.nc.close()
                self._connected = False
                logger.info("Disconnected from NATS")
            except Exception as e:
                logger.error(f"Error disconnecting from NATS: {e}")

    @property
    def connected(self) -> bool:
        """Check if connected to NATS."""
        return self._connected

    async def publish(
        self,
        subject: str,
        data: bytes | dict,
        headers: dict[str, str] | None = None,
    ) -> None:
        """Publish message to subject.

        Args:
            subject: NATS subject
            data: Message data (bytes or dict converted to JSON)
            headers: Optional NATS headers for metadata

        Raises:
            RuntimeError: If not connected to NATS
            NatsError: If publish fails
        """
        if not self._connected or self.nc is None:
            raise RuntimeError("Not connected to NATS")

        if isinstance(data, dict):
            data = json.dumps(data).encode()
        elif isinstance(data, str):
            data = data.encode()

        try:
            await self.nc.publish(subject, data, headers=headers)
            logger.debug(f"Published message to {subject}")
        except NatsError as e:
            logger.error(f"Failed to publish to {subject}: {e}")
            raise

    async def publish_to_jetstream(
        self,
        subject: str,
        data: bytes | dict,
        headers: dict[str, str] | None = None,
    ) -> str:
        """Publish message to JetStream for persistence.

        Args:
            subject: NATS subject
            data: Message data (bytes or dict converted to JSON)
            headers: Optional NATS headers

        Returns:
            Message sequence number as string

        Raises:
            RuntimeError: If not connected or JetStream not available
            NatsError: If publish fails
        """
        if not self._connected or self.js is None:
            raise RuntimeError("Not connected to NATS or JetStream not available")

        if isinstance(data, dict):
            data = json.dumps(data).encode()
        elif isinstance(data, str):
            data = data.encode()

        try:
            ack = await self.js.publish(subject, data, headers=headers)
            logger.debug(f"Published to JetStream {subject}, seq={ack.metadata.sequence.stream}")
            return str(ack.metadata.sequence.stream)
        except NatsError as e:
            logger.error(f"Failed to publish to JetStream {subject}: {e}")
            raise

    async def subscribe(
        self,
        subject: str,
        callback: Callable[[bytes], Any],
        queue: str | None = None,
    ) -> None:
        """Subscribe to subject with callback.

        Args:
            subject: Subject pattern (supports wildcards)
            callback: Async callback function that receives message data
            queue: Optional queue group for load balancing

        Raises:
            RuntimeError: If not connected
            NatsError: If subscribe fails
        """
        if not self._connected or self.nc is None:
            raise RuntimeError("Not connected to NATS")

        try:
            await self.nc.subscribe(subject, cb=self._make_callback(callback), queue=queue)
            logger.info(f"Subscribed to {subject}" + (f" (queue: {queue})" if queue else ""))
        except NatsError as e:
            logger.error(f"Failed to subscribe to {subject}: {e}")
            raise

    async def subscribe_jetstream(
        self,
        subject: str,
        callback: Callable[..., Any],
        durable: str | None = None,
        deliver_policy: DeliverPolicy | None = None,
        queue: str | None = None,
    ) -> None:
        """Subscribe to JetStream subject with persistence.

        Args:
            subject: Subject pattern
            callback: Async callback that receives full message
            durable: Optional durable consumer name
            deliver_policy: When to receive messages (all, new, last_per_subject)
            queue: Optional queue group for load balancing

        Raises:
            RuntimeError: If not connected or JetStream not available
            NatsError: If subscribe fails
        """
        if not self._connected or self.js is None:
            raise RuntimeError("Not connected to NATS or JetStream not available")

        try:
            await self.js.subscribe(
                subject,
                cb=callback,
                durable=durable,
                deliver_policy=deliver_policy or DeliverPolicy.ALL,
                queue=queue,
            )
            logger.info(
                f"Subscribed to JetStream {subject}"
                + (f" (durable: {durable})" if durable else "")
                + (f" (queue: {queue})" if queue else "")
            )
        except NatsError as e:
            logger.error(f"Failed to subscribe to JetStream {subject}: {e}")
            raise

    async def request(
        self,
        subject: str,
        data: bytes | dict,
        timeout: float | None = None,
    ) -> bytes:
        """Send request and wait for reply (RPC pattern).

        Args:
            subject: Request subject
            data: Request data
            timeout: Optional timeout in seconds

        Returns:
            Reply message data

        Raises:
            RuntimeError: If not connected
            NatsError: If request fails or times out
        """
        if not self._connected or self.nc is None:
            raise RuntimeError("Not connected to NATS")

        if isinstance(data, dict):
            data = json.dumps(data).encode()
        elif isinstance(data, str):
            data = data.encode()

        try:
            reply = await self.nc.request(subject, data, timeout=timeout or self.timeout)
            logger.debug(f"Received reply to {subject}")
            return reply.data
        except NatsError as e:
            logger.error(f"Request to {subject} failed: {e}")
            raise

    async def ensure_stream(
        self,
        name: str,
        subjects: list[str],
        max_age: int | None = None,
        retention_policy: RetentionPolicy | None = None,
    ) -> None:
        """Create or update JetStream stream.

        Args:
            name: Stream name
            subjects: List of subjects to store
            max_age: Optional max age in nanoseconds
            retention_policy: Optional retention policy (Limits, Interest, WorkQueue)

        Raises:
            RuntimeError: If JetStream not available
            NatsError: If stream creation fails
        """
        if not self._connected or self.js is None:
            raise RuntimeError("Not connected to NATS or JetStream not available")

        try:
            await self.js.add_stream(
                name=name,
                subjects=subjects,
                max_age=max_age,
                retention_policy=retention_policy or RetentionPolicy.LIMITS,
            )
            logger.info(f"JetStream stream created/updated: {name}")
        except Exception as e:
            if "already exists" in str(e):
                logger.debug(f"JetStream stream already exists: {name}")
            else:
                logger.error(f"Failed to create stream {name}: {e}")
                raise

    @staticmethod
    def _make_callback(callback: Callable) -> Callable:
        """Wrap callback to handle NATS message format.

        Args:
            callback: User callback that expects data bytes

        Returns:
            NATS callback that wraps user callback
        """

        async def nats_callback(msg: Any) -> None:
            try:
                await callback(msg.data)
            except Exception as e:
                logger.error(f"Callback error: {e}")

        return nats_callback


class NatsEventBus:
    """High-level event bus interface using NATS.

    Provides domain-specific methods for publishing deployment,
    health check, database, and metrics events.
    """

    def __init__(
        self,
        client: NatsClient | None = None,
        servers: list[str] | str | None = None,
        enabled: bool = True,
    ):
        """Initialize event bus.

        Args:
            client: Optional existing NatsClient (if not provided, creates new one)
            servers: NATS servers (if creating new client)
            enabled: Whether event bus is enabled (can be disabled via config)
        """
        self.enabled = enabled
        self.client = client or NatsClient(servers=servers)
        self._initialized = False

    async def initialize(self) -> None:
        """Initialize connection and create streams.

        Raises:
            NatsError: If initialization fails
        """
        if not self.enabled:
            logger.info("NATS event bus disabled")
            return

        try:
            await self.client.connect()

            # Create streams for different event categories
            await self.client.ensure_stream(
                name="FRAISIER_DEPLOYMENTS",
                subjects=["fraisier.deployments.>"],
                retention_policy=RetentionPolicy.LIMITS,
            )

            await self.client.ensure_stream(
                name="FRAISIER_HEALTH_CHECKS",
                subjects=["fraisier.health_checks.>"],
                retention_policy=RetentionPolicy.LIMITS,
            )

            await self.client.ensure_stream(
                name="FRAISIER_DATABASES",
                subjects=["fraisier.databases.>"],
                retention_policy=RetentionPolicy.LIMITS,
            )

            await self.client.ensure_stream(
                name="FRAISIER_METRICS",
                subjects=["fraisier.metrics.>"],
                retention_policy=RetentionPolicy.LIMITS,
            )

            self._initialized = True
            logger.info("NATS event bus initialized")

        except NatsError as e:
            logger.error(f"Failed to initialize event bus: {e}")
            raise

    async def shutdown(self) -> None:
        """Graceful shutdown of event bus."""
        if self.client and self.client.connected:
            await self.client.disconnect()

    async def publish_event(
        self,
        event: NatsEvent,
        persist: bool = True,
    ) -> None:
        """Publish event with optional persistence.

        Args:
            event: NatsEvent to publish
            persist: Whether to store in JetStream (default: True for event sourcing)

        Raises:
            RuntimeError: If event bus not initialized
        """
        if not self.enabled or not self._initialized:
            logger.debug("Event bus not enabled or initialized, skipping publish")
            return

        try:
            event_data = asdict(event)

            if persist:
                seq = await self.client.publish_to_jetstream(
                    subject=event.subject,
                    data=event_data,
                    headers=self._make_headers(event),
                )
                logger.info(f"Published event {event.event_type} (seq={seq})")
            else:
                await self.client.publish(
                    subject=event.subject,
                    data=event_data,
                    headers=self._make_headers(event),
                )
                logger.info(f"Published event {event.event_type}")

        except Exception as e:
            logger.error(f"Failed to publish event {event.event_type}: {e}")
            # Don't raise - allow deployments to continue even if event bus fails

    async def publish_deployment_event(
        self,
        event_type: str,
        deployment_id: str,
        data: dict,
        trace_id: str | None = None,
        region: str | None = None,
    ) -> None:
        """Publish deployment event.

        Args:
            event_type: Type of deployment event
            deployment_id: Deployment ID
            data: Event payload
            trace_id: Optional trace ID for observability
            region: Optional region information
        """
        from fraisier.nats.events import DeploymentEvents

        event = NatsEvent(
            subject=f"fraisier.deployments.{event_type}",
            event_type=event_type,
            correlation_id=deployment_id,
            trace_id=trace_id or self._get_trace_id(),
            timestamp=self._get_timestamp(),
            region=region,
            source="fraisier.deployer",
            data=data,
        )

        await self.publish_event(event)

    async def publish_health_check_event(
        self,
        event_type: str,
        service_name: str,
        data: dict,
        trace_id: str | None = None,
    ) -> None:
        """Publish health check event.

        Args:
            event_type: Type of health check event
            service_name: Service being checked
            data: Event payload
            trace_id: Optional trace ID
        """
        event = NatsEvent(
            subject=f"fraisier.health_checks.{event_type}",
            event_type=event_type,
            correlation_id=service_name,
            trace_id=trace_id or self._get_trace_id(),
            timestamp=self._get_timestamp(),
            region=None,
            source="fraisier.health_checker",
            data=data,
        )

        await self.publish_event(event)

    async def publish_database_event(
        self,
        event_type: str,
        database_type: str,
        table: str | None,
        data: dict,
        trace_id: str | None = None,
    ) -> None:
        """Publish database event.

        Args:
            event_type: Type of database event
            database_type: Type of database (postgresql, mysql, sqlite)
            table: Optional table name
            data: Event payload
            trace_id: Optional trace ID
        """
        subject = f"fraisier.databases.{database_type}"
        if table:
            subject += f".{table}"
        subject += f".{event_type}"

        event = NatsEvent(
            subject=subject,
            event_type=event_type,
            correlation_id=f"{database_type}:{table or 'all'}",
            trace_id=trace_id or self._get_trace_id(),
            timestamp=self._get_timestamp(),
            region=None,
            source="fraisier.database",
            data=data,
        )

        await self.publish_event(event)

    async def publish_metrics_event(
        self,
        event_type: str,
        data: dict,
        trace_id: str | None = None,
    ) -> None:
        """Publish metrics event.

        Args:
            event_type: Type of metrics event
            data: Metrics data
            trace_id: Optional trace ID
        """
        event = NatsEvent(
            subject=f"fraisier.metrics.{event_type}",
            event_type=event_type,
            correlation_id="metrics",
            trace_id=trace_id or self._get_trace_id(),
            timestamp=self._get_timestamp(),
            region=None,
            source="fraisier.metrics",
            data=data,
        )

        await self.publish_event(event)

    @staticmethod
    def _make_headers(event: NatsEvent) -> dict[str, str]:
        """Create NATS message headers from event metadata.

        Args:
            event: NatsEvent to extract headers from

        Returns:
            Dictionary of headers for NATS message
        """
        return {
            "trace_id": event.trace_id,
            "correlation_id": event.correlation_id,
            "source": event.source,
            "timestamp": event.timestamp.isoformat(),
        }

    @staticmethod
    def _get_trace_id() -> str:
        """Get current trace ID from context or generate new one.

        Returns:
            Trace ID string
        """
        # This can be enhanced to pull from logging context
        import uuid

        return str(uuid.uuid4())[:12]

    @staticmethod
    def _get_timestamp():
        """Get current timestamp.

        Returns:
            Current datetime
        """
        from datetime import datetime, timezone

        return datetime.now(timezone.utc)


def get_nats_event_bus() -> NatsEventBus | None:
    """Get global NATS event bus instance.

    Returns:
        NatsEventBus if enabled, None otherwise
    """
    enabled = os.getenv("FRAISIER_NATS_ENABLED", "false").lower() == "true"

    if not enabled:
        return None

    servers = os.getenv("FRAISIER_NATS_SERVERS", "nats://localhost:4222").split(",")
    username = os.getenv("FRAISIER_NATS_USERNAME")
    password = os.getenv("FRAISIER_NATS_PASSWORD")

    client = NatsClient(
        servers=servers,
        username=username,
        password=password,
    )

    return NatsEventBus(client=client, enabled=enabled)
