"""GraphQL subscriptions support for FraiseQL."""

from .caching import cache
from .complexity import complexity
from .decorator import subscription
from .filtering import filter as subscription_filter

# Alias for backward compatibility
filter = subscription_filter  # noqa: A001
from .lifecycle import with_lifecycle
from .websocket import (
    ConnectionState,
    GraphQLWSMessage,
    MessageType,
    SubProtocol,
    SubscriptionManager,
    WebSocketConnection,
)

__all__ = [
    "ConnectionState",
    "GraphQLWSMessage",
    "MessageType",
    "SubProtocol",
    "SubscriptionManager",
    "WebSocketConnection",
    "cache",
    "complexity",
    "filter",
    "subscription",
    "subscription_filter",
    "with_lifecycle",
]
