"""Prometheus metrics integration for FraiseQL.

This module re-exports the metrics functionality from the metrics subpackage
for backward compatibility.
"""

# Re-export everything from the new structure
from .metrics import (
    PROMETHEUS_AVAILABLE,
    FraiseQLMetrics,
    MetricsConfig,
    MetricsMiddleware,
    get_metrics,
    setup_metrics,
    with_metrics,
)

__all__ = [
    "PROMETHEUS_AVAILABLE",
    "FraiseQLMetrics",
    "MetricsConfig",
    "MetricsMiddleware",
    "get_metrics",
    "setup_metrics",
    "with_metrics",
]
