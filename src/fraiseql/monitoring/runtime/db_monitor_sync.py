"""Synchronous accessor for database monitoring data.

Provides thread-safe synchronous access to database monitoring metrics
without async/await overhead. Uses existing DatabaseMonitor locks for
thread safety.

Designed specifically for CLI commands and synchronous contexts.
"""

from __future__ import annotations

import logging
from typing import Any, Optional

from fraiseql.monitoring.db_monitor import (
    DatabaseMonitor,
    PoolMetrics,
    QueryMetrics,
    QueryStatistics,
)

logger = logging.getLogger(__name__)

# Global instance holders
_db_monitor_instance: Optional[DatabaseMonitor] = None
_db_monitor_sync_instance: Optional[DatabaseMonitorSync] = None


def set_database_monitor(monitor: DatabaseMonitor) -> None:
    """Set the global DatabaseMonitor instance.

    Args:
        monitor: DatabaseMonitor instance to use
    """
    global _db_monitor_instance, _db_monitor_sync_instance
    _db_monitor_instance = monitor
    # Reset sync wrapper so it uses the new monitor
    _db_monitor_sync_instance = None


def get_database_monitor() -> DatabaseMonitor:
    """Get the global DatabaseMonitor instance.

    Returns:
        The current DatabaseMonitor instance

    Raises:
        RuntimeError: If no monitor has been set
    """
    global _db_monitor_instance
    if _db_monitor_instance is None:
        # Create a default instance if none exists
        _db_monitor_instance = DatabaseMonitor()
    return _db_monitor_instance


def get_database_monitor_sync() -> DatabaseMonitorSync:
    """Get the global DatabaseMonitor via sync accessor.

    Returns synchronous wrapper for thread-safe access to monitoring metrics.

    Returns:
        DatabaseMonitorSync instance for synchronous access

    Raises:
        RuntimeError: If no monitor has been set
    """
    global _db_monitor_instance, _db_monitor_sync_instance
    if _db_monitor_instance is None:
        # Create a default instance if none exists
        _db_monitor_instance = DatabaseMonitor()
    if _db_monitor_sync_instance is None:
        _db_monitor_sync_instance = DatabaseMonitorSync(monitor=_db_monitor_instance)
    return _db_monitor_sync_instance


class DatabaseMonitorSync:
    """Synchronous accessor for database monitoring data.

    Provides fast, thread-safe access to database monitoring metrics
    without async/await overhead. Uses existing DatabaseMonitor
    thread-safe locks.

    Thread-safe: All methods use the monitor's internal lock for
    synchronization.

    Performance: All operations are CPU-bound (in-memory deque operations)
    and return in microseconds.
    """

    def __init__(self, monitor: Optional[DatabaseMonitor] = None) -> None:
        """Initialize synchronous database monitor accessor.

        Args:
            monitor: DatabaseMonitor instance. If None, uses global instance.
        """
        self._monitor = monitor or get_database_monitor()

    def get_recent_queries(self, limit: int = 20) -> list[QueryMetrics]:
        """Get recent database queries (synchronous).

        Returns queries in reverse chronological order (newest first).

        Args:
            limit: Maximum number of queries to return

        Returns:
            List of recent QueryMetrics, newest first
        """
        with self._monitor._lock:
            return list(self._monitor._recent_queries)[-limit:][::-1]

    def get_slow_queries(self, limit: int = 50) -> list[QueryMetrics]:
        """Get slowest database queries (synchronous).

        Returns queries sorted by duration (slowest first).

        Args:
            limit: Maximum number of queries to return

        Returns:
            List of slow QueryMetrics, slowest first
        """
        with self._monitor._lock:
            slow = list(self._monitor._slow_queries)
            # Sort by duration, slowest first
            slow.sort(key=lambda q: q.duration_ms, reverse=True)
            return slow[:limit]

    def get_queries_by_type(self) -> dict[str, int]:
        """Get query count by type (synchronous).

        Returns count of queries grouped by type (SELECT, INSERT, etc).

        Returns:
            Dict mapping query type to count
        """
        with self._monitor._lock:
            counts: dict[str, int] = {}
            for query in self._monitor._recent_queries:
                counts[query.query_type] = counts.get(query.query_type, 0) + 1
            return counts

    def get_pool_metrics(self) -> Optional[PoolMetrics]:
        """Get current connection pool metrics (synchronous).

        Returns:
            Current PoolMetrics or None if no data available
        """
        with self._monitor._lock:
            if self._monitor._pool_states:
                return self._monitor._pool_states[-1]
            return None

    def get_pool_history(self, limit: int = 100) -> list[PoolMetrics]:
        """Get connection pool state history (synchronous).

        Returns pool states in reverse chronological order (newest first).

        Args:
            limit: Maximum states to return

        Returns:
            List of PoolMetrics, newest first
        """
        with self._monitor._lock:
            return list(self._monitor._pool_states)[-limit:][::-1]

    def get_statistics(self) -> QueryStatistics:
        """Get aggregate query statistics (synchronous).

        Computes statistics from current queries in memory.

        Returns:
            QueryStatistics with aggregated metrics
        """
        with self._monitor._lock:
            queries = list(self._monitor._recent_queries)

        stats = QueryStatistics()

        if not queries:
            return stats

        # Basic counts
        stats.total_count = len(queries)
        stats.success_count = sum(1 for q in queries if q.is_success())
        stats.error_count = stats.total_count - stats.success_count

        if stats.total_count > 0:
            stats.success_rate = stats.success_count / stats.total_count

        # Duration statistics
        if queries:
            durations = [q.duration_ms for q in queries]
            stats.total_duration_ms = sum(durations)
            stats.avg_duration_ms = stats.total_duration_ms / len(durations)
            stats.min_duration_ms = min(durations)
            stats.max_duration_ms = max(durations)

            # Percentiles
            sorted_durations = sorted(durations)
            idx_50 = int(len(sorted_durations) * 0.50) - 1
            idx_95 = int(len(sorted_durations) * 0.95) - 1
            idx_99 = int(len(sorted_durations) * 0.99) - 1

            stats.p50_duration_ms = sorted_durations[max(0, idx_50)]
            stats.p95_duration_ms = sorted_durations[max(0, idx_95)]
            stats.p99_duration_ms = sorted_durations[max(0, idx_99)]

        # Slow query stats
        with self._monitor._lock:
            slow = list(self._monitor._slow_queries)
        stats.slow_count = len(slow)
        if stats.total_count > 0:
            stats.slow_rate = stats.slow_count / stats.total_count

        return stats

    def get_query_statistics(self) -> QueryStatistics:
        """Alias for get_statistics for API compatibility.

        Returns:
            QueryStatistics with aggregated metrics
        """
        return self.get_statistics()

    def get_query_count(self) -> int:
        """Get total number of queries tracked (synchronous).

        Returns:
            Total count of queries
        """
        with self._monitor._lock:
            return len(self._monitor._recent_queries)

    def get_slow_query_count(self) -> int:
        """Get total number of slow queries (synchronous).

        Returns:
            Count of queries marked as slow
        """
        with self._monitor._lock:
            return len(self._monitor._slow_queries)

    def get_last_query(self) -> Optional[QueryMetrics]:
        """Get the last recorded query (synchronous).

        Returns:
            Last QueryMetrics or None if no queries recorded
        """
        with self._monitor._lock:
            if self._monitor._recent_queries:
                return self._monitor._recent_queries[-1]
            return None

    def to_dict(self) -> dict[str, Any]:
        """Convert monitor state to dictionary (synchronous).

        Returns:
            Dictionary with monitor data
        """
        return {
            "query_count": self.get_query_count(),
            "slow_query_count": self.get_slow_query_count(),
            "pool_metrics": (
                self.get_pool_metrics().to_dict() if self.get_pool_metrics() else None
            ),
            "statistics": {
                "total": self.get_statistics().total_count,
                "success_rate": self.get_statistics().success_rate,
                "avg_duration_ms": self.get_statistics().avg_duration_ms,
                "p95_duration_ms": self.get_statistics().p95_duration_ms,
                "p99_duration_ms": self.get_statistics().p99_duration_ms,
            },
            "queries_by_type": self.get_queries_by_type(),
        }


# Create singleton instance for CLI use
db_monitor_sync = DatabaseMonitorSync()
