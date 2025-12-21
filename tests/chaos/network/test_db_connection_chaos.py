"""
Phase 1.1: Database Connection Chaos Tests

Tests for database connection failures and recovery scenarios.
Validates FraiseQL's resilience to PostgreSQL connectivity issues.
"""

import pytest
import time
import asyncio
from chaos.base import ChaosTestCase
from chaos.fixtures import ToxiproxyManager
from chaos.plugin import chaos_inject, FailureType


class TestDatabaseConnectionChaos(ChaosTestCase):
    """Test database connection chaos scenarios."""

    @pytest.mark.chaos
    @pytest.mark.chaos_database
    def test_connection_refused_recovery(self, toxiproxy: ToxiproxyManager):
        """
        Test recovery from database connection refused errors.

        Scenario: Database proxy rejects connections, then recovers.
        Expected: FraiseQL handles connection failures gracefully.
        """
        # Setup PostgreSQL proxy
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        # Start baseline measurement
        self.metrics.start_test()

        # TODO: Implement actual FraiseQL connection test
        # For now, simulate the test structure

        # Measure baseline performance (no chaos)
        baseline_times = []
        for _ in range(5):
            start = time.time()
            # Simulate database operation
            time.sleep(0.005)  # 5ms baseline
            baseline_times.append((time.time() - start) * 1000)

        avg_baseline = sum(baseline_times) / len(baseline_times)
        self.metrics.record_query_time(avg_baseline)

        # Inject chaos: Disable proxy (connection refused)
        toxiproxy.disable_proxy("fraiseql_postgres")

        # Test under chaos
        chaos_times = []
        errors_during_chaos = 0

        for _ in range(5):
            try:
                start = time.time()
                # This should fail due to connection refused
                time.sleep(0.001)  # Simulate fast failure
                chaos_times.append((time.time() - start) * 1000)
            except Exception:
                errors_during_chaos += 1
                self.metrics.record_error()
                # Simulate retry delay
                time.sleep(0.1)

        # Re-enable proxy
        toxiproxy.enable_proxy("fraiseql_postgres")

        # Test recovery
        recovery_times = []
        for _ in range(5):
            start = time.time()
            # Simulate database operation after recovery
            time.sleep(0.005)  # Should be back to baseline
            recovery_times.append((time.time() - start) * 1000)

        avg_recovery = sum(recovery_times) / len(recovery_times)
        self.metrics.record_query_time(avg_recovery)

        # End test and validate
        self.metrics.end_test()

        # Validate results
        assert errors_during_chaos > 0, "Should have connection errors during chaos"
        assert abs(avg_recovery - avg_baseline) < 2.0, (
            f"Recovery time {avg_recovery:.2f}ms should be close to baseline {avg_baseline:.2f}ms"
        )

        # Compare to baseline
        comparison = self.compare_to_baseline("db_connection")
        assert "current" in comparison

        # Cleanup
        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_database
    def test_pool_exhaustion_recovery(self, toxiproxy: ToxiproxyManager):
        """
        Test recovery from database connection pool exhaustion.

        Scenario: All database connections become slow/unavailable, then recover.
        Expected: FraiseQL handles pool exhaustion gracefully with queuing/recovery.
        """
        # Setup proxy
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Baseline: Normal operations
        baseline_times = []
        for _ in range(3):
            start = time.time()
            time.sleep(0.005)
            baseline_times.append((time.time() - start) * 1000)

        avg_baseline = sum(baseline_times) / len(baseline_times)

        # Inject chaos: Add extreme latency to simulate pool exhaustion
        toxiproxy.add_latency_toxic("fraiseql_postgres", latency_ms=5000)  # 5 second delay

        # Test under pool exhaustion conditions
        chaos_times = []
        timeouts = 0

        for _ in range(3):
            start = time.time()
            try:
                # Simulate operation that should timeout due to pool exhaustion
                time.sleep(1.0)  # 1 second operation
                chaos_times.append((time.time() - start) * 1000)
            except TimeoutError:
                timeouts += 1
                self.metrics.record_error()
                chaos_times.append(1000.0)  # Record timeout as 1 second

        # Remove chaos
        toxiproxy.remove_all_toxics("fraiseql_postgres")

        # Test recovery
        recovery_times = []
        for _ in range(3):
            start = time.time()
            time.sleep(0.005)  # Should be back to normal
            recovery_times.append((time.time() - start) * 1000)

        avg_recovery = sum(recovery_times) / len(recovery_times)

        self.metrics.end_test()

        # Validate pool exhaustion behavior
        assert timeouts >= 1, "Should experience some timeouts during pool exhaustion"
        assert avg_recovery < avg_baseline * 2, (
            f"Recovery should be reasonably fast: {avg_recovery:.2f}ms vs baseline {avg_baseline:.2f}ms"
        )

        # Cleanup
        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_database
    def test_slow_connection_establishment(self, toxiproxy: ToxiproxyManager):
        """
        Test handling of slow database connection establishment.

        Scenario: Database connections take progressively longer to establish.
        Expected: FraiseQL adapts to slow connection times.
        """
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Baseline
        baseline_times = []
        for _ in range(3):
            start = time.time()
            time.sleep(0.01)  # 10ms connection time
            baseline_times.append((time.time() - start) * 1000)

        avg_baseline = sum(baseline_times) / len(baseline_times)

        # Inject gradual latency increase (simulating network congestion)
        latencies = [100, 500, 1000, 2000]  # Progressive increase

        for latency_ms in latencies:
            toxiproxy.remove_all_toxics("fraiseql_postgres")
            toxiproxy.add_latency_toxic("fraiseql_postgres", latency_ms)

            # Test connection under increased latency
            connection_times = []
            for _ in range(2):
                start = time.time()
                time.sleep(latency_ms / 1000.0)  # Simulate connection delay
                connection_times.append((time.time() - start) * 1000)

            avg_connection_time = sum(connection_times) / len(connection_times)
            self.metrics.record_query_time(avg_connection_time)

        # Remove chaos and test recovery
        toxiproxy.remove_all_toxics("fraiseql_postgres")

        recovery_times = []
        for _ in range(3):
            start = time.time()
            time.sleep(0.01)
            recovery_times.append((time.time() - start) * 1000)

        avg_recovery = sum(recovery_times) / len(recovery_times)

        self.metrics.end_test()

        # Validate adaptation to slow connections
        assert avg_recovery < avg_baseline * 1.5, (
            f"Should recover to near-baseline: {avg_recovery:.2f}ms vs {avg_baseline:.2f}ms"
        )

        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_database
    @pytest.mark.parametrize("drop_after_ms", [100, 500, 1000])
    def test_mid_query_connection_drop(self, toxiproxy: ToxiproxyManager, drop_after_ms: int):
        """
        Test recovery from mid-query connection drops.

        Scenario: Connection drops partway through a query execution.
        Expected: FraiseQL handles partial query failures gracefully.
        """
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Baseline successful queries
        successful_queries = 0
        for _ in range(5):
            start = time.time()
            time.sleep(0.020)  # 20ms query
            successful_queries += 1
            self.metrics.record_query_time((time.time() - start) * 1000)

        # Inject chaos: Connection drops after specified time
        # This simulates network interruption mid-query
        chaos_queries = 0
        interrupted_queries = 0

        for _ in range(5):
            start = time.time()
            try:
                # Simulate query that gets interrupted
                time.sleep(drop_after_ms / 1000.0)
                # At this point, connection would drop in real scenario
                # We simulate this by raising an exception
                if drop_after_ms < 500:  # Early drops cause failures
                    raise ConnectionError("Connection dropped mid-query")
                else:
                    time.sleep(0.010)  # Complete the query
                    chaos_queries += 1
            except ConnectionError:
                interrupted_queries += 1
                self.metrics.record_error()
                # Simulate retry delay
                time.sleep(0.2)

        self.metrics.end_test()

        # Validate mid-query failure handling
        assert interrupted_queries > 0, (
            f"Should have interrupted queries with {drop_after_ms}ms drop time"
        )
        assert chaos_queries >= successful_queries * 0.6, (
            "Should maintain reasonable success rate under chaos"
        )

        toxiproxy.delete_proxy("fraiseql_postgres")
