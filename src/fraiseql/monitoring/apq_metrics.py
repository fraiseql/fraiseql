"""APQ Performance Metrics Tracking for FraiseQL.

This module provides comprehensive metrics tracking for the APQ system,
enabling monitoring of cache hit rates, query patterns, and performance optimization.
"""

import logging
from dataclasses import dataclass
from datetime import UTC, datetime
from threading import Lock
from typing import Any, Optional

logger = logging.getLogger(__name__)


@dataclass
class APQMetricsSnapshot:
    """Immutable snapshot of APQ metrics at a point in time."""

    # Timestamp
    timestamp: datetime

    # Query cache metrics
    query_cache_hits: int
    query_cache_misses: int
    query_cache_stores: int

    # Response cache metrics
    response_cache_hits: int
    response_cache_misses: int
    response_cache_stores: int

    # Storage statistics
    stored_queries_count: int
    cached_responses_count: int
    total_storage_bytes: int

    # Performance metrics
    total_requests: int
    avg_query_parse_time_ms: Optional[float] = None

    # Derived metrics
    @property
    def query_cache_hit_rate(self) -> float:
        """Calculate query cache hit rate (0.0 to 1.0)."""
        total = self.query_cache_hits + self.query_cache_misses
        return self.query_cache_hits / total if total > 0 else 0.0

    @property
    def response_cache_hit_rate(self) -> float:
        """Calculate response cache hit rate (0.0 to 1.0)."""
        total = self.response_cache_hits + self.response_cache_misses
        return self.response_cache_hits / total if total > 0 else 0.0

    @property
    def overall_hit_rate(self) -> float:
        """Calculate overall cache hit rate across both layers."""
        total_hits = self.query_cache_hits + self.response_cache_hits
        total_requests = self.total_requests
        return total_hits / total_requests if total_requests > 0 else 0.0

    def to_dict(self) -> dict[str, Any]:
        """Convert metrics to dictionary format."""
        return {
            "timestamp": self.timestamp.isoformat(),
            "query_cache": {
                "hits": self.query_cache_hits,
                "misses": self.query_cache_misses,
                "stores": self.query_cache_stores,
                "hit_rate": round(self.query_cache_hit_rate, 4),
            },
            "response_cache": {
                "hits": self.response_cache_hits,
                "misses": self.response_cache_misses,
                "stores": self.response_cache_stores,
                "hit_rate": round(self.response_cache_hit_rate, 4),
            },
            "storage": {
                "stored_queries": self.stored_queries_count,
                "cached_responses": self.cached_responses_count,
                "total_bytes": self.total_storage_bytes,
            },
            "performance": {
                "total_requests": self.total_requests,
                "overall_hit_rate": round(self.overall_hit_rate, 4),
                "avg_parse_time_ms": self.avg_query_parse_time_ms,
            },
        }


@dataclass
class QueryPattern:
    """Track usage pattern for a specific query hash."""

    hash_value: str
    first_seen: datetime
    last_seen: datetime
    hit_count: int = 0
    miss_count: int = 0
    total_parse_time_ms: float = 0.0

    @property
    def avg_parse_time_ms(self) -> float:
        """Average parse time for this query."""
        return self.total_parse_time_ms / self.hit_count if self.hit_count > 0 else 0.0


class APQMetrics:
    """Thread-safe APQ metrics tracker.

    This class tracks comprehensive metrics for the APQ system:
    - Query cache hit/miss rates
    - Response cache hit/miss rates
    - Storage statistics
    - Query patterns and performance
    - Historical trends

    Example Usage:
        ```python
        metrics = APQMetrics()

        # Record query cache hit
        metrics.record_query_cache_hit(query_hash)

        # Record response cache miss
        metrics.record_response_cache_miss(query_hash)

        # Get current snapshot
        snapshot = metrics.get_snapshot()
        print(f"Query cache hit rate: {snapshot.query_cache_hit_rate:.1%}")
        ```
    """

    def __init__(self) -> None:
        """Initialize APQ metrics tracker."""
        self._lock = Lock()

        # Query cache counters
        self._query_cache_hits = 0
        self._query_cache_misses = 0
        self._query_cache_stores = 0

        # Response cache counters
        self._response_cache_hits = 0
        self._response_cache_misses = 0
        self._response_cache_stores = 0

        # Storage statistics (updated periodically)
        self._stored_queries_count = 0
        self._cached_responses_count = 0
        self._total_storage_bytes = 0

        # Performance tracking
        self._total_requests = 0
        self._total_parse_time_ms = 0.0
        self._parse_time_samples = 0

        # Query patterns (top N queries)
        self._query_patterns: dict[str, QueryPattern] = {}
        self._max_tracked_queries = 100

        # Historical snapshots (last 100)
        self._snapshots: list[APQMetricsSnapshot] = []
        self._max_snapshots = 100

        logger.info("APQ metrics tracker initialized")

    # === Query Cache Metrics ===

    def record_query_cache_hit(self, query_hash: str) -> None:
        """Record a query cache hit."""
        with self._lock:
            self._query_cache_hits += 1
            self._total_requests += 1
            self._update_query_pattern(query_hash, cache_hit=True)

    def record_query_cache_miss(self, query_hash: str) -> None:
        """Record a query cache miss."""
        with self._lock:
            self._query_cache_misses += 1
            self._total_requests += 1
            self._update_query_pattern(query_hash, cache_hit=False)

    def record_query_cache_store(self, query_hash: str) -> None:
        """Record storing a query in cache."""
        with self._lock:
            self._query_cache_stores += 1

    # === Response Cache Metrics ===

    def record_response_cache_hit(self, query_hash: str) -> None:
        """Record a response cache hit (fastest path!)."""
        with self._lock:
            self._response_cache_hits += 1
            self._total_requests += 1
            # Response cache hit means we skipped parsing entirely
            self._update_query_pattern(query_hash, cache_hit=True, parse_time_ms=0.0)

    def record_response_cache_miss(self, query_hash: str) -> None:
        """Record a response cache miss."""
        with self._lock:
            self._response_cache_misses += 1
            # Don't increment total_requests - will be counted by query cache

    def record_response_cache_store(self, query_hash: str) -> None:
        """Record storing a response in cache."""
        with self._lock:
            self._response_cache_stores += 1

    # === Performance Metrics ===

    def record_query_parse_time(self, query_hash: str, parse_time_ms: float) -> None:
        """Record query parsing time.

        Args:
            query_hash: SHA256 hash of the query
            parse_time_ms: Time taken to parse query in milliseconds
        """
        with self._lock:
            self._total_parse_time_ms += parse_time_ms
            self._parse_time_samples += 1
            self._update_query_pattern(query_hash, parse_time_ms=parse_time_ms)

    # === Storage Statistics ===

    def update_storage_stats(
        self, stored_queries: int, cached_responses: int, total_bytes: int
    ) -> None:
        """Update storage statistics (called periodically).

        Args:
            stored_queries: Number of queries in storage
            cached_responses: Number of cached responses
            total_bytes: Total storage size in bytes
        """
        with self._lock:
            self._stored_queries_count = stored_queries
            self._cached_responses_count = cached_responses
            self._total_storage_bytes = total_bytes

    # === Query Patterns ===

    def _update_query_pattern(
        self, query_hash: str, cache_hit: bool = False, parse_time_ms: Optional[float] = None
    ) -> None:
        """Update query pattern statistics (called within lock)."""
        if query_hash not in self._query_patterns:
            # Only track top N queries
            if len(self._query_patterns) >= self._max_tracked_queries:
                # Evict least recently used query
                lru_hash = min(
                    self._query_patterns.keys(), key=lambda h: self._query_patterns[h].last_seen
                )
                del self._query_patterns[lru_hash]

            self._query_patterns[query_hash] = QueryPattern(
                hash_value=query_hash,
                first_seen=datetime.now(UTC),
                last_seen=datetime.now(UTC),
            )

        pattern = self._query_patterns[query_hash]
        pattern.last_seen = datetime.now(UTC)

        if cache_hit:
            pattern.hit_count += 1
        else:
            pattern.miss_count += 1

        if parse_time_ms is not None:
            pattern.total_parse_time_ms += parse_time_ms

    def get_top_queries(self, limit: int = 10) -> list[dict[str, Any]]:
        """Get top N queries by usage frequency.

        Args:
            limit: Number of top queries to return

        Returns:
            List of query pattern dictionaries sorted by total requests
        """
        with self._lock:
            sorted_patterns = sorted(
                self._query_patterns.values(),
                key=lambda p: p.hit_count + p.miss_count,
                reverse=True,
            )[:limit]

            return [
                {
                    "hash": p.hash_value[:16] + "...",
                    "total_requests": p.hit_count + p.miss_count,
                    "hit_count": p.hit_count,
                    "miss_count": p.miss_count,
                    "avg_parse_time_ms": round(p.avg_parse_time_ms, 2),
                    "first_seen": p.first_seen.isoformat(),
                    "last_seen": p.last_seen.isoformat(),
                }
                for p in sorted_patterns
            ]

    # === Snapshots ===

    def get_snapshot(self) -> APQMetricsSnapshot:
        """Get current metrics snapshot.

        Returns:
            Immutable snapshot of current metrics
        """
        with self._lock:
            avg_parse_time = (
                self._total_parse_time_ms / self._parse_time_samples
                if self._parse_time_samples > 0
                else None
            )

            snapshot = APQMetricsSnapshot(
                timestamp=datetime.now(UTC),
                query_cache_hits=self._query_cache_hits,
                query_cache_misses=self._query_cache_misses,
                query_cache_stores=self._query_cache_stores,
                response_cache_hits=self._response_cache_hits,
                response_cache_misses=self._response_cache_misses,
                response_cache_stores=self._response_cache_stores,
                stored_queries_count=self._stored_queries_count,
                cached_responses_count=self._cached_responses_count,
                total_storage_bytes=self._total_storage_bytes,
                total_requests=self._total_requests,
                avg_query_parse_time_ms=avg_parse_time,
            )

            # Store snapshot for historical tracking
            self._snapshots.append(snapshot)
            if len(self._snapshots) > self._max_snapshots:
                self._snapshots.pop(0)

            return snapshot

    def get_historical_snapshots(self, limit: int = 10) -> list[APQMetricsSnapshot]:
        """Get recent historical snapshots.

        Args:
            limit: Number of recent snapshots to return

        Returns:
            List of historical snapshots (most recent first)
        """
        with self._lock:
            return list(reversed(self._snapshots[-limit:]))

    # === Reset & Export ===

    def reset(self) -> None:
        """Reset all metrics to zero.

        Useful for testing or when starting a new monitoring period.
        """
        with self._lock:
            self._query_cache_hits = 0
            self._query_cache_misses = 0
            self._query_cache_stores = 0
            self._response_cache_hits = 0
            self._response_cache_misses = 0
            self._response_cache_stores = 0
            self._total_requests = 0
            self._total_parse_time_ms = 0.0
            self._parse_time_samples = 0
            self._query_patterns.clear()
            self._snapshots.clear()

        logger.info("APQ metrics reset")

    def export_metrics(self) -> dict[str, Any]:
        """Export comprehensive metrics for monitoring/alerting.

        Returns:
            Dictionary with all metrics in monitoring-friendly format
        """
        snapshot = self.get_snapshot()
        top_queries = self.get_top_queries(limit=10)

        return {
            "current": snapshot.to_dict(),
            "top_queries": top_queries,
            "health": {
                "status": self._assess_health(snapshot),
                "warnings": self._get_warnings(snapshot),
            },
        }

    def _assess_health(self, snapshot: APQMetricsSnapshot) -> str:
        """Assess overall APQ system health.

        Args:
            snapshot: Current metrics snapshot

        Returns:
            Health status: 'healthy', 'warning', or 'critical'
        """
        query_hit_rate = snapshot.query_cache_hit_rate
        response_hit_rate = snapshot.response_cache_hit_rate

        # Critical: Query cache hit rate < 50%
        if query_hit_rate < 0.5 and snapshot.total_requests > 100:
            return "critical"

        # Warning: Query cache hit rate < 70%
        if query_hit_rate < 0.7 and snapshot.total_requests > 100:
            return "warning"

        # Warning: Response cache enabled but hit rate < 50%
        if snapshot.response_cache_stores > 0 and response_hit_rate < 0.5:
            return "warning"

        return "healthy"

    def _get_warnings(self, snapshot: APQMetricsSnapshot) -> list[str]:
        """Get list of current warnings.

        Args:
            snapshot: Current metrics snapshot

        Returns:
            List of warning messages
        """
        warnings = []

        if snapshot.query_cache_hit_rate < 0.7 and snapshot.total_requests > 100:
            warnings.append(
                f"Low query cache hit rate: {snapshot.query_cache_hit_rate:.1%} (target: >70%)"
            )

        if snapshot.response_cache_hit_rate < 0.5 and snapshot.response_cache_stores > 10:
            warnings.append(
                f"Low response cache hit rate: {snapshot.response_cache_hit_rate:.1%} "
                "(target: >50%)"
            )

        if snapshot.total_storage_bytes > 100 * 1024 * 1024:  # 100MB
            warnings.append(
                f"High storage usage: {snapshot.total_storage_bytes / (1024 * 1024):.1f}MB "
                "(consider TTL or eviction)"
            )

        return warnings


# Global metrics instance for convenience
_global_metrics: Optional[APQMetrics] = None


def get_global_metrics() -> APQMetrics:
    """Get or create the global APQ metrics instance.

    Returns:
        Global APQMetrics instance
    """
    global _global_metrics
    if _global_metrics is None:
        _global_metrics = APQMetrics()
    return _global_metrics


def reset_global_metrics() -> None:
    """Reset the global metrics instance.

    Primarily for testing purposes.
    """
    global _global_metrics
    _global_metrics = None
