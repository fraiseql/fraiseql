"""Database query and connection pool monitoring.

This module provides comprehensive monitoring of database operations including:
- Query performance tracking (duration, type, rows affected)
- Connection pool utilization metrics
- Transaction duration monitoring
- Slow query detection and alerting
"""

from collections import deque
from dataclasses import dataclass, field
from datetime import UTC, datetime
from threading import Lock


@dataclass
class QueryMetrics:
    """Metrics for a single database query.

    Attributes:
        query_id: Unique identifier for this query
        query_hash: SHA-256 hash of query text (for privacy)
        query_type: Type of query (SELECT, INSERT, UPDATE, DELETE, etc.)
        timestamp: When the query started
        duration_ms: Total duration in milliseconds
        execution_time_ms: Time spent in database execution
        network_time_ms: Time spent in network I/O
        rows_affected: Number of rows modified or returned
        parameter_count: Number of query parameters
        connection_acquired_ms: Time to acquire connection
        is_slow: Whether query exceeded slow threshold
        error: Error message if query failed (None if success)
        trace_id: W3C trace context ID for distributed tracing
    """

    query_id: str
    query_hash: str
    query_type: str
    timestamp: datetime
    duration_ms: float
    execution_time_ms: float = 0.0
    network_time_ms: float = 0.0
    rows_affected: int = 0
    parameter_count: int = 0
    connection_acquired_ms: float = 0.0
    is_slow: bool = False
    error: str | None = None
    trace_id: str | None = None

    def is_success(self) -> bool:
        """Check if query succeeded."""
        return self.error is None

    def is_failed(self) -> bool:
        """Check if query failed."""
        return self.error is not None


@dataclass
class PoolMetrics:
    """Metrics for connection pool state.

    Attributes:
        timestamp: When metrics were captured
        total_connections: Total pool size
        active_connections: Currently in use
        idle_connections: Available for use
        waiting_requests: Requests waiting for a connection
        avg_wait_time_ms: Average time to acquire connection
        max_wait_time_ms: Max time to acquire connection
        pool_utilization: Fraction of pool in use (0.0-1.0)
        connection_reuse_count: Total connections reused from pool
    """

    timestamp: datetime
    total_connections: int = 0
    active_connections: int = 0
    idle_connections: int = 0
    waiting_requests: int = 0
    avg_wait_time_ms: float = 0.0
    max_wait_time_ms: float = 0.0
    pool_utilization: float = 0.0
    connection_reuse_count: int = 0

    def get_utilization_percent(self) -> float:
        """Get pool utilization as percentage."""
        if self.total_connections == 0:
            return 0.0
        return (self.active_connections / self.total_connections) * 100


@dataclass
class TransactionMetrics:
    """Metrics for database transactions.

    Attributes:
        transaction_id: Unique transaction identifier
        start_time: When transaction started
        end_time: When transaction ended (None if still running)
        duration_ms: Total transaction duration
        query_count: Number of queries in transaction
        status: STARTED, COMMITTED, ROLLED_BACK
        is_long_running: Whether transaction exceeded duration threshold
        error: Error message if transaction failed
    """

    transaction_id: str
    start_time: datetime
    end_time: datetime | None = None
    duration_ms: float | None = None
    query_count: int = 0
    status: str = "STARTED"
    is_long_running: bool = False
    error: str | None = None

    def is_active(self) -> bool:
        """Check if transaction is still active."""
        return self.status == "STARTED"

    def is_committed(self) -> bool:
        """Check if transaction was committed."""
        return self.status == "COMMITTED"

    def is_rolled_back(self) -> bool:
        """Check if transaction was rolled back."""
        return self.status == "ROLLED_BACK"


@dataclass
class QueryStatistics:
    """Aggregate statistics for queries.

    Attributes:
        total_count: Total number of queries tracked
        success_count: Number of successful queries
        error_count: Number of failed queries
        success_rate: Percentage of successful queries (0.0-1.0)
        total_duration_ms: Sum of all query durations
        avg_duration_ms: Average query duration
        min_duration_ms: Minimum query duration
        max_duration_ms: Maximum query duration
        p50_duration_ms: 50th percentile (median) duration
        p95_duration_ms: 95th percentile duration
        p99_duration_ms: 99th percentile duration
        slow_count: Number of queries flagged as slow
        slow_rate: Percentage of slow queries (0.0-1.0)
    """

    total_count: int = 0
    success_count: int = 0
    error_count: int = 0
    success_rate: float = 0.0
    total_duration_ms: float = 0.0
    avg_duration_ms: float = 0.0
    min_duration_ms: float = 0.0
    max_duration_ms: float = 0.0
    p50_duration_ms: float = 0.0
    p95_duration_ms: float = 0.0
    p99_duration_ms: float = 0.0
    slow_count: int = 0
    slow_rate: float = 0.0


@dataclass
class PerformanceReport:
    """Comprehensive database performance report.

    Attributes:
        start_time: Report period start
        end_time: Report period end
        generated_at: When report was generated
        query_stats: Aggregate query statistics
        queries_by_type: Breakdown by query type
        slow_queries: Slowest queries in period
        pool_avg_utilization: Average pool utilization
        transactions_total: Total transactions in period
        transactions_committed: Committed transactions
        transactions_rolled_back: Rolled back transactions
    """

    start_time: datetime
    end_time: datetime
    generated_at: datetime
    query_stats: QueryStatistics = field(default_factory=QueryStatistics)
    queries_by_type: dict[str, int] = field(default_factory=dict)
    slow_queries: list[QueryMetrics] = field(default_factory=list)
    pool_avg_utilization: float = 0.0
    transactions_total: int = 0
    transactions_committed: int = 0
    transactions_rolled_back: int = 0

    def get_period_minutes(self) -> float:
        """Get report period in minutes."""
        delta = self.end_time - self.start_time
        return delta.total_seconds() / 60

    def get_queries_per_minute(self) -> float:
        """Get average queries per minute."""
        minutes = self.get_period_minutes()
        if minutes == 0:
            return 0.0
        return self.query_stats.total_count / minutes

    def get_summary_string(self) -> str:
        """Get human-readable summary of report."""
        minutes = self.get_period_minutes()
        return (
            f"Database Performance Report: Last {minutes:.0f} minutes\n"
            f"  Total Queries: {self.query_stats.total_count}\n"
            f"  Slow Queries: {self.query_stats.slow_count} "
            f"({self.query_stats.slow_rate:.1%})\n"
            f"  Success Rate: {self.query_stats.success_rate:.1%}\n"
            f"  Avg Duration: {self.query_stats.avg_duration_ms:.2f}ms\n"
            f"  P95 Duration: {self.query_stats.p95_duration_ms:.2f}ms\n"
            f"  P99 Duration: {self.query_stats.p99_duration_ms:.2f}ms\n"
            f"  Pool Utilization: {self.pool_avg_utilization:.1%}\n"
            f"  Transactions: {self.transactions_total} "
            f"(committed: {self.transactions_committed}, "
            f"rolled back: {self.transactions_rolled_back})"
        )


class DatabaseMonitor:
    """Thread-safe database monitoring and metrics collection.

    Tracks query performance, connection pool utilization, and transaction
    durations for operational visibility and performance optimization.
    """

    def __init__(
        self,
        max_recent_queries: int = 1000,
        max_slow_queries: int = 100,
        slow_query_threshold_ms: float = 100.0,
    ):
        """Initialize database monitor.

        Args:
            max_recent_queries: Max recent queries to keep in memory
            max_slow_queries: Max slow queries to keep in memory
            slow_query_threshold_ms: Threshold for marking query as slow
        """
        self._lock = Lock()
        self._recent_queries: deque[QueryMetrics] = deque(maxlen=max_recent_queries)
        self._slow_queries: deque[QueryMetrics] = deque(maxlen=max_slow_queries)
        self._pool_states: deque[PoolMetrics] = deque(maxlen=100)
        self._transactions: dict[str, TransactionMetrics] = {}
        self._slow_query_threshold = slow_query_threshold_ms

    # ===== Query Tracking =====

    async def record_query(self, metrics: QueryMetrics) -> None:
        """Record completed database query.

        Args:
            metrics: QueryMetrics object with query details
        """
        with self._lock:
            self._recent_queries.append(metrics)
            if metrics.is_slow:
                self._slow_queries.append(metrics)

    async def get_recent_queries(self, limit: int = 100) -> list[QueryMetrics]:
        """Get recent database queries.

        Args:
            limit: Maximum queries to return

        Returns:
            List of recent QueryMetrics, most recent first
        """
        with self._lock:
            return list(self._recent_queries)[-limit:][::-1]

    async def get_slow_queries(self, limit: int = 50) -> list[QueryMetrics]:
        """Get slow database queries.

        Args:
            limit: Maximum slow queries to return

        Returns:
            List of slow QueryMetrics, slowest first
        """
        with self._lock:
            slow = list(self._slow_queries)
            # Sort by duration, slowest first
            slow.sort(key=lambda q: q.duration_ms, reverse=True)
            return slow[:limit]

    async def get_queries_by_type(self) -> dict[str, int]:
        """Get query count by type.

        Returns:
            Dict mapping query type to count
        """
        with self._lock:
            counts: dict[str, int] = {}
            for query in self._recent_queries:
                counts[query.query_type] = counts.get(query.query_type, 0) + 1
            return counts

    # ===== Pool Monitoring =====

    async def record_pool_state(self, metrics: PoolMetrics) -> None:
        """Record connection pool state snapshot.

        Args:
            metrics: PoolMetrics object with pool details
        """
        with self._lock:
            self._pool_states.append(metrics)

    async def get_pool_metrics(self) -> PoolMetrics | None:
        """Get current connection pool metrics.

        Returns:
            Most recent PoolMetrics or None if no data
        """
        with self._lock:
            if self._pool_states:
                return self._pool_states[-1]
            return None

    async def get_pool_history(self, limit: int = 100) -> list[PoolMetrics]:
        """Get connection pool state history.

        Args:
            limit: Maximum states to return

        Returns:
            List of PoolMetrics, most recent first
        """
        with self._lock:
            return list(self._pool_states)[-limit:][::-1]

    # ===== Transaction Tracking =====

    async def start_transaction(self, transaction_id: str) -> None:
        """Start tracking a transaction.

        Args:
            transaction_id: Unique transaction identifier
        """
        with self._lock:
            self._transactions[transaction_id] = TransactionMetrics(
                transaction_id=transaction_id,
                start_time=datetime.now(UTC),
            )

    async def record_transaction_query(self, transaction_id: str) -> None:
        """Record a query within a transaction.

        Args:
            transaction_id: Transaction identifier
        """
        with self._lock:
            if transaction_id in self._transactions:
                self._transactions[transaction_id].query_count += 1

    async def commit_transaction(self, transaction_id: str) -> None:
        """Mark transaction as committed.

        Args:
            transaction_id: Transaction identifier
        """
        with self._lock:
            if transaction_id in self._transactions:
                txn = self._transactions[transaction_id]
                txn.end_time = datetime.now(UTC)
                txn.duration_ms = (txn.end_time - txn.start_time).total_seconds() * 1000
                txn.status = "COMMITTED"

    async def rollback_transaction(self, transaction_id: str) -> None:
        """Mark transaction as rolled back.

        Args:
            transaction_id: Transaction identifier
        """
        with self._lock:
            if transaction_id in self._transactions:
                txn = self._transactions[transaction_id]
                txn.end_time = datetime.now(UTC)
                txn.duration_ms = (txn.end_time - txn.start_time).total_seconds() * 1000
                txn.status = "ROLLED_BACK"

    # ===== Statistics =====

    async def get_query_statistics(self) -> QueryStatistics:
        """Get aggregate query statistics.

        Returns:
            QueryStatistics with aggregate metrics
        """
        with self._lock:
            queries = list(self._recent_queries)

        if not queries:
            return QueryStatistics()

        durations = [q.duration_ms for q in queries]
        success_count = sum(1 for q in queries if q.is_success())
        error_count = sum(1 for q in queries if q.is_failed())
        slow_count = sum(1 for q in queries if q.is_slow)

        stats = QueryStatistics(
            total_count=len(queries),
            success_count=success_count,
            error_count=error_count,
            success_rate=success_count / len(queries) if queries else 0.0,
            total_duration_ms=sum(durations),
            avg_duration_ms=sum(durations) / len(durations),
            min_duration_ms=min(durations),
            max_duration_ms=max(durations),
            slow_count=slow_count,
            slow_rate=slow_count / len(queries) if queries else 0.0,
        )

        # Calculate percentiles
        if durations:
            sorted_durations = sorted(durations)
            stats.p50_duration_ms = sorted_durations[len(sorted_durations) // 2]
            stats.p95_duration_ms = sorted_durations[int(len(sorted_durations) * 0.95)]
            stats.p99_duration_ms = sorted_durations[int(len(sorted_durations) * 0.99)]

        return stats

    # ===== Reports =====

    async def get_performance_report(
        self,
        start_time: datetime,
        end_time: datetime,
    ) -> PerformanceReport:
        """Generate comprehensive performance report.

        Args:
            start_time: Report period start
            end_time: Report period end

        Returns:
            PerformanceReport with aggregate statistics
        """
        with self._lock:
            queries = [q for q in self._recent_queries if start_time <= q.timestamp <= end_time]
            pool_states = [p for p in self._pool_states if start_time <= p.timestamp <= end_time]
            transactions = [
                t for t in self._transactions.values() if start_time <= t.start_time <= end_time
            ]

        report = PerformanceReport(
            start_time=start_time,
            end_time=end_time,
            generated_at=datetime.now(UTC),
        )

        # Query statistics
        if queries:
            durations = [q.duration_ms for q in queries]
            success_count = sum(1 for q in queries if q.is_success())
            slow_count = sum(1 for q in queries if q.is_slow)

            report.query_stats = QueryStatistics(
                total_count=len(queries),
                success_count=success_count,
                error_count=len(queries) - success_count,
                success_rate=success_count / len(queries),
                total_duration_ms=sum(durations),
                avg_duration_ms=sum(durations) / len(durations),
                min_duration_ms=min(durations),
                max_duration_ms=max(durations),
                slow_count=slow_count,
                slow_rate=slow_count / len(queries),
            )

            # Percentiles
            sorted_durations = sorted(durations)
            report.query_stats.p50_duration_ms = sorted_durations[len(sorted_durations) // 2]
            report.query_stats.p95_duration_ms = sorted_durations[int(len(sorted_durations) * 0.95)]
            report.query_stats.p99_duration_ms = sorted_durations[int(len(sorted_durations) * 0.99)]

            # By type breakdown
            from collections import Counter

            type_counts = Counter(q.query_type for q in queries)
            report.queries_by_type = dict(type_counts)

            # Slow queries
            slow = [q for q in queries if q.is_slow]
            slow.sort(key=lambda q: q.duration_ms, reverse=True)
            report.slow_queries = slow[:10]

        # Pool metrics
        if pool_states:
            avg_utilization = sum(p.pool_utilization for p in pool_states) / len(pool_states)
            report.pool_avg_utilization = avg_utilization

        # Transaction stats
        report.transactions_total = len(transactions)
        report.transactions_committed = sum(1 for t in transactions if t.is_committed())
        report.transactions_rolled_back = sum(1 for t in transactions if t.is_rolled_back())

        return report

    # ===== Utility =====

    async def clear(self) -> None:
        """Clear all collected metrics."""
        with self._lock:
            self._recent_queries.clear()
            self._slow_queries.clear()
            self._pool_states.clear()
            self._transactions.clear()

    async def get_query_count(self) -> int:
        """Get total number of queries tracked."""
        with self._lock:
            return len(self._recent_queries)

    async def get_slow_query_count(self) -> int:
        """Get total number of slow queries tracked."""
        with self._lock:
            return len(self._slow_queries)
