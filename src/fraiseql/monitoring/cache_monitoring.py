"""Cache monitoring and metrics integration for FraiseQL (Phase 19, Commit 3).

This module extends the caching layer with comprehensive monitoring, including:
- Cache hit/miss rate tracking
- Per-cache-type metrics
- Memory usage estimation
- Eviction rate monitoring
- Cache effectiveness metrics
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any

logger = logging.getLogger(__name__)


@dataclass
class CacheMetrics:
    """Detailed cache metrics for monitoring.

    Attributes:
        hits: Total number of cache hits
        misses: Total number of cache misses
        errors: Total number of cache errors
        evictions: Total number of cache evictions
        memory_bytes: Estimated memory usage in bytes
        avg_hit_latency_ms: Average latency for cache hits (milliseconds)
        avg_miss_latency_ms: Average latency for cache misses (milliseconds)
        effective_entries: Number of entries in cache
        ttl_expirations: Number of entries expired by TTL
    """

    hits: int = 0
    misses: int = 0
    errors: int = 0
    evictions: int = 0
    memory_bytes: int = 0
    avg_hit_latency_ms: float = 0.0
    avg_miss_latency_ms: float = 0.0
    effective_entries: int = 0
    ttl_expirations: int = 0

    @property
    def total_operations(self) -> int:
        """Total cache operations."""
        return self.hits + self.misses

    @property
    def hit_rate(self) -> float:
        """Cache hit rate as percentage (0-100)."""
        if self.total_operations == 0:
            return 0.0
        return (self.hits / self.total_operations) * 100

    @property
    def error_rate(self) -> float:
        """Error rate as percentage (0-100)."""
        if self.total_operations == 0:
            return 0.0
        return (self.errors / self.total_operations) * 100

    @property
    def bytes_per_entry(self) -> float:
        """Estimated bytes per cache entry."""
        if self.effective_entries == 0:
            return 0.0
        return self.memory_bytes / self.effective_entries

    def to_dict(self) -> dict[str, Any]:
        """Convert metrics to dictionary for serialization."""
        return {
            "hits": self.hits,
            "misses": self.misses,
            "errors": self.errors,
            "evictions": self.evictions,
            "memory_bytes": self.memory_bytes,
            "avg_hit_latency_ms": round(self.avg_hit_latency_ms, 2),
            "avg_miss_latency_ms": round(self.avg_miss_latency_ms, 2),
            "effective_entries": self.effective_entries,
            "ttl_expirations": self.ttl_expirations,
            "hit_rate_percent": round(self.hit_rate, 2),
            "error_rate_percent": round(self.error_rate, 2),
            "bytes_per_entry": round(self.bytes_per_entry, 2),
            "total_operations": self.total_operations,
        }


class CacheMonitor:
    """Monitor cache performance and collect detailed metrics.

    Integrates with FraiseQL's caching layer to track:
    - Hit/miss/error rates
    - Latency statistics
    - Memory usage
    - Eviction patterns
    - TTL expiration tracking
    """

    def __init__(self, cache_name: str = "default") -> None:
        """Initialize cache monitor.

        Args:
            cache_name: Name of cache being monitored (e.g., 'result_cache', 'query_cache')
        """
        self.cache_name = cache_name
        self.metrics = CacheMetrics()
        self._hit_latencies: list[float] = []
        self._miss_latencies: list[float] = []
        self._max_latency_history = 1000  # Keep last 1000 measurements

    def record_hit(self, latency_ms: float | None = None) -> None:
        """Record a cache hit.

        Args:
            latency_ms: Optional latency of cache hit in milliseconds
        """
        self.metrics.hits += 1
        if latency_ms is not None:
            self._hit_latencies.append(latency_ms)
            if len(self._hit_latencies) > self._max_latency_history:
                self._hit_latencies.pop(0)
            self.metrics.avg_hit_latency_ms = sum(self._hit_latencies) / len(self._hit_latencies)

    def record_miss(self, latency_ms: float | None = None) -> None:
        """Record a cache miss.

        Args:
            latency_ms: Optional latency of cache miss in milliseconds
        """
        self.metrics.misses += 1
        if latency_ms is not None:
            self._miss_latencies.append(latency_ms)
            if len(self._miss_latencies) > self._max_latency_history:
                self._miss_latencies.pop(0)
            self.metrics.avg_miss_latency_ms = sum(self._miss_latencies) / len(self._miss_latencies)

    def record_error(self) -> None:
        """Record a cache operation error."""
        self.metrics.errors += 1

    def record_eviction(self, count: int = 1) -> None:
        """Record cache evictions.

        Args:
            count: Number of entries evicted
        """
        self.metrics.evictions += count

    def record_ttl_expiration(self, count: int = 1) -> None:
        """Record TTL-based expirations.

        Args:
            count: Number of entries expired
        """
        self.metrics.ttl_expirations += count

    def set_memory_usage(self, bytes_used: int) -> None:
        """Set estimated cache memory usage.

        Args:
            bytes_used: Bytes used by cache
        """
        self.metrics.memory_bytes = bytes_used

    def set_effective_entries(self, count: int) -> None:
        """Set number of effective entries in cache.

        Args:
            count: Number of entries currently in cache
        """
        self.metrics.effective_entries = count

    def get_metrics(self) -> CacheMetrics:
        """Get current cache metrics.

        Returns:
            CacheMetrics object with current values
        """
        return self.metrics

    def reset(self) -> None:
        """Reset all metrics."""
        self.metrics = CacheMetrics()
        self._hit_latencies = []
        self._miss_latencies = []
        logger.debug(f"Reset metrics for cache: {self.cache_name}")


class CacheMonitoringIntegration:
    """Integration layer between ResultCache and metrics collection.

    Hooks into cache operations to record metrics for Prometheus/monitoring systems.
    """

    def __init__(self) -> None:
        """Initialize cache monitoring integration."""
        self._monitors: dict[str, CacheMonitor] = {}

    def get_monitor(self, cache_name: str) -> CacheMonitor:
        """Get or create monitor for cache.

        Args:
            cache_name: Name of cache

        Returns:
            CacheMonitor for specified cache
        """
        if cache_name not in self._monitors:
            self._monitors[cache_name] = CacheMonitor(cache_name)
        return self._monitors[cache_name]

    def record_cache_operation(
        self,
        cache_name: str,
        operation_type: str,
        success: bool = True,
        latency_ms: float | None = None,
    ) -> None:
        """Record a cache operation.

        Args:
            cache_name: Name of cache
            operation_type: Type of operation ('hit', 'miss', 'error')
            success: Whether operation was successful
            latency_ms: Optional latency in milliseconds
        """
        monitor = self.get_monitor(cache_name)

        if operation_type == "hit":
            monitor.record_hit(latency_ms)
        elif operation_type == "miss":
            monitor.record_miss(latency_ms)
        elif operation_type == "error":
            monitor.record_error()

    def get_all_metrics(self) -> dict[str, CacheMetrics]:
        """Get metrics for all monitored caches.

        Returns:
            Dictionary of cache_name -> CacheMetrics
        """
        return {name: monitor.get_metrics() for name, monitor in self._monitors.items()}

    def get_metrics_dict(self) -> dict[str, dict[str, Any]]:
        """Get metrics as dictionaries (for JSON serialization).

        Returns:
            Dictionary of cache_name -> metrics dict
        """
        return {name: metrics.to_dict() for name, metrics in self.get_all_metrics().items()}

    def reset_all(self) -> None:
        """Reset metrics for all caches."""
        for monitor in self._monitors.values():
            monitor.reset()
        logger.info("Reset all cache metrics")


# Global monitoring instance
_cache_monitoring: CacheMonitoringIntegration | None = None


def get_cache_monitoring() -> CacheMonitoringIntegration:
    """Get global cache monitoring instance.

    Returns:
        Global CacheMonitoringIntegration instance
    """
    global _cache_monitoring
    if _cache_monitoring is None:
        _cache_monitoring = CacheMonitoringIntegration()
    return _cache_monitoring


def set_cache_monitoring(monitoring: CacheMonitoringIntegration) -> None:
    """Set global cache monitoring instance.

    Args:
        monitoring: CacheMonitoringIntegration instance to use globally
    """
    global _cache_monitoring
    _cache_monitoring = monitoring


def integrate_cache_metrics(result_cache: Any, cache_name: str = "default") -> None:
    """Integrate ResultCache with metrics monitoring.

    Attaches monitoring hooks to existing ResultCache instance.

    Args:
        result_cache: ResultCache instance to instrument
        cache_name: Name for monitoring (default: "default")
    """
    monitoring = get_cache_monitoring()
    monitor = monitoring.get_monitor(cache_name)

    # Store original get_or_set method
    original_get_or_set = result_cache.get_or_set

    async def monitored_get_or_set(key: str, func: Any, ttl: int | None = None) -> Any:
        """Instrumented get_or_set that records metrics."""
        try:
            return await original_get_or_set(key, func, ttl)
            # Metrics tracking happens via the wrapped get_stats method
        except Exception as e:
            monitor.record_error()
            logger.warning(f"Cache error in {cache_name}: {e}")
            raise

    # Replace method
    result_cache.get_or_set = monitored_get_or_set

    # Also wrap the cache stats getters
    original_get_stats = result_cache.get_stats

    def get_stats_with_monitoring() -> Any:
        """Get stats and update monitoring."""
        cache_stats = original_get_stats()

        # Sync cache stats to our monitor
        monitor.metrics.hits = cache_stats.hits
        monitor.metrics.misses = cache_stats.misses
        monitor.metrics.errors = cache_stats.errors
        monitor.set_effective_entries(cache_stats.total)

        return cache_stats

    result_cache.get_stats = get_stats_with_monitoring

    logger.info(f"Integrated cache metrics monitoring for: {cache_name}")
