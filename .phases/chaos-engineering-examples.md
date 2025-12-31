# Chaos Engineering Test Suite - Implementation Examples

This document provides concrete code examples for implementing the chaos engineering test suite outlined in `phase-chaos-engineering-plan.md`.

---

## Example 1: Base Chaos Test Class

```python
# tests/chaos/base.py

import json
import time
from abc import ABC
from dataclasses import dataclass, field, asdict
from typing import Dict, List, Any, Optional
from contextlib import contextmanager
import pytest
from collections import defaultdict


@dataclass
class ChaosMetrics:
    """Metrics collected during a chaos test."""

    # Request metrics
    total_requests: int = 0
    successful_requests: int = 0
    failed_requests: int = 0

    # Latency metrics (in milliseconds)
    latencies: List[float] = field(default_factory=list)
    min_latency: float = 0.0
    max_latency: float = 0.0
    avg_latency: float = 0.0
    p50_latency: float = 0.0
    p95_latency: float = 0.0
    p99_latency: float = 0.0

    # Error breakdown
    error_types: Dict[str, int] = field(default_factory=lambda: defaultdict(int))

    # Recovery metrics
    recovery_time_ms: float = 0.0
    recovered_after_failure: bool = False

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        data = asdict(self)
        # Convert error_types defaultdict to regular dict
        data['error_types'] = dict(data['error_types'])
        return data

    def calculate_percentiles(self):
        """Calculate latency percentiles."""
        if not self.latencies:
            return

        sorted_latencies = sorted(self.latencies)
        self.min_latency = sorted_latencies[0]
        self.max_latency = sorted_latencies[-1]
        self.avg_latency = sum(sorted_latencies) / len(sorted_latencies)

        def percentile(p):
            idx = int(len(sorted_latencies) * (p / 100.0))
            return sorted_latencies[min(idx, len(sorted_latencies) - 1)]

        self.p50_latency = percentile(50)
        self.p95_latency = percentile(95)
        self.p99_latency = percentile(99)


class ChaosTestCase(ABC):
    """Base class for chaos engineering tests."""

    def __init__(self):
        self.metrics = ChaosMetrics()
        self.baseline_metrics = None
        self.chaos_active = False

    def load_baseline(self, baseline_file: str = "tests/chaos/baseline_metrics.json"):
        """Load baseline metrics from file."""
        try:
            with open(baseline_file, 'r') as f:
                baseline_data = json.load(f)
                # Convert dict back to ChaosMetrics
                self.baseline_metrics = ChaosMetrics(**baseline_data)
        except FileNotFoundError:
            pytest.skip(f"Baseline metrics not found: {baseline_file}")

    def record_request(self, latency_ms: float, success: bool, error_type: Optional[str] = None):
        """Record a single request's metrics."""
        self.metrics.total_requests += 1
        self.metrics.latencies.append(latency_ms)

        if success:
            self.metrics.successful_requests += 1
        else:
            self.metrics.failed_requests += 1
            if error_type:
                self.metrics.error_types[error_type] += 1

    def assert_within_baseline(self, tolerance: float = 2.0):
        """
        Assert that metrics are within tolerance of baseline.

        Args:
            tolerance: Multiplier (e.g., 2.0 = 2x baseline is acceptable)
        """
        if self.baseline_metrics is None:
            self.load_baseline()

        self.metrics.calculate_percentiles()
        baseline_latency = self.baseline_metrics.avg_latency
        acceptable_latency = baseline_latency * tolerance

        assert self.metrics.avg_latency <= acceptable_latency, (
            f"Latency {self.metrics.avg_latency}ms exceeds tolerance "
            f"({acceptable_latency}ms = {baseline_latency}ms * {tolerance})"
        )

    def assert_recovery_time(self, max_ms: float = 5000):
        """Assert that recovery time is within acceptable bounds."""
        assert self.metrics.recovery_time_ms <= max_ms, (
            f"Recovery time {self.metrics.recovery_time_ms}ms exceeds {max_ms}ms"
        )

    def assert_no_data_corruption(self, original_data: Dict, retrieved_data: Dict):
        """Assert that retrieved data matches original."""
        assert original_data == retrieved_data, (
            f"Data corruption detected:\nOriginal: {original_data}\nRetrieved: {retrieved_data}"
        )

    def save_metrics(self, test_name: str):
        """Save metrics to file for later analysis."""
        filename = f"tests/chaos/results/{test_name}_metrics.json"
        with open(filename, 'w') as f:
            json.dump(self.metrics.to_dict(), f, indent=2)

    @contextmanager
    def measure_latency(self):
        """Context manager for measuring operation latency."""
        start = time.time()
        try:
            yield
        finally:
            elapsed_ms = (time.time() - start) * 1000
            # Don't record here; let the operation record it

    def measure_recovery(self, check_interval_ms: int = 100, max_wait_ms: int = 30000):
        """
        Measure how long it takes for system to recover.

        Args:
            check_interval_ms: How often to check if recovered
            max_wait_ms: Maximum time to wait
        """
        recovery_start = time.time()
        recovered = False

        while time.time() - recovery_start < max_wait_ms / 1000:
            if self.is_system_healthy():
                recovered = True
                break
            time.sleep(check_interval_ms / 1000)

        recovery_ms = (time.time() - recovery_start) * 1000
        self.metrics.recovery_time_ms = recovery_ms
        self.metrics.recovered_after_failure = recovered

        return recovered

    def is_system_healthy(self) -> bool:
        """Check if system is healthy (override in subclasses)."""
        # Default: try a simple query
        try:
            # This would call your actual application
            return True
        except Exception:
            return False
```

---

## Example 2: Network Chaos Fixtures

```python
# tests/chaos/fixtures.py

import pytest
import subprocess
import time
from typing import Optional
from dataclasses import dataclass


@dataclass
class ToxiproxyConfig:
    """Toxiproxy configuration."""
    host: str = "localhost"
    port: int = 8474
    upstream_host: str = "localhost"
    upstream_port: int = 5432
    proxy_name: str = "postgres_chaos"


class ToxiproxyManager:
    """Manage Toxiproxy for network chaos injection."""

    def __init__(self, config: ToxiproxyConfig = None):
        self.config = config or ToxiproxyConfig()
        self.process = None

    def start(self):
        """Start Toxiproxy server."""
        try:
            self.process = subprocess.Popen(
                ["toxiproxy-server"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )
            time.sleep(2)  # Wait for startup
        except FileNotFoundError:
            raise RuntimeError("toxiproxy-server not found. Install with: brew install toxiproxy")

    def stop(self):
        """Stop Toxiproxy server."""
        if self.process:
            self.process.terminate()
            self.process.wait()

    def add_latency(self, latency_ms: int, jitter_ms: int = 0):
        """
        Add latency to all traffic through proxy.

        Args:
            latency_ms: Base latency to add
            jitter_ms: Random jitter (±jitter_ms)
        """
        # In real implementation, would call Toxiproxy API
        pass

    def drop_packets(self, percentage: float):
        """Drop a percentage of packets."""
        # Call Toxiproxy API
        pass

    def corrupt_data(self, percentage: float):
        """Corrupt a percentage of data in packets."""
        # Call Toxiproxy API
        pass

    def reset(self):
        """Reset all toxics (remove chaos)."""
        # Call Toxiproxy API
        pass


@pytest.fixture
def toxiproxy():
    """Fixture for Toxiproxy manager."""
    manager = ToxiproxyManager()
    manager.start()
    yield manager
    manager.stop()


@pytest.fixture
def chaos_database_latency(toxiproxy):
    """Fixture for database latency chaos."""
    def _inject(latency_ms: int, duration_s: Optional[int] = None):
        toxiproxy.add_latency(latency_ms)
        if duration_s:
            time.sleep(duration_s)
            toxiproxy.reset()
    return _inject


@pytest.fixture
def chaos_packet_loss(toxiproxy):
    """Fixture for packet loss chaos."""
    def _inject(percentage: float, duration_s: Optional[int] = None):
        toxiproxy.drop_packets(percentage)
        if duration_s:
            time.sleep(duration_s)
            toxiproxy.reset()
    return _inject
```

---

## Example 3: Database Connection Chaos Test

```python
# tests/chaos/network/test_db_connection_chaos.py

import pytest
import asyncio
from chaos.base import ChaosTestCase
from fraiseql.db import DatabasePool


class TestDatabaseConnectionChaos(ChaosTestCase):
    """Test database connection failure scenarios."""

    @pytest.fixture
    def db_pool(self):
        """Create database pool for testing."""
        return DatabasePool(
            host="localhost",
            port=5432,
            database="fraiseql_test",
            min_size=5,
            max_size=20,
            timeout=5.0
        )

    @pytest.mark.chaos
    def test_connection_refused(self, db_pool):
        """Test behavior when database connection is refused."""
        # Start with pool working
        with db_pool.acquire() as conn:
            assert conn is not None

        # Stop PostgreSQL (simulate connection refused)
        # In real test, would use subprocess to stop PostgreSQL

        # Try to connect (should fail fast, not hang)
        start = time.time()
        with pytest.raises(ConnectionRefusedError):
            with db_pool.acquire() as conn:
                pass

        elapsed_ms = (time.time() - start) * 1000
        self.record_request(elapsed_ms, success=False, error_type="connection_refused")

        # Should fail quickly (timeout configured)
        assert elapsed_ms < 5000, "Connection timeout should be <5s"

        # Start PostgreSQL again
        # connection pool should recover automatically

        recovered = self.measure_recovery(max_wait_ms=10000)
        assert recovered, "Pool should recover when database comes back"

        self.assert_recovery_time(max_ms=10000)
        self.save_metrics("test_connection_refused")

    @pytest.mark.chaos
    def test_connection_pool_exhaustion(self, db_pool):
        """Test behavior when all connections are in use."""
        connections = []

        # Acquire all available connections
        for i in range(db_pool.max_size):
            conn = db_pool.acquire()
            connections.append(conn)

        # Next request should wait in queue
        start = time.time()

        # This should not fail, just wait
        async def wait_for_connection():
            try:
                async with asyncio.timeout(5):
                    with db_pool.acquire() as conn:
                        pass
            except asyncio.TimeoutError:
                return False
            return True

        success = asyncio.run(wait_for_connection())
        elapsed_ms = (time.time() - start) * 1000

        self.record_request(elapsed_ms, success=success, error_type=None if success else "queue_timeout")

        # Release connections
        for conn in connections:
            conn.close()

        # Verify queue depth was reported
        # assert db_pool.queue_depth() == 1  # Would need to implement queue depth tracking

        self.save_metrics("test_connection_pool_exhaustion")

    @pytest.mark.chaos
    def test_connection_drops_mid_query(self, db_pool):
        """Test behavior when connection drops during query execution."""
        attempt_count = 0
        max_attempts = 3

        while attempt_count < max_attempts:
            try:
                with db_pool.acquire() as conn:
                    # Execute query that will be interrupted
                    # In real test, would inject network failure mid-query
                    result = conn.execute("SELECT * FROM users LIMIT 10")
                    break
            except ConnectionError as e:
                attempt_count += 1
                self.record_request(0, success=False, error_type="connection_drop")

                if attempt_count < max_attempts:
                    time.sleep(1)  # Wait before retry
                else:
                    raise AssertionError(f"Failed after {max_attempts} attempts: {e}")

        # Should succeed on retry
        assert attempt_count < max_attempts, "Should recover within max attempts"
        self.save_metrics("test_connection_drops_mid_query")
```

---

## Example 4: Chaos Decorator

```python
# tests/chaos/decorators.py

import functools
import time
from typing import Optional
from enum import Enum


class FailureType(Enum):
    """Types of failures that can be injected."""
    NETWORK_LATENCY = "network_latency"
    PACKET_LOSS = "packet_loss"
    CONNECTION_REFUSED = "connection_refused"
    TIMEOUT = "timeout"
    MEMORY_PRESSURE = "memory_pressure"
    CONCURRENT_LOAD = "concurrent_load"


def chaos_inject(
    failure_type: FailureType,
    duration: int = 30,
    intensity: float = 1.0,
    **kwargs
):
    """
    Decorator to inject chaos into test.

    Args:
        failure_type: Type of failure to inject
        duration: How long to maintain failure (seconds)
        intensity: Intensity of failure (0.0-1.0 for percentage, or multiplier)
        **kwargs: Failure-specific parameters

    Example:
        @chaos_inject(FailureType.NETWORK_LATENCY, duration=30, latency_ms=500)
        def test_query_with_latency(self):
            ...
    """
    def decorator(func):
        @functools.wraps(func)
        def wrapper(self, *args, **test_kwargs):
            # Initialize chaos
            with self.inject_chaos(failure_type, duration, intensity, **kwargs):
                # Run test while chaos is active
                result = func(self, *args, **test_kwargs)

            return result
        return wrapper
    return decorator


def fault_tolerant(
    max_retries: int = 3,
    retry_delay: float = 1.0,
    backoff_multiplier: float = 2.0
):
    """
    Decorator to verify fault tolerance.

    Wraps a test to automatically retry on failure and track retry metrics.
    """
    def decorator(func):
        @functools.wraps(func)
        def wrapper(self, *args, **kwargs):
            last_error = None

            for attempt in range(max_retries):
                try:
                    return func(self, *args, **kwargs)
                except Exception as e:
                    last_error = e

                    if attempt < max_retries - 1:
                        delay = retry_delay * (backoff_multiplier ** attempt)
                        time.sleep(delay)

            raise AssertionError(
                f"Failed after {max_retries} attempts: {last_error}"
            )
        return wrapper
    return decorator
```

---

## Example 5: Metrics Comparison

```python
# tests/chaos/metrics.py

import json
from dataclasses import asdict
from typing import Dict, Tuple
from chaos.base import ChaosMetrics


class MetricsComparator:
    """Compare test metrics against baseline."""

    def __init__(self, baseline: ChaosMetrics):
        self.baseline = baseline

    def compare(self, actual: ChaosMetrics) -> Dict[str, any]:
        """
        Compare actual metrics against baseline.

        Returns:
            Dictionary with comparison results
        """
        actual.calculate_percentiles()

        return {
            "success_rate": self._compare_percentage(
                actual.successful_requests / actual.total_requests if actual.total_requests else 0
            ),
            "latency_p50": self._compare_latency(
                actual.p50_latency, self.baseline.p50_latency
            ),
            "latency_p95": self._compare_latency(
                actual.p95_latency, self.baseline.p95_latency
            ),
            "latency_p99": self._compare_latency(
                actual.p99_latency, self.baseline.p99_latency
            ),
            "max_latency": self._compare_latency(
                actual.max_latency, self.baseline.max_latency
            ),
            "error_rate": self._compare_percentage(
                actual.failed_requests / actual.total_requests if actual.total_requests else 0
            ),
        }

    def _compare_latency(self, actual: float, baseline: float) -> Tuple[float, str]:
        """Compare latency values."""
        multiplier = actual / baseline if baseline > 0 else 0
        status = "pass" if multiplier <= 3.0 else "fail"  # Allow 3x degradation
        return (multiplier, status)

    def _compare_percentage(self, actual: float) -> Tuple[float, str]:
        """Compare percentage values."""
        percentage = actual * 100
        status = "pass" if percentage >= 90 else "fail"  # Allow 10% error
        return (percentage, status)
```

---

## Example 6: Running Chaos Tests

```bash
#!/bin/bash
# scripts/run_chaos_tests.sh

set -e

CHAOS_RESULTS_DIR="tests/chaos/results"
BASELINE_FILE="tests/chaos/baseline_metrics.json"

# Create results directory
mkdir -p "$CHAOS_RESULTS_DIR"

echo "🔥 Running Chaos Engineering Test Suite"
echo "========================================"

# Phase 0: Generate baseline (if not exists)
if [ ! -f "$BASELINE_FILE" ]; then
    echo "📊 Generating baseline metrics..."
    pytest tests/chaos/baseline/ -v --tb=short
fi

# Phase 1: Network Chaos
echo ""
echo "🌐 Phase 1: Network Chaos Tests"
pytest tests/chaos/network/ -v --tb=short -m chaos

# Phase 2: Database Chaos
echo ""
echo "🗄️  Phase 2: Database Chaos Tests"
pytest tests/chaos/database/ -v --tb=short -m chaos

# Phase 3: Cache/Auth Chaos
echo ""
echo "🔐 Phase 3: Cache & Auth Chaos Tests"
pytest tests/chaos/cache/ tests/chaos/auth/ -v --tb=short -m chaos

# Phase 4: Resource/Concurrency Chaos
echo ""
echo "⚙️  Phase 4: Resource & Concurrency Chaos Tests"
pytest tests/chaos/resources/ tests/chaos/concurrency/ -v --tb=short -m chaos

# Phase 5: Observability
echo ""
echo "📈 Phase 5: Observability & Reporting"
pytest tests/chaos/observability/ -v --tb=short -m chaos

# Generate report
echo ""
echo "📋 Generating chaos test report..."
python tests/chaos/reporting/generate_report.py "$CHAOS_RESULTS_DIR"

echo ""
echo "✅ Chaos Engineering Test Suite Complete!"
echo "📊 Results in: $CHAOS_RESULTS_DIR"
```

---

## Usage

### Run all chaos tests:
```bash
pytest tests/chaos/ -v --chaos-report=chaos_report.html
```

### Run specific phase:
```bash
pytest tests/chaos/network/ -v
```

### Run with specific failure injection:
```bash
pytest tests/chaos/ -v -k "latency"
```

### Run with coverage:
```bash
pytest tests/chaos/ -v --cov=fraiseql --cov-report=html
```

---

*These examples provide the foundation for the full chaos engineering test suite.*
*Adapt and expand based on specific FraiseQL implementation details.*
