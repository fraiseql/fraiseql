"""NATS event bus integration for Fraisier.

Provides event-driven coordination across multi-region deployments
using NATS JetStream for persistent event sourcing.
"""

from fraisier.nats.client import NatsClient, NatsEventBus
from fraisier.nats.events import (
    DeploymentEvents,
    HealthCheckEvents,
    DatabaseEvents,
    MetricsEvents,
    NatsEvent,
)
from fraisier.nats.provider import NatsEventProvider

__all__ = [
    "NatsClient",
    "NatsEventBus",
    "NatsEvent",
    "NatsEventProvider",
    "DeploymentEvents",
    "HealthCheckEvents",
    "DatabaseEvents",
    "MetricsEvents",
]
