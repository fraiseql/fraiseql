"""NATS configuration management.

Loads NATS settings from environment variables and provides configuration
objects for initializing NATS clients and event buses.
"""

import os
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class NatsConnectionConfig:
    """NATS connection configuration.

    Settings for connecting to NATS servers, including authentication,
    timeout, and reconnection parameters.
    """

    servers: list[str] = field(default_factory=lambda: ["nats://localhost:4222"])
    """NATS server URLs (comma-separated or list)."""

    username: Optional[str] = None
    """NATS username for authentication."""

    password: Optional[str] = None
    """NATS password for authentication."""

    timeout: float = 5.0
    """Connection timeout in seconds."""

    max_reconnect_attempts: int = 60
    """Maximum number of reconnection attempts."""

    reconnect_time_wait: float = 2.0
    """Time to wait between reconnection attempts in seconds."""

    @classmethod
    def from_env(cls) -> "NatsConnectionConfig":
        """Load NATS connection config from environment variables.

        Environment variables:
            NATS_SERVERS: NATS server URLs (comma-separated)
            NATS_USERNAME: NATS username
            NATS_PASSWORD: NATS password
            NATS_TIMEOUT: Connection timeout in seconds
            NATS_MAX_RECONNECT_ATTEMPTS: Max reconnection attempts
            NATS_RECONNECT_TIME_WAIT: Time between retries in seconds

        Returns:
            NatsConnectionConfig with values from environment
        """
        servers_str = os.getenv("NATS_SERVERS", "nats://localhost:4222")
        servers = [s.strip() for s in servers_str.split(",")]

        return cls(
            servers=servers,
            username=os.getenv("NATS_USERNAME"),
            password=os.getenv("NATS_PASSWORD"),
            timeout=float(os.getenv("NATS_TIMEOUT", "5.0")),
            max_reconnect_attempts=int(
                os.getenv("NATS_MAX_RECONNECT_ATTEMPTS", "60")
            ),
            reconnect_time_wait=float(os.getenv("NATS_RECONNECT_TIME_WAIT", "2.0")),
        )

    def to_nats_client_kwargs(self) -> dict:
        """Convert to kwargs for NatsClient initialization.

        Returns:
            Dict of keyword arguments for NatsClient.__init__
        """
        kwargs = {
            "servers": self.servers,
            "timeout": self.timeout,
            "max_reconnect_attempts": self.max_reconnect_attempts,
            "reconnect_time_wait": self.reconnect_time_wait,
        }

        if self.username:
            kwargs["username"] = self.username
        if self.password:
            kwargs["password"] = self.password

        return kwargs


@dataclass
class NatsStreamConfig:
    """NATS stream configuration.

    Settings for JetStream streams storing persistent events.
    """

    deployment_events_retention_hours: int = 720  # 30 days
    """Retention period for deployment events in hours."""

    health_events_retention_hours: int = 168  # 7 days
    """Retention period for health check events in hours."""

    database_events_retention_hours: int = 720  # 30 days
    """Retention period for database events in hours."""

    metrics_events_retention_hours: int = 168  # 7 days
    """Retention period for metrics events in hours."""

    max_stream_size: int = 1073741824  # 1GB
    """Maximum size per stream in bytes."""

    @classmethod
    def from_env(cls) -> "NatsStreamConfig":
        """Load NATS stream config from environment variables.

        Environment variables:
            NATS_DEPLOYMENT_EVENTS_RETENTION: Deployment retention (hours)
            NATS_HEALTH_EVENTS_RETENTION: Health check retention (hours)
            NATS_DATABASE_EVENTS_RETENTION: Database retention (hours)
            NATS_METRICS_EVENTS_RETENTION: Metrics retention (hours)
            NATS_STREAM_MAX_SIZE: Max stream size (bytes)

        Returns:
            NatsStreamConfig with values from environment
        """
        return cls(
            deployment_events_retention_hours=int(
                os.getenv("NATS_DEPLOYMENT_EVENTS_RETENTION", "720")
            ),
            health_events_retention_hours=int(
                os.getenv("NATS_HEALTH_EVENTS_RETENTION", "168")
            ),
            database_events_retention_hours=int(
                os.getenv("NATS_DATABASE_EVENTS_RETENTION", "720")
            ),
            metrics_events_retention_hours=int(
                os.getenv("NATS_METRICS_EVENTS_RETENTION", "168")
            ),
            max_stream_size=int(
                os.getenv("NATS_STREAM_MAX_SIZE", "1073741824")
            ),
        )


@dataclass
class NatsRegionalConfig:
    """Multi-region deployment configuration.

    Settings for NATS event routing in multi-region deployments.
    """

    region: str = "default"
    """Current deployment region."""

    all_regions: list[str] = field(default_factory=lambda: ["default"])
    """List of all deployment regions."""

    inter_region_timeout: float = 30.0
    """Timeout for inter-region communication in seconds."""

    @classmethod
    def from_env(cls) -> "NatsRegionalConfig":
        """Load regional config from environment variables.

        Environment variables:
            NATS_REGION: Current region name
            DEPLOYMENT_REGIONS: All regions (comma-separated)
            INTER_REGION_TIMEOUT: Inter-region timeout (seconds)

        Returns:
            NatsRegionalConfig with values from environment
        """
        region = os.getenv("NATS_REGION", "default")
        regions_str = os.getenv("DEPLOYMENT_REGIONS", region)
        all_regions = [r.strip() for r in regions_str.split(",")]

        return cls(
            region=region,
            all_regions=all_regions,
            inter_region_timeout=float(os.getenv("INTER_REGION_TIMEOUT", "30.0")),
        )


@dataclass
class NatsEventHandlerConfig:
    """Event handler configuration.

    Settings for NATS event subscribers and handlers.
    """

    enable_webhook_notifications: bool = True
    """Enable webhook notifications for events."""

    deployment_webhook_url: Optional[str] = None
    """Webhook URL for deployment events."""

    enable_metrics_recording: bool = True
    """Enable recording of metrics from events."""

    enable_event_logging: bool = True
    """Enable logging of events to database."""

    @classmethod
    def from_env(cls) -> "NatsEventHandlerConfig":
        """Load handler config from environment variables.

        Environment variables:
            ENABLE_WEBHOOK_NOTIFICATIONS: Enable webhooks (true/false)
            DEPLOYMENT_WEBHOOK_URL: Webhook URL
            ENABLE_METRICS_RECORDING: Enable metrics (true/false)
            ENABLE_EVENT_LOGGING: Enable logging (true/false)

        Returns:
            NatsEventHandlerConfig with values from environment
        """
        def parse_bool(value: Optional[str], default: bool = True) -> bool:
            if value is None:
                return default
            return value.lower() in ("true", "1", "yes", "on")

        return cls(
            enable_webhook_notifications=parse_bool(
                os.getenv("ENABLE_WEBHOOK_NOTIFICATIONS"), True
            ),
            deployment_webhook_url=os.getenv("DEPLOYMENT_WEBHOOK_URL"),
            enable_metrics_recording=parse_bool(
                os.getenv("ENABLE_METRICS_RECORDING"), True
            ),
            enable_event_logging=parse_bool(
                os.getenv("ENABLE_EVENT_LOGGING"), True
            ),
        )


@dataclass
class NatsFullConfig:
    """Complete NATS configuration.

    Combines all NATS-related configuration into a single container.
    """

    connection: NatsConnectionConfig
    """NATS connection settings."""

    streams: NatsStreamConfig
    """NATS stream settings."""

    regional: NatsRegionalConfig
    """Multi-region settings."""

    handlers: NatsEventHandlerConfig
    """Event handler settings."""

    @classmethod
    def from_env(cls) -> "NatsFullConfig":
        """Load complete NATS config from environment.

        Returns:
            NatsFullConfig with all settings from environment
        """
        return cls(
            connection=NatsConnectionConfig.from_env(),
            streams=NatsStreamConfig.from_env(),
            regional=NatsRegionalConfig.from_env(),
            handlers=NatsEventHandlerConfig.from_env(),
        )


# Environment detection
def is_nats_enabled() -> bool:
    """Check if NATS is enabled via environment variables.

    Returns:
        True if NATS_SERVERS is configured, False otherwise
    """
    return bool(os.getenv("NATS_SERVERS"))


def get_nats_config() -> NatsFullConfig:
    """Get NATS configuration from environment.

    Returns:
        Complete NATS configuration

    Raises:
        ValueError: If NATS_SERVERS is not configured
    """
    if not is_nats_enabled():
        raise ValueError(
            "NATS is not configured. Set NATS_SERVERS environment variable."
        )

    return NatsFullConfig.from_env()
