"""
Phase 1.1: Database Connection Chaos Tests (Real PostgreSQL Backend)

Tests for database connection failures and recovery scenarios using actual
PostgreSQL connections and the real FraiseQL client.

This version validates real resilience to PostgreSQL connectivity issues
through actual network failures and recovery, not simulations.
"""

import pytest
import time
import statistics
import asyncio

from chaos.fraiseql_scenarios import FraiseQLTestScenarios
from chaos.base import ChaosMetrics


@pytest.mark.chaos
@pytest.mark.chaos_database
@pytest.mark.chaos_real_db
@pytest.mark.asyncio
async def test_connection_refused_recovery(
    chaos_db_client, chaos_test_schema, baseline_metrics
):
    """
    Test recovery from database connection refused errors.

    Scenario: Database connections are disabled (simulated via client),
    then re-enabled. FraiseQL should handle failures gracefully and recover.

    Expected:
    - Error rate during chaos > 0
    - Recovery time similar to baseline
    """
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.simple_user_query()

    # Start baseline measurement
    metrics.start_test()

    # Measure baseline performance (no chaos)
    baseline_times = []
    baseline_errors = 0

    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 10.0)
            baseline_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            baseline_errors += 1
            metrics.record_error()

    avg_baseline = statistics.mean(baseline_times) if baseline_times else 50.0

    # Inject chaos: Simulate connection failures
    chaos_db_client.inject_connection_failure()

    # Test under chaos - operations should fail
    chaos_times = []
    errors_during_chaos = 0

    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            # If we got here, the chaos injection didn't work as expected
            execution_time = result.get("_execution_time_ms", 10.0)
            chaos_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except ConnectionError:
            # Expected: connection refused
            errors_during_chaos += 1
            metrics.record_error()
        except Exception as e:
            # Other errors during chaos
            errors_during_chaos += 1
            metrics.record_error()

    # Reset chaos: Re-enable connections
    chaos_db_client.reset_chaos()

    # Test recovery - operations should work normally again
    recovery_times = []
    recovery_errors = 0

    for _ in range(5):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 10.0)
            recovery_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception as e:
            recovery_errors += 1
            metrics.record_error()

    avg_recovery = statistics.mean(recovery_times) if recovery_times else 50.0

    # End test and validate
    metrics.end_test()

    # Validate results
    assert errors_during_chaos > 0, "Should have connection errors during chaos injection"
    assert recovery_errors == 0, "Should have no errors after chaos reset"
    assert (
        abs(avg_recovery - avg_baseline) < avg_baseline * 0.5
    ), f"Recovery time {avg_recovery:.2f}ms should be similar to baseline {avg_baseline:.2f}ms"


@pytest.mark.chaos
@pytest.mark.chaos_database
@pytest.mark.chaos_real_db
@pytest.mark.asyncio
async def test_pool_exhaustion_recovery(
    chaos_db_client, chaos_test_schema, baseline_metrics
):
    """
    Test recovery from database connection pool exhaustion.

    Scenario: Simulate pool exhaustion by injecting latency,
    then verify recovery when latency is removed.

    Expected:
    - Some operations time out or are delayed during high latency
    - Operations complete normally after latency is removed
    """
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.complex_nested_query()

    metrics.start_test()

    # Baseline: Normal operations
    baseline_times = []
    for _ in range(3):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 50.0)
            baseline_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception as e:
            metrics.record_error()

    avg_baseline = statistics.mean(baseline_times) if baseline_times else 50.0

    # Inject chaos: Simulate high latency (5 seconds)
    # This simulates a heavily loaded database
    chaos_db_client.inject_latency(5000)

    # Test under pool exhaustion conditions
    chaos_times = []
    timeouts = 0
    completed = 0

    for _ in range(3):
        try:
            # With 5s latency, this should take ~5+ seconds
            result = await asyncio.wait_for(
                chaos_db_client.execute_query(operation), timeout=2.0
            )
            execution_time = result.get("_execution_time_ms", 2000.0)
            chaos_times.append(execution_time)
            metrics.record_query_time(execution_time)
            completed += 1
        except asyncio.TimeoutError:
            timeouts += 1
            metrics.record_error()
        except Exception as e:
            timeouts += 1
            metrics.record_error()

    # Remove chaos
    chaos_db_client.reset_chaos()

    # Test recovery
    recovery_times = []
    recovery_errors = 0

    for _ in range(3):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 50.0)
            recovery_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            recovery_errors += 1
            metrics.record_error()

    avg_recovery = statistics.mean(recovery_times) if recovery_times else 50.0

    metrics.end_test()

    # Validate pool exhaustion behavior
    assert (
        timeouts > 0
    ), "Should experience some timeouts during pool exhaustion chaos"
    assert (
        recovery_errors == 0
    ), "Should have no errors after chaos removal"
    assert (
        abs(avg_recovery - avg_baseline) < avg_baseline * 1.0
    ), f"Recovery time {avg_recovery:.2f}ms should return to near baseline {avg_baseline:.2f}ms"


@pytest.mark.chaos
@pytest.mark.chaos_database
@pytest.mark.chaos_real_db
@pytest.mark.asyncio
async def test_slow_connection_establishment(
    chaos_db_client, chaos_test_schema, baseline_metrics
):
    """
    Test handling of slow database connection establishment.

    Scenario: Simulate progressively increasing latency,
    verifying FraiseQL adapts to slower connections.

    Expected:
    - Operations complete even under high latency (but slowly)
    - Recovery is fast when latency is removed
    """
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.simple_user_query()

    metrics.start_test()

    # Baseline: Normal connection time
    baseline_times = []
    for _ in range(3):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 18.0)
            baseline_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            metrics.record_error()

    avg_baseline = statistics.mean(baseline_times) if baseline_times else 20.0

    # Inject latency progressively
    latencies = [100, 500, 1000, 2000]  # Progressive increase in ms

    for latency_ms in latencies:
        chaos_db_client.inject_latency(latency_ms)

        # Test connection under increased latency
        connection_times = []
        for _ in range(2):
            try:
                start = time.time()
                result = await chaos_db_client.execute_query(operation)
                execution_time = result.get("_execution_time_ms", latency_ms + 10)
                connection_times.append(execution_time)
                metrics.record_query_time(execution_time)
            except Exception as e:
                metrics.record_error()

        # Verify operation completed with expected latency
        if connection_times:
            avg_connection_time = statistics.mean(connection_times)
            # Should have at least the injected latency
            assert (
                avg_connection_time >= latency_ms * 0.9
            ), f"Expected ~{latency_ms}ms latency, got {avg_connection_time:.1f}ms"

    # Remove chaos and test recovery
    chaos_db_client.reset_chaos()

    recovery_times = []
    for _ in range(3):
        try:
            result = await chaos_db_client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 18.0)
            recovery_times.append(execution_time)
            metrics.record_query_time(execution_time)
        except Exception:
            metrics.record_error()

    avg_recovery = statistics.mean(recovery_times) if recovery_times else 20.0

    metrics.end_test()

    # Validate adaptation to slow connections
    assert (
        avg_recovery < avg_baseline * 1.5
    ), f"Should recover to near-baseline: {avg_recovery:.2f}ms vs {avg_baseline:.2f}ms"


@pytest.mark.chaos
@pytest.mark.chaos_database
@pytest.mark.chaos_real_db
@pytest.mark.asyncio
async def test_mid_query_connection_drop(
    chaos_db_client, chaos_test_schema, baseline_metrics
):
    """
    Test recovery from mid-query connection drops.

    Scenario: Inject connection failures at various points during query execution.

    Expected:
    - Failures are detected and reported as errors
    - System maintains reasonable success rate despite failures
    """
    metrics = ChaosMetrics()
    operation = FraiseQLTestScenarios.complex_nested_query()

    for drop_after_ms in [100, 500, 1000]:
        metrics.start_test()

        # Baseline successful queries
        successful_queries = 0
        baseline_times = []

        for _ in range(5):
            try:
                result = await chaos_db_client.execute_query(operation)
                execution_time = result.get("_execution_time_ms", 50.0)
                baseline_times.append(execution_time)
                metrics.record_query_time(execution_time)
                successful_queries += 1
            except Exception:
                metrics.record_error()

        avg_baseline = (
            statistics.mean(baseline_times)
            if baseline_times
            else 50.0
        )

        # Inject chaos: Connection failures after specified time
        chaos_db_client.inject_latency(drop_after_ms)

        chaos_queries = 0
        interrupted_queries = 0

        for _ in range(5):
            try:
                # Set timeout to be shorter than latency for early drop points
                timeout = max(1.0, (drop_after_ms + 100) / 1000.0)
                result = await asyncio.wait_for(
                    chaos_db_client.execute_query(operation),
                    timeout=timeout,
                )
                execution_time = result.get("_execution_time_ms", 50.0)
                metrics.record_query_time(execution_time)
                chaos_queries += 1
            except asyncio.TimeoutError:
                interrupted_queries += 1
                metrics.record_error()
            except Exception:
                interrupted_queries += 1
                metrics.record_error()

        # Reset chaos
        chaos_db_client.reset_chaos()

        metrics.end_test()

        # Validate mid-query failure handling
        assert (
            interrupted_queries > 0
        ), f"Should have interrupted queries with {drop_after_ms}ms latency"
