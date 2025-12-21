"""
Phase 0 Verification Test

This test verifies that the Phase 0 chaos engineering infrastructure is working correctly.
"""

import sys
import os

sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from chaos.base import ChaosTestCase
from chaos.plugin import chaos_inject, FailureType


class TestPhase0Infrastructure(ChaosTestCase):
    """Test that Phase 0 infrastructure works."""

    def test_baseline_loading(self):
        """Test that baselines can be loaded."""
        baselines = self.load_baseline()
        assert isinstance(baselines, dict)
        assert len(baselines) > 0
        assert "simple_query" in baselines

    def test_metrics_collection(self):
        """Test that metrics collection works."""
        self.metrics.start_test()
        self.metrics.record_query_time(15.0)
        self.metrics.record_error()
        self.metrics.end_test()

        summary = self.metrics.get_summary()
        assert summary["query_count"] == 1
        assert summary["error_count"] == 1
        assert summary["avg_query_time_ms"] == 15.0

    def test_baseline_comparison(self):
        """Test baseline comparison functionality."""
        # Simulate a test result
        self.metrics.start_test()
        self.metrics.record_query_time(15.0)  # Same as baseline
        self.metrics.end_test()

        comparison = self.compare_to_baseline("simple_query")
        assert "current" in comparison
        assert "baseline" in comparison

    @chaos_inject(FailureType.NETWORK_LATENCY, duration_ms=100)
    def test_chaos_decorator(self):
        """Test that chaos injection decorator works."""
        # This test should have chaos injection metadata
        pass


def test_chaos_injector_creation():
    """Test that chaos injector can be created."""
    from chaos.plugin import _chaos_injector

    assert hasattr(_chaos_injector, "inject_failure")
    assert hasattr(_chaos_injector, "is_failure_active")


if __name__ == "__main__":
    # Run basic smoke tests
    test = TestPhase0Infrastructure()

    print("🧪 Running Phase 0 verification tests...")

    try:
        test.test_baseline_loading()
        print("✅ Baseline loading test passed")

        test.test_metrics_collection()
        print("✅ Metrics collection test passed")

        test.test_baseline_comparison()
        print("✅ Baseline comparison test passed")

        test_chaos_injector_creation()
        print("✅ Chaos injector test passed")

        print("\n🎉 All Phase 0 verification tests passed!")
        print("🚀 Chaos engineering infrastructure is ready!")

    except Exception as e:
        print(f"\n❌ Phase 0 verification failed: {e}")
        sys.exit(1)
