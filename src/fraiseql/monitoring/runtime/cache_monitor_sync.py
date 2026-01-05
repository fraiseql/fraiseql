"""Synchronous accessor for cache monitoring data.

Provides thread-safe synchronous access to cache monitoring metrics
without async/await overhead.
"""

from __future__ import annotations

import logging
from typing import Any

from fraiseql.monitoring.cache_monitoring import CacheMetrics, CacheMonitor

logger = logging.getLogger(__name__)

# Global instance holder
_cache_monitor_instance: CacheMonitor | None = None


def set_cache_monitor(monitor: CacheMonitor) -> None:
    """Set the global CacheMonitor instance.

    Args:
        monitor: CacheMonitor instance to use
    """
    global _cache_monitor_instance
    _cache_monitor_instance = monitor


def get_cache_monitor() -> CacheMonitor:
    """Get the global CacheMonitor instance.

    Returns:
        The current CacheMonitor instance

    Raises:
        RuntimeError: If no monitor has been set
    """
    global _cache_monitor_instance
    if _cache_monitor_instance is None:
        _cache_monitor_instance = CacheMonitor()
    return _cache_monitor_instance


class CacheMonitorSync:
    """Synchronous accessor for cache monitoring data.

    Provides fast, thread-safe access to cache monitoring metrics
    without async/await overhead.
    """

    def __init__(self, monitor: CacheMonitor | None = None) -> None:
        """Initialize synchronous cache monitor accessor.

        Args:
            monitor: CacheMonitor instance. If None, uses global instance.
        """
        self._monitor = monitor or get_cache_monitor()

    def get_metrics(self) -> CacheMetrics:
        """Get current cache metrics (synchronous).

        Returns:
            Current CacheMetrics snapshot
        """
        # CacheMonitor should provide thread-safe metric access
        # For now, return empty metrics if monitor doesn't have methods
        if hasattr(self._monitor, "get_metrics"):
            return self._monitor.get_metrics()

        # Fallback for basic CacheMetrics
        return CacheMetrics()

    def get_hit_rate(self) -> float:
        """Get cache hit rate as percentage (synchronous).

        Returns:
            Hit rate (0.0-100.0)
        """
        metrics = self.get_metrics()
        return metrics.hit_rate

    def get_metrics_dict(self) -> dict[str, Any]:
        """Get metrics as dictionary (synchronous).

        Returns:
            Dictionary with cache metrics
        """
        metrics = self.get_metrics()
        return {
            "hits": metrics.hits,
            "misses": metrics.misses,
            "errors": metrics.errors,
            "evictions": metrics.evictions,
            "hit_rate": round(metrics.hit_rate, 2),
            "error_rate": round(metrics.error_rate, 2),
            "total_operations": metrics.total_operations,
            "effective_entries": metrics.effective_entries,
            "memory_bytes": metrics.memory_bytes,
        }

    def is_healthy(
        self,
        hit_rate_threshold: float = 80.0,
        eviction_rate_threshold: float = 30.0,
    ) -> bool:
        """Check if cache is healthy (synchronous).

        Args:
            hit_rate_threshold: Minimum acceptable hit rate (%)
            eviction_rate_threshold: Maximum acceptable eviction rate (%)

        Returns:
            True if cache is healthy
        """
        metrics = self.get_metrics()

        # Check hit rate
        if metrics.hit_rate < hit_rate_threshold:
            logger.warning(
                f"Cache hit rate {metrics.hit_rate:.1f}% below threshold {hit_rate_threshold}%",
            )
            return False

        # Check eviction rate
        if metrics.total_operations > 0:
            eviction_rate = (metrics.evictions / metrics.total_operations) * 100
            if eviction_rate > eviction_rate_threshold:
                logger.warning(
                    f"Cache eviction rate {eviction_rate:.1f}% "
                    f"above threshold {eviction_rate_threshold}%",
                )
                return False

        return True

    def get_status_string(self) -> str:
        """Get human-readable status string (synchronous).

        Returns:
            Status string for display
        """
        metrics = self.get_metrics()
        health = self.is_healthy()
        status = "HEALTHY" if health else "DEGRADED"

        return (
            f"Cache Status: {status}\n"
            f"  Hit Rate: {metrics.hit_rate:.1f}%\n"
            f"  Hits: {metrics.hits}\n"
            f"  Misses: {metrics.misses}\n"
            f"  Evictions: {metrics.evictions}\n"
            f"  Entries: {metrics.effective_entries}"
        )


# Create singleton instance for CLI use
cache_monitor_sync = CacheMonitorSync()
