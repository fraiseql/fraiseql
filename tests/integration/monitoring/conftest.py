"""Fixtures for Phase 19 integration testing with PostgreSQL."""

from __future__ import annotations

import asyncio
import os
from collections import defaultdict
from unittest.mock import MagicMock

import pytest

from fraiseql.cli.monitoring.database_commands import _get_db_monitor
from fraiseql.monitoring.runtime.cache_monitor_sync import cache_monitor_sync
from fraiseql.monitoring.runtime.db_monitor_sync import (
    DatabaseMonitorSync,
    get_database_monitor,
)


@pytest.fixture(scope="session")
def event_loop():
    """Create event loop for async tests."""
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    yield loop
    loop.close()


@pytest.fixture
def postgres_available():
    """Check if PostgreSQL is available for testing."""
    # In CI/testing, we might use in-memory or test database
    # For now, assume PostgreSQL is available if we have a connection string
    return os.getenv("DATABASE_URL") is not None or True  # Default to True for local testing


@pytest.fixture
def monitoring_enabled():
    """Initialize monitoring for each test."""
    # Reset database monitor singleton to clean state
    db_monitor = get_database_monitor()

    # Clear previous metrics
    with db_monitor._lock:
        db_monitor._recent_queries.clear()
        db_monitor._slow_queries.clear()
        db_monitor._statistics = None
        db_monitor._pool_metrics = None

    yield db_monitor

    # Cleanup after test
    with db_monitor._lock:
        db_monitor._recent_queries.clear()
        db_monitor._slow_queries.clear()
        db_monitor._statistics = None
        db_monitor._pool_metrics = None


@pytest.fixture
def db_monitor_sync(monitoring_enabled):
    """Provide DatabaseMonitorSync accessor."""
    return DatabaseMonitorSync(monitor=monitoring_enabled)


@pytest.fixture
def cache_monitor_fixture():
    """Provide cache monitor fixture."""
    return cache_monitor_sync


@pytest.fixture
def mock_health_components():
    """Create mock health components for testing health checks."""
    return {
        "database": MagicMock(get_utilization_percent=MagicMock(return_value=50.0)),
        "cache": MagicMock(is_healthy=MagicMock(return_value=True)),
        "graphql": MagicMock(get_statistics=MagicMock(return_value=MagicMock(success_rate=0.95))),
        "tracing": MagicMock(is_enabled=MagicMock(return_value=True)),
    }


@pytest.fixture
def performance_baseline():
    """Performance baseline for validation."""
    return {
        "operation_overhead_ms": 0.15,  # Rust target
        "python_overhead_ms": 1.0,  # Python target
        "health_check_ms": 100.0,  # All health checks combined
        "audit_query_ms": 500.0,  # Slow audit queries
        "cli_response_ms": 2000.0,  # CLI worst case
    }


@pytest.fixture
def sample_query_metrics():
    """Create sample query metrics for testing."""
    from datetime import datetime, timedelta

    from fraiseql.monitoring.db_monitor import QueryMetrics

    import hashlib
    import uuid

    metrics = []
    now = datetime.now()

    # Recent fast queries
    for i in range(5):
        sql = "SELECT * FROM users LIMIT 10"
        m = QueryMetrics(
            query_id=str(uuid.uuid4()),
            query_hash=hashlib.sha256(sql.encode()).hexdigest(),
            query_type="SELECT",
            timestamp=now - timedelta(seconds=i),
            duration_ms=5.0 + (i * 0.5),
            rows_affected=10,
        )
        metrics.append(m)

    # Recent slow queries
    for i in range(3):
        sql = "SELECT * FROM large_table JOIN other_table"
        m = QueryMetrics(
            query_id=str(uuid.uuid4()),
            query_hash=hashlib.sha256(sql.encode()).hexdigest(),
            query_type="SELECT",
            timestamp=now - timedelta(seconds=10 + i),
            duration_ms=150.0 + (i * 10),
            rows_affected=1000,
        )
        metrics.append(m)

    # Failed queries
    for i in range(2):
        sql = "UPDATE users SET active=true WHERE id=999999"
        m = QueryMetrics(
            query_id=str(uuid.uuid4()),
            query_hash=hashlib.sha256(sql.encode()).hexdigest(),
            query_type="UPDATE",
            timestamp=now - timedelta(seconds=20 + i),
            duration_ms=50.0,
            rows_affected=0,
            error="constraint violation",
        )
        metrics.append(m)

    return metrics


@pytest.fixture
def sample_graphql_operations():
    """Create sample GraphQL operation metrics."""
    from datetime import datetime, timedelta
    from dataclasses import dataclass
    from enum import Enum

    @dataclass
    class GraphQLOperationType(Enum):
        """GraphQL operation types."""
        Query = "query"
        Mutation = "mutation"
        Subscription = "subscription"

    @dataclass
    class OperationMetrics:
        """GraphQL operation metrics."""
        operation_id: str
        operation_name: str
        operation_type: GraphQLOperationType
        query_length: int
        duration_ms: float = 0.0
        trace_id: str | None = None

        def set_duration(self, duration_ms: float):
            """Set operation duration."""
            self.duration_ms = duration_ms

    operations = []
    now = datetime.now()

    # Recent queries
    for i in range(5):
        op = OperationMetrics(
            operation_id=f"op-{i}",
            operation_name=f"GetUser{i}",
            operation_type=GraphQLOperationType.Query,
            query_length=100 + (i * 10),
            duration_ms=10.0 + (i * 2),
        )
        operations.append(op)

    # Recent mutations
    for i in range(3):
        op = OperationMetrics(
            operation_id=f"mut-{i}",
            operation_name=f"CreateUser{i}",
            operation_type=GraphQLOperationType.Mutation,
            query_length=200 + (i * 20),
            duration_ms=50.0 + (i * 5),
        )
        operations.append(op)

    # Slow operations
    for i in range(2):
        op = OperationMetrics(
            operation_id=f"slow-{i}",
            operation_name=f"ComplexQuery{i}",
            operation_type=GraphQLOperationType.Query,
            query_length=500 + (i * 100),
            duration_ms=500.0 + (i * 100),
        )
        operations.append(op)

    return operations


@pytest.fixture
def make_query_metric():
    """Factory for creating query metrics."""
    import hashlib
    import uuid
    from datetime import datetime

    def _make_metric(query_type="SELECT", duration_ms=5.0, rows_affected=0, error=None):
        sql = f"{query_type} * FROM table"
        return QueryMetrics(
            query_id=str(uuid.uuid4()),
            query_hash=hashlib.sha256(sql.encode()).hexdigest(),
            query_type=query_type,
            timestamp=datetime.now(),
            duration_ms=duration_ms,
            rows_affected=rows_affected,
            error=error,
        )

    return _make_metric


@pytest.fixture
def concurrent_operation_counter():
    """Counter for tracking concurrent operations."""
    return defaultdict(int)


@pytest.mark.asyncio
@pytest.fixture
async def async_monitoring_context(monitoring_enabled):
    """Async context for monitoring tests."""

    class AsyncContext:
        def __init__(self, monitor):
            self.monitor = monitor
            self.operations = []
            self.errors = []

        async def record_operation(self, op_type, duration_ms):
            """Record a simulated operation."""
            self.operations.append({"type": op_type, "duration_ms": duration_ms})

        async def record_error(self, op_type, error_msg):
            """Record a simulated error."""
            self.errors.append({"type": op_type, "error": error_msg})

    return AsyncContext(monitoring_enabled)
