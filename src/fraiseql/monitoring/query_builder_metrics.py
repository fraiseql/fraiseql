"""Prometheus metrics for query builder monitoring.

Phase 7 - Production Integration Monitoring
"""

try:
    from prometheus_client import Counter, Gauge, Histogram

    PROMETHEUS_AVAILABLE = True
except ImportError:
    # Prometheus client not available - use no-op metrics
    PROMETHEUS_AVAILABLE = False


# Query builder usage counters
if PROMETHEUS_AVAILABLE:
    query_builder_calls = Counter(
        "fraiseql_query_builder_calls_total",
        "Total query builder calls",
        ["builder_type"],  # 'rust' or 'python'
    )

    query_builder_errors = Counter(
        "fraiseql_query_builder_errors_total",
        "Query builder errors",
        ["builder_type"],
    )

    query_builder_fallbacks = Counter(
        "fraiseql_query_builder_fallbacks_total",
        "Rust to Python fallbacks",
    )

    # Query build duration histogram
    query_build_duration = Histogram(
        "fraiseql_query_build_duration_seconds",
        "Query build duration in seconds",
        ["builder_type"],
        buckets=(
            0.00005,
            0.0001,
            0.00025,
            0.0005,
            0.001,
            0.0025,
            0.005,
            0.01,
            0.025,
            0.05,
            0.1,
        ),  # 50μs to 100ms
    )

    # Current query builder mode
    query_builder_mode = Gauge(
        "fraiseql_query_builder_mode",
        "Current query builder mode (0=Python, 1=Rust)",
    )

    # Rust availability
    rust_available = Gauge(
        "fraiseql_rust_query_builder_available",
        "Whether Rust query builder is available",
    )


def record_query_build(builder_type: str, duration: float) -> None:
    """Record a query build operation.

    Args:
        builder_type: 'rust' or 'python'
        duration: Build duration in seconds
    """
    if PROMETHEUS_AVAILABLE:
        query_builder_calls.labels(builder_type=builder_type).inc()
        query_build_duration.labels(builder_type=builder_type).observe(duration)


def record_query_build_error(builder_type: str) -> None:
    """Record a query build error.

    Args:
        builder_type: 'rust' or 'python'
    """
    if PROMETHEUS_AVAILABLE:
        query_builder_errors.labels(builder_type=builder_type).inc()


def record_fallback() -> None:
    """Record a fallback from Rust to Python."""
    if PROMETHEUS_AVAILABLE:
        query_builder_fallbacks.inc()


def set_query_builder_mode(use_rust: bool) -> None:
    """Set the current query builder mode gauge.

    Args:
        use_rust: True if Rust is being used, False for Python
    """
    if PROMETHEUS_AVAILABLE:
        query_builder_mode.set(1 if use_rust else 0)


def set_rust_availability(available: bool) -> None:
    """Set the Rust availability gauge.

    Args:
        available: True if Rust query builder is available
    """
    if PROMETHEUS_AVAILABLE:
        rust_available.set(1 if available else 0)
