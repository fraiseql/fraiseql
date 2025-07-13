"""Configuration and dependencies for FraiseQL metrics.

This module handles prometheus_client availability and provides
configuration for metrics collection.
"""

from dataclasses import dataclass
from dataclasses import field as dataclass_field

try:
    from prometheus_client import (  # type: ignore[import-untyped]
        CONTENT_TYPE_LATEST,
        CollectorRegistry,
        Counter,
        Gauge,
        Histogram,
        generate_latest,
    )

    PROMETHEUS_AVAILABLE = True
except ImportError:
    PROMETHEUS_AVAILABLE = False

    # Define placeholder classes when prometheus is not available
    class CollectorRegistry:  # type: ignore[misc]
        """Placeholder registry when prometheus_client is not available."""

    class Counter:  # type: ignore[misc]
        """Placeholder counter when prometheus_client is not available."""

        def __init__(self, *args, **kwargs) -> None:
            """Initialize placeholder counter."""

        def inc(self, *args, **kwargs) -> None:
            """Increment placeholder counter."""

        def labels(self, *args, **kwargs):
            """Return labeled placeholder counter."""
            return self

    class Gauge:  # type: ignore[misc]
        """Placeholder gauge when prometheus_client is not available."""

        def __init__(self, *args, **kwargs) -> None:
            """Initialize placeholder gauge."""

        def set(self, *args, **kwargs) -> None:
            """Set placeholder gauge value."""

        def inc(self, *args, **kwargs) -> None:
            """Increment placeholder gauge."""

        def dec(self, *args, **kwargs) -> None:
            """Decrement placeholder gauge."""

        def labels(self, *args, **kwargs):
            """Return labeled placeholder gauge."""
            return self

    class Histogram:  # type: ignore[misc]
        """Placeholder histogram when prometheus_client is not available."""

        def __init__(self, *args, **kwargs) -> None:
            """Initialize placeholder histogram."""

        def observe(self, *args, **kwargs) -> None:
            """Observe value in placeholder histogram."""

        def labels(self, *args, **kwargs):
            """Return labeled placeholder histogram."""
            return self

    CONTENT_TYPE_LATEST = "text/plain"

    def generate_latest(*args, **kwargs) -> bytes:
        """Placeholder for generate_latest when prometheus_client is not available."""
        # Return mock metrics data
        return b"""# HELP fraiseql_graphql_queries_total Total GraphQL queries
# TYPE fraiseql_graphql_queries_total counter
fraiseql_graphql_queries_total 1
# HELP fraiseql_graphql_query_duration_seconds GraphQL query duration
# TYPE fraiseql_graphql_query_duration_seconds histogram
fraiseql_graphql_query_duration_seconds_sum 0.01
fraiseql_graphql_query_duration_seconds_count 1
"""


@dataclass
class MetricsConfig:
    """Configuration for metrics collection.

    Attributes:
        enabled: Whether metrics collection is enabled.
        namespace: Prefix for all metric names (default: "fraiseql").
        metrics_path: URL path for metrics endpoint (default: "/metrics").
        buckets: Histogram bucket boundaries for latency metrics.
        exclude_paths: Set of URL paths to exclude from HTTP metrics.
        labels: Additional labels to apply to all metrics.
    """

    enabled: bool = True
    namespace: str = "fraiseql"
    metrics_path: str = "/metrics"
    buckets: list[float] = dataclass_field(
        default_factory=lambda: [
            0.005,
            0.01,
            0.025,
            0.05,
            0.1,
            0.25,
            0.5,
            1,
            2.5,
            5,
            10,
        ],
    )
    exclude_paths: set[str] = dataclass_field(
        default_factory=lambda: {
            "/metrics",
            "/health",
            "/ready",
            "/startup",
        },
    )
    labels: dict[str, str] = dataclass_field(default_factory=dict)

    def __post_init__(self):
        """Validate configuration."""
        if not self.namespace:
            msg = "Namespace cannot be empty"
            raise ValueError(msg)

        # Ensure buckets are monotonic
        for i in range(1, len(self.buckets)):
            if self.buckets[i] <= self.buckets[i - 1]:
                msg = "Histogram buckets must be monotonically increasing"
                raise ValueError(msg)


# Export all imports for convenience
__all__ = [
    "CONTENT_TYPE_LATEST",
    "PROMETHEUS_AVAILABLE",
    "CollectorRegistry",
    "Counter",
    "Gauge",
    "Histogram",
    "MetricsConfig",
    "generate_latest",
]
