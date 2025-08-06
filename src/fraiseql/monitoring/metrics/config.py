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

        def __init__(self) -> None:
            """Initialize placeholder registry."""
            self._metrics: dict[str, object] = {}

        def register(self, metric) -> None:
            """Register a metric."""
            if hasattr(metric, "_name"):
                self._metrics[metric._name] = metric

        def collect(self):
            """Collect metrics for iteration."""
            return []

    class Counter:  # type: ignore[misc]
        """Placeholder counter when prometheus_client is not available."""

        def __init__(self, name: str, documentation: str, labelnames=None, registry=None) -> None:
            """Initialize placeholder counter."""
            self._name = name
            self._value = 0.0
            self._labeled_instances: dict[tuple, Counter] = {}
            if registry:
                registry.register(self)

        def inc(self, amount: float = 1.0) -> None:
            """Increment placeholder counter."""
            self._value += amount

        def labels(self, *args, **kwargs):
            """Return labeled placeholder counter."""
            # Create label key from args and kwargs
            label_key = tuple(args) + tuple(sorted(kwargs.items()))
            if label_key not in self._labeled_instances:
                labeled_counter = Counter(f"{self._name}_{hash(label_key)}", "labeled counter")
                self._labeled_instances[label_key] = labeled_counter
            return self._labeled_instances[label_key]

        def get_value(self) -> float:
            """Get current counter value."""
            return self._value

    class Gauge:  # type: ignore[misc]
        """Placeholder gauge when prometheus_client is not available."""

        def __init__(self, name: str, documentation: str, labelnames=None, registry=None) -> None:
            """Initialize placeholder gauge."""
            self._name = name
            self._value = 0.0
            self._labeled_instances: dict[tuple, Gauge] = {}
            if registry:
                registry.register(self)

        def set(self, value: float) -> None:
            """Set placeholder gauge value."""
            self._value = value

        def inc(self, amount: float = 1.0) -> None:
            """Increment placeholder gauge."""
            self._value += amount

        def dec(self, amount: float = 1.0) -> None:
            """Decrement placeholder gauge."""
            self._value -= amount

        def labels(self, *args, **kwargs):
            """Return labeled placeholder gauge."""
            label_key = tuple(args) + tuple(sorted(kwargs.items()))
            if label_key not in self._labeled_instances:
                labeled_gauge = Gauge(f"{self._name}_{hash(label_key)}", "labeled gauge")
                self._labeled_instances[label_key] = labeled_gauge
            return self._labeled_instances[label_key]

        def get_value(self) -> float:
            """Get current gauge value."""
            return self._value

    class Histogram:  # type: ignore[misc]
        """Placeholder histogram when prometheus_client is not available."""

        def __init__(
            self, name: str, documentation: str, labelnames=None, buckets=None, registry=None
        ) -> None:
            """Initialize placeholder histogram."""
            self._name = name
            self._sum = 0.0
            self._count = 0.0
            self._labeled_instances: dict[tuple, Histogram] = {}
            if registry:
                registry.register(self)

        def observe(self, amount: float) -> None:
            """Observe value in placeholder histogram."""
            self._sum += amount
            self._count += 1

        def labels(self, *args, **kwargs):
            """Return labeled placeholder histogram."""
            label_key = tuple(args) + tuple(sorted(kwargs.items()))
            if label_key not in self._labeled_instances:
                labeled_histogram = Histogram(
                    f"{self._name}_{hash(label_key)}", "labeled histogram"
                )
                self._labeled_instances[label_key] = labeled_histogram
            return self._labeled_instances[label_key]

        def get_sum(self) -> float:
            """Get histogram sum."""
            return self._sum

        def get_count(self) -> float:
            """Get histogram count."""
            return self._count

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
