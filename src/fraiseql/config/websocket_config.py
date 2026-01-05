"""WebSocket subscription configuration for FraiseQL.

Provides simple, zero-configuration WebSocket support with sensible defaults.
Supports three preset modes: DEVELOPMENT, PRODUCTION, HIGH_PERFORMANCE.

Philosophy:
- Simple Python API: subscriptions=True enables subscriptions
- Smart Defaults: Sensible per-mode configuration
- Progressive Disclosure: Simple → Standard → Advanced
- Auto-Detection: Redis, PostgreSQL event routing auto-detected
"""

import os
from dataclasses import dataclass, field, fields, replace
from typing import Any, Literal

__all__ = ["WebSocketConfig", "WebSocketPresets"]


@dataclass
class WebSocketConfig:
    """WebSocket subscription configuration with all-optional fields.

    All fields are optional with intelligent defaults. Configuration can be:
    1. Minimal: subscriptions=True (uses DEVELOPMENT preset)
    2. Preset-based: subscriptions=WebSocketPresets.PRODUCTION
    3. Custom: subscriptions=WebSocketConfig(custom_fields=...)

    Examples:
        # Minimal setup
        config = WebSocketConfig()

        # Custom setup
        config = WebSocketConfig(
            max_subscriptions_per_connection=5,
            event_backend="redis",
            require_authentication=True,
        )
    """

    # Mode selection (overrides all other settings if specified)
    mode: Literal["development", "production", "high_performance"] | None = None
    """Preset mode: development, production, or high_performance.
    When set, overrides other fields with preset values."""

    # Basic control
    enabled: bool = True
    """Whether WebSocket subscriptions are enabled."""

    # Event routing configuration
    event_backend: Literal["redis", "postgresql", "memory"] | None = None
    """Event backend for pub/sub: redis, postgresql, or memory.
    If None, auto-detects: tries Redis first, falls back to memory."""

    redis_url: str | None = None
    """Redis connection URL. If None, auto-detects from REDIS_URL env var."""

    # Resource limits (None = use preset defaults)
    max_subscriptions_per_connection: int | None = None
    """Maximum subscriptions per WebSocket connection."""

    max_concurrent_connections: int | None = None
    """Maximum concurrent WebSocket connections."""

    max_connections_per_user: int | None = None
    """Maximum concurrent connections per authenticated user."""

    max_filter_complexity: int | None = None
    """Maximum complexity score for subscription filters."""

    max_event_payload_size: int | None = None
    """Maximum size of event payloads in bytes."""

    max_query_size: int | None = None
    """Maximum size of subscription queries in bytes."""

    # Timeouts and intervals
    ping_interval_seconds: int | None = None
    """Interval for WebSocket ping messages in seconds."""

    connection_init_timeout_seconds: int | None = None
    """Timeout for connection initialization in seconds."""

    pong_timeout_seconds: int | None = None
    """Timeout for pong response after ping in seconds."""

    shutdown_grace_seconds: int | None = None
    """Grace period for graceful shutdown in seconds."""

    # Rate limiting
    rate_limit_enabled: bool | None = None
    """Whether rate limiting is enabled."""

    max_events_per_subscription_per_second: int | None = None
    """Maximum events per subscription per second."""

    max_subscriptions_per_user: int | None = None
    """Maximum subscriptions per authenticated user."""

    # Authentication
    require_authentication: bool | None = None
    """Whether authentication is required for subscriptions."""

    auth_via_connection_params: bool = True
    """Whether to accept auth tokens via connection params."""

    # Caching
    enable_caching: bool | None = None
    """Whether to enable subscription result caching."""

    cache_ttl_seconds: int | None = None
    """Cache TTL for subscription results in seconds."""

    # Advanced features
    enable_complexity_analysis: bool | None = None
    """Whether to analyze subscription complexity."""

    enable_filtering: bool | None = None
    """Whether to enable subscription filtering."""

    # Additional metadata
    _resolved: bool = field(default=False, repr=False, init=False)
    """Internal flag indicating if config has been resolved."""

    def __post_init__(self) -> None:
        """Validate configuration after initialization."""
        if self.enabled is False:
            # If explicitly disabled, clear other settings
            self.event_backend = None
            self.rate_limit_enabled = False

    def resolve(
        self,
        database_url: str | None = None,
        environment: Literal["development", "production", "testing"] | None = None,
    ) -> "WebSocketConfig":
        """Resolve all None values to sensible defaults.

        Args:
            database_url: PostgreSQL connection URL for event backend detection
            environment: Application environment for preset selection

        Returns:
            Fully resolved WebSocketConfig with all None values filled in.
        """
        if self._resolved:
            return self

        # If mode is specified, start with preset
        if self.mode:
            preset = _get_preset_for_mode(self.mode)
            resolved = self._merge_with_preset(preset)
        else:
            resolved = replace(self)

        # Auto-detect Redis if not specified
        if resolved.event_backend is None:
            redis_url = resolved.redis_url or os.getenv("REDIS_URL")
            if redis_url:
                resolved.event_backend = "redis"
                resolved.redis_url = redis_url
            else:
                resolved.event_backend = "memory"

        # Apply remaining defaults for None values
        if resolved.max_subscriptions_per_connection is None:
            resolved.max_subscriptions_per_connection = 10

        if resolved.max_concurrent_connections is None:
            resolved.max_concurrent_connections = 500

        if resolved.max_connections_per_user is None:
            resolved.max_connections_per_user = 5

        if resolved.max_filter_complexity is None:
            resolved.max_filter_complexity = 50

        if resolved.max_event_payload_size is None:
            resolved.max_event_payload_size = 1024 * 1024  # 1MB

        if resolved.max_query_size is None:
            resolved.max_query_size = 64 * 1024  # 64KB

        if resolved.ping_interval_seconds is None:
            resolved.ping_interval_seconds = 30

        if resolved.connection_init_timeout_seconds is None:
            resolved.connection_init_timeout_seconds = 10

        if resolved.pong_timeout_seconds is None:
            resolved.pong_timeout_seconds = 5

        if resolved.shutdown_grace_seconds is None:
            resolved.shutdown_grace_seconds = 5

        if resolved.rate_limit_enabled is None:
            resolved.rate_limit_enabled = True

        if resolved.max_events_per_subscription_per_second is None:
            resolved.max_events_per_subscription_per_second = 100

        if resolved.max_subscriptions_per_user is None:
            resolved.max_subscriptions_per_user = 50

        if resolved.require_authentication is None:
            resolved.require_authentication = False

        if resolved.enable_caching is None:
            resolved.enable_caching = True

        if resolved.cache_ttl_seconds is None:
            resolved.cache_ttl_seconds = 300

        if resolved.enable_complexity_analysis is None:
            resolved.enable_complexity_analysis = True

        if resolved.enable_filtering is None:
            resolved.enable_filtering = True

        # Mark as resolved
        object.__setattr__(resolved, "_resolved", True)

        return resolved

    def _merge_with_preset(self, preset: "WebSocketConfig") -> "WebSocketConfig":
        """Merge this config with a preset, with self values taking precedence.

        Args:
            preset: Preset configuration to merge with

        Returns:
            Merged configuration with preset as base and self values on top.
        """
        # Get all field names
        field_names = {f.name for f in fields(self) if not f.name.startswith("_")}

        merged_dict = {}
        for field_name in field_names:
            self_value = getattr(self, field_name)
            preset_value = getattr(preset, field_name)

            # Use self value if not None/default, otherwise use preset
            if self_value is not None and self_value != field_names:
                merged_dict[field_name] = self_value
            else:
                merged_dict[field_name] = preset_value

        return WebSocketConfig(**merged_dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert config to dictionary for Rust binding.

        Returns:
            Dictionary representation with None values excluded.
        """
        result = {}
        for f in fields(self):
            if f.name.startswith("_"):
                continue
            value = getattr(self, f.name)
            if value is not None:
                result[f.name] = value
        return result


class WebSocketPresets:
    """Pre-configured WebSocket settings for common scenarios."""

    DEVELOPMENT = WebSocketConfig(
        mode="development",
        enabled=True,
        event_backend="memory",
        max_subscriptions_per_connection=10,
        max_concurrent_connections=100,
        max_connections_per_user=10,
        ping_interval_seconds=30,
        connection_init_timeout_seconds=10,
        rate_limit_enabled=False,  # Permissive for development
        require_authentication=False,
        enable_caching=False,  # No caching in development
        enable_complexity_analysis=True,
        enable_filtering=True,
    )
    """Development preset: Permissive, minimal rate limiting, in-memory events.

    Use for local development and testing.
    - In-memory event bus (no Redis needed)
    - No rate limiting
    - Higher connection limits
    - No auth required
    """

    PRODUCTION = WebSocketConfig(
        mode="production",
        enabled=True,
        event_backend="redis",  # Will fallback to memory if Redis unavailable
        max_subscriptions_per_connection=5,
        max_concurrent_connections=500,
        max_connections_per_user=5,
        ping_interval_seconds=20,
        connection_init_timeout_seconds=5,
        rate_limit_enabled=True,
        max_events_per_subscription_per_second=100,
        max_subscriptions_per_user=50,
        require_authentication=True,
        enable_caching=True,
        cache_ttl_seconds=300,
        enable_complexity_analysis=True,
        enable_filtering=True,
    )
    """Production preset: Secure, optimized, Redis-backed events.

    Use for production deployments.
    - Redis event bus (with memory fallback)
    - Rate limiting enabled
    - Stricter connection limits
    - Authentication required
    - Result caching enabled
    """

    HIGH_PERFORMANCE = WebSocketConfig(
        mode="high_performance",
        enabled=True,
        event_backend="redis",
        max_subscriptions_per_connection=20,
        max_concurrent_connections=2000,
        max_connections_per_user=20,
        ping_interval_seconds=10,
        connection_init_timeout_seconds=5,
        rate_limit_enabled=True,
        max_events_per_subscription_per_second=1000,
        max_subscriptions_per_user=100,
        require_authentication=False,
        enable_caching=True,
        cache_ttl_seconds=600,
        enable_complexity_analysis=False,  # Skip for performance
        enable_filtering=True,
    )
    """High-performance preset: Maximum throughput, Redis-backed, relaxed limits.

    Use for high-traffic deployments.
    - Redis event bus (required)
    - Higher rate limits
    - More subscriptions per connection
    - Complexity analysis disabled (performance trade-off)
    - Result caching enabled with longer TTL
    """


def _get_preset_for_mode(
    mode: Literal["development", "production", "high_performance"],
) -> WebSocketConfig:
    """Get preset configuration for a mode.

    Args:
        mode: Mode name

    Returns:
        WebSocketConfig preset for the mode

    Raises:
        ValueError: If mode is not recognized
    """
    presets = {
        "development": WebSocketPresets.DEVELOPMENT,
        "production": WebSocketPresets.PRODUCTION,
        "high_performance": WebSocketPresets.HIGH_PERFORMANCE,
    }

    if mode not in presets:
        raise ValueError(
            f"Unknown WebSocket mode: {mode}. Must be one of: {', '.join(presets.keys())}",
        )

    return presets[mode]


def resolve_websocket_config(
    config: bool | WebSocketConfig | dict | None,
    environment: Literal["development", "production", "testing"] = "development",
    database_url: str | None = None,
) -> WebSocketConfig | None:
    """Resolve WebSocket configuration from various input types.

    Handles:
    - True: Use preset based on environment
    - False/None: Subscriptions disabled
    - WebSocketConfig: Use as-is
    - dict: Create WebSocketConfig from dict

    Args:
        config: Configuration input
        environment: Application environment
        database_url: Database URL for event backend detection

    Returns:
        Resolved WebSocketConfig or None if disabled
    """
    # Disabled explicitly
    if config is None or config is False:
        return None

    # Simple enable with preset
    if config is True:
        if environment == "production":
            return WebSocketPresets.PRODUCTION.resolve(
                database_url=database_url,
                environment=environment,
            )
        return WebSocketPresets.DEVELOPMENT.resolve(
            database_url=database_url,
            environment=environment,
        )

    # WebSocketConfig instance
    if isinstance(config, WebSocketConfig):
        return config.resolve(database_url=database_url, environment=environment)

    # Dictionary
    if isinstance(config, dict):
        ws_config = WebSocketConfig(**config)
        return ws_config.resolve(database_url=database_url, environment=environment)

    raise TypeError(
        f"Invalid websocket_config type: {type(config)}. Must be bool, WebSocketConfig, or dict",
    )
