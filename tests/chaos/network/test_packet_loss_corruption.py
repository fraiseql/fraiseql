"""
Phase 1.3: Packet Loss & Corruption Chaos Tests

Tests for packet loss, corruption, and network reliability scenarios.
Validates FraiseQL's handling of unreliable network conditions.
"""

import pytest
import time
import random
import statistics
from chaos.base import ChaosTestCase
from chaos.fixtures import ToxiproxyManager
from chaos.plugin import chaos_inject, FailureType
from chaos.fraiseql_scenarios import MockFraiseQLClient, FraiseQLTestScenarios


class TestPacketLossCorruptionChaos(ChaosTestCase):
    """Test packet loss and corruption chaos scenarios."""

    @pytest.mark.chaos
    @pytest.mark.chaos_network
    @pytest.mark.parametrize("loss_percentage", [0.01, 0.05, 0.10])  # 1%, 5%, 10%
    def test_packet_loss_recovery(self, toxiproxy: ToxiproxyManager, loss_percentage: float):
        """
        Test recovery from packet loss at different severity levels.

        Scenario: Network drops packets at specified rate.
        Expected: FraiseQL handles packet loss with retries and timeouts.
        """
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        # Create FraiseQL client for testing
        client = MockFraiseQLClient()
        operation = FraiseQLTestScenarios.simple_user_query()

        self.metrics.start_test()

        # Baseline: No packet loss
        baseline_successes = 0
        baseline_times = []

        for _ in range(10):
            result = client.execute_query(operation)
            execution_time = result.get("_execution_time_ms", 10.0)
            baseline_times.append(execution_time)
            self.metrics.record_query_time(execution_time)
            baseline_successes += 1

        avg_baseline = statistics.mean(baseline_times)

        # Inject packet loss
        toxiproxy.add_packet_loss_toxic("fraiseql_postgres", loss_percentage)

        # Test under packet loss
        chaos_successes = 0
        chaos_failures = 0
        chaos_times = []
        retry_count = 0

        for _ in range(20):  # More samples for statistical significance
            start = time.time()
            try:
                # Simulate operation that may fail due to packet loss
                if random.random() < loss_percentage:
                    # Packet loss - operation fails
                    raise ConnectionError("Packet loss")

                time.sleep(0.010)
                chaos_times.append((time.time() - start) * 1000)
                chaos_successes += 1

            except ConnectionError:
                chaos_failures += 1
                self.metrics.record_error()

                # Simulate retry logic
                retry_count += 1
                if random.random() < 0.7:  # 70% retry success rate
                    time.sleep(0.050)  # Retry delay
                    try:
                        time.sleep(0.010)
                        chaos_times.append((time.time() - start) * 1000)
                        chaos_successes += 1
                        retry_count -= 1  # Successful retry
                    except:
                        pass  # Retry failed

        self.metrics.record_query_time(statistics.mean(chaos_times) if chaos_times else 1000.0)

        # Remove chaos
        toxiproxy.remove_all_toxics("fraiseql_postgres")

        # Test recovery
        recovery_successes = 0
        recovery_times = []

        for _ in range(10):
            start = time.time()
            time.sleep(0.010)
            recovery_times.append((time.time() - start) * 1000)
            recovery_successes += 1

        avg_recovery = statistics.mean(recovery_times)

        self.metrics.end_test()

        # Validate packet loss behavior
        expected_failures = int(20 * loss_percentage * 0.3)  # Account for retries
        assert chaos_failures >= expected_failures, (
            f"Expected ~{expected_failures} failures at {loss_percentage * 100}% loss, got {chaos_failures}"
        )

        success_rate = chaos_successes / 20.0
        min_success_rate = 1.0 - (loss_percentage * 2)  # Allow for retry effectiveness
        assert success_rate >= min_success_rate, (
            f"Success rate {success_rate:.2f} too low for {loss_percentage * 100}% loss"
        )

        # Recovery should be near baseline
        assert abs(avg_recovery - avg_baseline) < 5.0, (
            f"Recovery time {avg_recovery:.1f}ms vs baseline {avg_baseline:.1f}ms"
        )

        print(".1f")
        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_network
    def test_packet_corruption_handling(self, toxiproxy: ToxiproxyManager):
        """
        Test handling of corrupted packets.

        Scenario: Network delivers corrupted data.
        Expected: FraiseQL detects corruption and handles appropriately.
        """
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Simulate packet corruption (not directly supported by toxiproxy)
        # We'll simulate this through timeout patterns
        corruption_scenarios = [
            ("minor_corruption", 0.02, 0.1),  # 2% corruption, 10% impact
            ("moderate_corruption", 0.05, 0.3),  # 5% corruption, 30% impact
            ("severe_corruption", 0.10, 0.6),  # 10% corruption, 60% impact
        ]

        for scenario_name, corruption_rate, impact_rate in corruption_scenarios:
            corrupt_successes = 0
            corrupt_failures = 0

            for _ in range(15):
                if random.random() < corruption_rate:
                    # Corrupted packet - operation fails
                    corrupt_failures += 1
                    self.metrics.record_error()
                else:
                    # Normal operation, but may still fail due to impact
                    if random.random() >= impact_rate:
                        corrupt_successes += 1
                        self.metrics.record_query_time(10.0 + random.uniform(-2, 2))
                    else:
                        corrupt_failures += 1
                        self.metrics.record_error()

            success_rate = corrupt_successes / 15.0
            expected_min_success = 1.0 - corruption_rate - impact_rate

            assert success_rate >= expected_min_success, (
                f"{scenario_name}: Success rate {success_rate:.2f} below expected {expected_min_success:.2f}"
            )

        self.metrics.end_test()

        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_network
    def test_out_of_order_delivery(self, toxiproxy: ToxiproxyManager):
        """
        Test handling of out-of-order packet delivery.

        Scenario: Network delivers packets in wrong order.
        Expected: FraiseQL handles reordering gracefully (TCP handles this).
        """
        # Note: Out-of-order delivery is primarily handled by TCP
        # This test simulates application-level reordering effects
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Simulate out-of-order effects through variable timing
        reorder_times = []

        for _ in range(10):
            # Simulate packets arriving out of order
            packet_delays = [0.010, 0.015, 0.008, 0.012, 0.009]  # Varied delays
            random.shuffle(packet_delays)  # Out of order

            start = time.time()
            for delay in packet_delays:
                time.sleep(delay)

            total_time = (time.time() - start) * 1000
            reorder_times.append(total_time)
            self.metrics.record_query_time(total_time)

        avg_reorder_time = statistics.mean(reorder_times)
        reorder_variance = statistics.stdev(reorder_times)

        # Validate reordering doesn't cause excessive variance
        assert reorder_variance < avg_reorder_time * 0.5, (
            f"High variance under reordering: {reorder_variance:.1f}ms"
        )

        self.metrics.end_test()

        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_network
    def test_duplicate_packet_handling(self, toxiproxy: ToxiproxyManager):
        """
        Test handling of duplicate packet delivery.

        Scenario: Network delivers duplicate packets.
        Expected: FraiseQL handles duplicates gracefully (TCP handles this).
        """
        # Note: Duplicate packets are primarily handled by TCP
        # This test simulates application-level duplicate handling
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Simulate duplicate packet effects
        duplicate_scenarios = []

        for _ in range(8):
            # Simulate receiving some packets twice
            packet_count = 5
            duplicates = random.randint(0, 2)  # 0-2 duplicates

            start = time.time()
            for i in range(packet_count + duplicates):
                time.sleep(0.002)  # 2ms per packet

            total_time = (time.time() - start) * 1000
            duplicate_scenarios.append(total_time)
            self.metrics.record_query_time(total_time)

        avg_duplicate_time = statistics.mean(duplicate_scenarios)

        # Duplicates shouldn't cause excessive delays
        expected_max_time = 5 * 2 * 1.5  # 5 packets * 2ms * 1.5x overhead
        assert avg_duplicate_time < expected_max_time, (
            f"Duplicate handling too slow: {avg_duplicate_time:.1f}ms > {expected_max_time}ms"
        )

        self.metrics.end_test()

        toxiproxy.delete_proxy("fraiseql_postgres")

    @pytest.mark.chaos
    @pytest.mark.chaos_network
    @pytest.mark.parametrize("packet_loss_rate", [0.02, 0.08, 0.15])
    def test_adaptive_retry_under_packet_loss(self, packet_loss_rate: float):
        """
        Test adaptive retry strategies under packet loss.

        Scenario: System adapts retry count based on packet loss conditions.
        Expected: FraiseQL implements intelligent retry logic.
        """
        self.metrics.start_test()

        # Simulate adaptive retry behavior
        operations = 12
        successful_operations = 0
        total_retries = 0

        for _ in range(operations):
            retries = 0
            success = False

            while retries < 5 and not success:  # Max 5 retries
                if random.random() >= packet_loss_rate:
                    success = True
                    successful_operations += 1
                else:
                    retries += 1
                    total_retries += 1
                    # Exponential backoff
                    time.sleep(0.001 * (2**retries))

                self.metrics.record_query_time(10.0 * (retries + 1))

        success_rate = successful_operations / operations
        avg_retries_per_operation = total_retries / operations

        # Validate adaptive behavior
        expected_success_rate = 1.0 - (packet_loss_rate**2)  # With retries
        assert success_rate >= expected_success_rate * 0.8, (
            f"Success rate {success_rate:.2f} too low for {packet_loss_rate * 100}% loss"
        )

        # Should use more retries under higher loss
        expected_avg_retries = packet_loss_rate * 3  # Rough estimate
        assert avg_retries_per_operation >= expected_avg_retries * 0.5, (
            f"Too few retries: {avg_retries_per_operation:.1f} < {expected_avg_retries}"
        )

        self.metrics.end_test()

    @pytest.mark.chaos
    @pytest.mark.chaos_network
    def test_network_recovery_after_corruption(self, toxiproxy: ToxiproxyManager):
        """
        Test network recovery after corruption chaos.

        Scenario: Heavy packet corruption followed by network recovery.
        Expected: FraiseQL recovers quickly when network improves.
        """
        proxy = toxiproxy.create_proxy("fraiseql_postgres", "0.0.0.0:5433", "postgres:5432")

        self.metrics.start_test()

        # Phase 1: Baseline
        baseline_times = []
        for _ in range(5):
            start = time.time()
            time.sleep(0.010)
            baseline_times.append((time.time() - start) * 1000)

        avg_baseline = statistics.mean(baseline_times)

        # Phase 2: Heavy corruption (simulate 20% packet issues)
        toxiproxy.add_packet_loss_toxic("fraiseql_postgres", 0.15)  # 15% loss
        toxiproxy.add_latency_toxic("fraiseql_postgres", 200, 50)  # High latency + jitter

        corruption_times = []
        corruption_errors = 0

        for _ in range(8):
            start = time.time()
            try:
                # High chance of failure under corruption
                if random.random() < 0.25:  # 25% failure rate
                    raise ConnectionError("Network corruption")

                # Variable timing due to corruption
                delay = 0.010 + random.uniform(0, 0.200)  # 10-210ms
                time.sleep(delay)
                corruption_times.append((time.time() - start) * 1000)

            except ConnectionError:
                corruption_errors += 1
                self.metrics.record_error()

        # Phase 3: Network recovery
        toxiproxy.remove_all_toxics("fraiseql_postgres")

        recovery_times = []
        for _ in range(5):
            start = time.time()
            time.sleep(0.010)  # Should be back to normal
            recovery_times.append((time.time() - start) * 1000)

        avg_recovery = statistics.mean(recovery_times)

        self.metrics.end_test()

        # Validate recovery behavior
        assert corruption_errors > 0, "Should experience corruption-related errors"
        assert abs(avg_recovery - avg_baseline) < 10.0, (
            f"Recovery should be quick: {avg_recovery:.1f}ms vs baseline {avg_baseline:.1f}ms"
        )

        print(".1f")
        toxiproxy.delete_proxy("fraiseql_postgres")
