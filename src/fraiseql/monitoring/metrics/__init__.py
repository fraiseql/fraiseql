"""FraiseQL metrics collection package.

This package provides comprehensive metrics collection for monitoring
FraiseQL applications in production using Prometheus.
"""

from .collectors import FraiseQLMetrics
from .config import PROMETHEUS_AVAILABLE, MetricsConfig
from .integration import MetricsMiddleware, get_metrics, setup_metrics, with_metrics

__all__ = [
    "PROMETHEUS_AVAILABLE",
    "FraiseQLMetrics",
    "MetricsConfig",
    "MetricsMiddleware",
    "get_metrics",
    "setup_metrics",
    "with_metrics",
]
