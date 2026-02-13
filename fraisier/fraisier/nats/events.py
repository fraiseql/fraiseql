"""NATS event format definitions.

Defines standard event types and the NatsEvent dataclass for
publishing and subscribing to events across the Fraisier ecosystem.
"""

from dataclasses import dataclass, field, asdict
from datetime import datetime, timezone
from typing import Any
import json


@dataclass
class NatsEvent:
    """Standard NATS event wrapper for all Fraisier events.

    Attributes:
        subject: NATS subject (e.g., "fraisier.deployments.started")
        event_type: Type of event (e.g., "deployment.started")
        correlation_id: Unique ID linking related events
        trace_id: Trace ID for distributed tracing
        timestamp: When event was created
        region: Optional region information for multi-region deployments
        source: Who published this event (e.g., "provider.bare_metal")
        data: Event payload (arbitrary dict)
    """

    subject: str
    event_type: str
    correlation_id: str
    trace_id: str
    timestamp: datetime
    region: str | None
    source: str
    data: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to JSON-serializable dictionary.

        Returns:
            Dictionary representation of event
        """
        data = asdict(self)
        # Convert datetime to ISO format
        data["timestamp"] = self.timestamp.isoformat()
        return data

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "NatsEvent":
        """Deserialize event from dictionary.

        Args:
            data: Dictionary with event data

        Returns:
            NatsEvent instance

        Raises:
            ValueError: If required fields are missing
        """
        required_fields = {
            "subject",
            "event_type",
            "correlation_id",
            "trace_id",
            "timestamp",
            "source",
        }

        if not all(field in data for field in required_fields):
            missing = required_fields - set(data.keys())
            raise ValueError(f"Missing required fields: {missing}")

        # Parse timestamp if string
        timestamp = data["timestamp"]
        if isinstance(timestamp, str):
            timestamp = datetime.fromisoformat(timestamp)

        return cls(
            subject=data["subject"],
            event_type=data["event_type"],
            correlation_id=data["correlation_id"],
            trace_id=data["trace_id"],
            timestamp=timestamp,
            region=data.get("region"),
            source=data["source"],
            data=data.get("data", {}),
        )

    def to_json(self) -> str:
        """Serialize to JSON string.

        Returns:
            JSON string representation
        """
        return json.dumps(self.to_dict())

    @classmethod
    def from_json(cls, json_str: str) -> "NatsEvent":
        """Deserialize from JSON string.

        Args:
            json_str: JSON string

        Returns:
            NatsEvent instance
        """
        data = json.loads(json_str)
        return cls.from_dict(data)


class DeploymentEvents:
    """Deployment event type constants.

    Subject format: fraisier.deployments.{event_type}
    """

    TRIGGERED = "triggered"
    """New deployment requested (webhook or manual)"""

    STARTED = "started"
    """Provider started deployment execution"""

    HEALTH_CHECK_STARTED = "health_check_started"
    """Health check started for deployment"""

    HEALTH_CHECK_PASSED = "health_check_passed"
    """Health check passed for deployment"""

    HEALTH_CHECK_FAILED = "health_check_failed"
    """Health check failed for deployment"""

    COMPLETED = "completed"
    """Deployment completed successfully"""

    FAILED = "failed"
    """Deployment failed"""

    ROLLED_BACK = "rolled_back"
    """Deployment rolled back"""

    METRICS_RECORDED = "metrics_recorded"
    """Deployment metrics recorded"""

    # Convenience method to validate event types
    @classmethod
    def all_types(cls) -> list[str]:
        """Get all deployment event types.

        Returns:
            List of event type constants
        """
        return [
            cls.TRIGGERED,
            cls.STARTED,
            cls.HEALTH_CHECK_STARTED,
            cls.HEALTH_CHECK_PASSED,
            cls.HEALTH_CHECK_FAILED,
            cls.COMPLETED,
            cls.FAILED,
            cls.ROLLED_BACK,
            cls.METRICS_RECORDED,
        ]


class HealthCheckEvents:
    """Health check event type constants.

    Subject format: fraisier.health_checks.{event_type}
    """

    CHECK_STARTED = "started"
    """Health check started"""

    CHECK_PASSED = "passed"
    """Health check passed"""

    CHECK_FAILED = "failed"
    """Health check failed"""

    CHECK_TIMEOUT = "timeout"
    """Health check timed out"""

    RETRY_STARTED = "retry_started"
    """Retry of failed health check started"""

    @classmethod
    def all_types(cls) -> list[str]:
        """Get all health check event types.

        Returns:
            List of event type constants
        """
        return [
            cls.CHECK_STARTED,
            cls.CHECK_PASSED,
            cls.CHECK_FAILED,
            cls.CHECK_TIMEOUT,
            cls.RETRY_STARTED,
        ]


class DatabaseEvents:
    """Database event type constants.

    Subject format: fraisier.databases.{db_type}.{table}.{event_type}
    Example: fraisier.databases.postgresql.deployments.schema_changed
    """

    SCHEMA_CHANGED = "schema_changed"
    """Database schema was modified"""

    DATA_MIGRATED = "data_migrated"
    """Data migration completed"""

    MIGRATION_STARTED = "migration_started"
    """Database migration started"""

    MIGRATION_FAILED = "migration_failed"
    """Database migration failed"""

    CONNECTION_POOL_EXHAUSTED = "pool_exhausted"
    """Connection pool exhausted"""

    CONNECTION_TIMEOUT = "connection_timeout"
    """Database connection timeout"""

    QUERY_SLOW = "query_slow"
    """Slow query detected"""

    INDEX_CREATED = "index_created"
    """Database index created"""

    VACUUM_COMPLETED = "vacuum_completed"
    """Database vacuum completed"""

    @classmethod
    def all_types(cls) -> list[str]:
        """Get all database event types.

        Returns:
            List of event type constants
        """
        return [
            cls.SCHEMA_CHANGED,
            cls.DATA_MIGRATED,
            cls.MIGRATION_STARTED,
            cls.MIGRATION_FAILED,
            cls.CONNECTION_POOL_EXHAUSTED,
            cls.CONNECTION_TIMEOUT,
            cls.QUERY_SLOW,
            cls.INDEX_CREATED,
            cls.VACUUM_COMPLETED,
        ]


class MetricsEvents:
    """Metrics event type constants.

    Subject format: fraisier.metrics.{event_type}
    """

    DEPLOYMENT_METRICS = "deployment"
    """Deployment metrics snapshot"""

    PROVIDER_METRICS = "provider"
    """Provider health and resource metrics"""

    DATABASE_METRICS = "database"
    """Database performance metrics"""

    HEALTH_CHECK_METRICS = "health_check"
    """Health check result metrics"""

    SYSTEM_METRICS = "system"
    """System resource metrics"""

    @classmethod
    def all_types(cls) -> list[str]:
        """Get all metrics event types.

        Returns:
            List of event type constants
        """
        return [
            cls.DEPLOYMENT_METRICS,
            cls.PROVIDER_METRICS,
            cls.DATABASE_METRICS,
            cls.HEALTH_CHECK_METRICS,
            cls.SYSTEM_METRICS,
        ]


class RegionalEvents:
    """Regional event subject prefixes for multi-region deployments.

    Subject format: fraisier.regions.{region}.{event_category}.{event_type}
    Example: fraisier.regions.us-east-1.deployments.started
    """

    @staticmethod
    def deployment_subject(region: str | None, event_type: str) -> str:
        """Build deployment subject for region.

        Args:
            region: Region name or None for global
            event_type: Deployment event type

        Returns:
            NATS subject string
        """
        if region:
            return f"fraisier.regions.{region}.deployments.{event_type}"
        return f"fraisier.deployments.{event_type}"

    @staticmethod
    def provider_subject(region: str | None, provider_name: str) -> str:
        """Build provider subject for region.

        Args:
            region: Region name or None for global
            provider_name: Provider name (bare_metal, docker_compose, coolify)

        Returns:
            NATS subject string
        """
        if region:
            return f"fraisier.regions.{region}.providers.{provider_name}.>"
        return f"fraisier.providers.{provider_name}.>"


# Event data schemas (for reference - not enforced at runtime)
DEPLOYMENT_EVENT_DATA_SCHEMA = {
    "triggered": {
        "service": str,
        "branch": str,
        "commit_sha": str,
        "triggered_by": str,
    },
    "started": {
        "service": str,
        "provider": str,
        "strategy": str,
    },
    "completed": {
        "service": str,
        "provider": str,
        "status": str,  # "success" or "failure"
        "duration_seconds": float,
    },
    "failed": {
        "service": str,
        "error": str,
        "provider": str,
        "duration_seconds": float,
    },
}

HEALTH_CHECK_EVENT_DATA_SCHEMA = {
    "started": {
        "service": str,
        "check_type": str,  # "http", "tcp", "exec", "systemd"
        "endpoint": str,
    },
    "passed": {
        "service": str,
        "check_type": str,
        "duration_ms": int,
    },
    "failed": {
        "service": str,
        "check_type": str,
        "reason": str,
        "duration_ms": int,
    },
}

DATABASE_EVENT_DATA_SCHEMA = {
    "schema_changed": {
        "database": str,
        "tables": list[str],
        "changes": dict,
    },
    "migration_started": {
        "database": str,
        "migration_name": str,
    },
    "migration_failed": {
        "database": str,
        "migration_name": str,
        "error": str,
    },
}
