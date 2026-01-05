"""GraphQL subscriptions support for FraiseQL."""

from collections.abc import Callable
from typing import Any, TypeVar

from .caching import cache
from .complexity import complexity
from .decorator import subscription, websocket_auth
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

F = TypeVar("F")

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
    "simple_subscription",
    "subscription",
    "subscription_filter",
    "websocket_auth",
    "with_lifecycle",
]


def simple_subscription(
    fn: Callable[..., Any] | None = None,
    cache_ttl: float | None = None,
    require_auth: bool | None = None,
    roles: list[str] | None = None,
) -> Callable[[Any], Any]:
    """Simplified subscription decorator combining common patterns.

    Combines @subscription + @cache + @websocket_auth into a single decorator
    for maximum convenience.

    Args:
        fn: The subscription function (when used without parentheses)
        cache_ttl: Cache TTL in seconds (if None, caching disabled)
        require_auth: Whether to require authentication (if None, inherited from config)
        roles: Required roles for authentication (optional)

    Returns:
        Decorated function with combined behavior

    Examples:
        # Simple subscription without any extras
        @simple_subscription
        async def my_sub(info):
            yield data

        # With caching
        @simple_subscription(cache_ttl=10)
        async def cached_sub(info):
            yield data

        # With authentication and caching
        @simple_subscription(cache_ttl=5, require_auth=True, roles=["admin"])
        async def protected_sub(info):
            yield admin_data

        # Just auth, no caching
        @simple_subscription(require_auth=True)
        async def auth_sub(info):
            yield data
    """

    def decorator(func: F) -> F:
        """Apply combined decorators."""
        result = func

        # Apply cache if specified
        if cache_ttl is not None:
            result = cache(ttl=cache_ttl)(result)  # type: ignore[assignment]

        # Apply auth if specified
        if require_auth is not None:
            result = websocket_auth(required=require_auth, roles=roles)(result)  # type: ignore[assignment]

        # Apply subscription decorator last (it needs to register with schema)
        return subscription(result)  # type: ignore[assignment]


    # Allow usage with or without parentheses
    if fn is not None:
        # Called as @simple_subscription (without parentheses)
        return decorator(fn)
    # Called as @simple_subscription() or @simple_subscription(cache_ttl=10)
    return decorator
