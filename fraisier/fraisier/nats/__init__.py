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

__all__ = [
    "NatsClient",
    "NatsEventBus",
    "NatsEvent",
    "DeploymentEvents",
    "HealthCheckEvents",
    "DatabaseEvents",
    "MetricsEvents",
]
