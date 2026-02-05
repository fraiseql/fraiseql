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
from fraisier.nats.subscribers import (
    EventSubscriberRegistry,
    EventSubscription,
    EventFilter,
    EventHandler,
    AsyncEventHandler,
    EventHandlers,
    get_subscriber_registry,
    reset_subscriber_registry,
)
from fraisier.nats.config import (
    NatsConnectionConfig,
    NatsStreamConfig,
    NatsRegionalConfig,
    NatsEventHandlerConfig,
    NatsFullConfig,
    is_nats_enabled,
    get_nats_config,
)

__all__ = [
    "NatsClient",
    "NatsEventBus",
    "NatsEvent",
    "NatsEventProvider",
    "DeploymentEvents",
    "HealthCheckEvents",
    "DatabaseEvents",
    "MetricsEvents",
    "EventSubscriberRegistry",
    "EventSubscription",
    "EventFilter",
    "EventHandler",
    "AsyncEventHandler",
    "EventHandlers",
    "get_subscriber_registry",
    "reset_subscriber_registry",
    "NatsConnectionConfig",
    "NatsStreamConfig",
    "NatsRegionalConfig",
    "NatsEventHandlerConfig",
    "NatsFullConfig",
    "is_nats_enabled",
    "get_nats_config",
]
