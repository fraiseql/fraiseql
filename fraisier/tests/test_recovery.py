"""Tests for recovery strategies."""

import time

import pytest

from fraisier.errors import (
    DeploymentTimeoutError,
    FraisierError,
    HealthCheckError,
    ProviderError,
)
from fraisier.recovery import (
    CircuitBreakerStrategy,
    FallbackStrategy,
    RetryStrategy,
    RetryableOperation,
    RollbackRecoveryStrategy,
)


class TestRetryStrategy:
    """Test retry strategy with exponential backoff."""

    def test_can_recover_recoverable_error(self):
        """Test can recover from recoverable error."""
        strategy = RetryStrategy()
        error = FraisierError("Test", recoverable=True)
        assert strategy.can_recover(error) is True

    def test_cannot_recover_non_recoverable_error(self):
        """Test cannot recover from non-recoverable error."""
        strategy = RetryStrategy()
        error = FraisierError("Test", recoverable=False)
        assert strategy.can_recover(error) is False

    def test_cannot_recover_non_fraisier_error(self):
        """Test cannot recover from non-FraisierError."""
        strategy = RetryStrategy()
        error = ValueError("Not a FraisierError")
        assert strategy.can_recover(error) is False

    def test_retry_below_max_attempts(self):
        """Test retry is allowed below max attempts."""
        strategy = RetryStrategy(max_attempts=3)
        context = {"attempt": 1}
        assert strategy.execute_recovery(context) is True

    def test_retry_at_max_attempts(self):
        """Test retry fails at max attempts."""
        strategy = RetryStrategy(max_attempts=3)
        context = {"attempt": 3}
        assert strategy.execute_recovery(context) is False

    def test_exponential_backoff(self):
        """Test exponential backoff calculation."""
        strategy = RetryStrategy(
            max_attempts=5,
            base_delay=0.1,
            backoff_factor=2.0,
            max_delay=5.0,
        )

        # Test delay increases exponentially
        delays = []
        for attempt in range(1, 4):
            start = time.time()
            context = {"attempt": attempt}
            strategy.execute_recovery(context)
            elapsed = time.time() - start
            delays.append(elapsed)

        # Each delay should be roughly 2x the previous (with tolerance)
        assert delays[1] > delays[0]  # Second > first
        assert delays[2] > delays[1]  # Third > second

    def test_backoff_respects_max_delay(self):
        """Test backoff respects maximum delay."""
        strategy = RetryStrategy(
            max_attempts=10,
            base_delay=1.0,
            backoff_factor=10.0,
            max_delay=2.0,
        )

        # High attempt number would cause very long delay without max
        start = time.time()
        context = {"attempt": 5}
        strategy.execute_recovery(context)
        elapsed = time.time() - start

        # Should respect max_delay
        assert elapsed <= 2.5  # Some tolerance


class TestFallbackStrategy:
    """Test fallback provider strategy."""

    def test_can_recover_provider_error(self):
        """Test can recover from ProviderError."""
        strategy = FallbackStrategy(fallback_provider="docker_compose")
        error = ProviderError("Provider failed")
        assert strategy.can_recover(error) is True

    def test_cannot_recover_other_errors(self):
        """Test cannot recover from non-provider errors."""
        strategy = FallbackStrategy(fallback_provider="docker_compose")
        error = DeploymentTimeoutError("Timeout")
        assert strategy.can_recover(error) is False

    def test_fallback_to_different_provider(self):
        """Test fallback when current provider is different."""
        strategy = FallbackStrategy(fallback_provider="docker_compose")
        context = {"provider": "bare_metal"}
        assert strategy.execute_recovery(context) is True

    def test_no_fallback_to_same_provider(self):
        """Test no fallback when already on fallback provider."""
        strategy = FallbackStrategy(fallback_provider="docker_compose")
        context = {"provider": "docker_compose"}
        assert strategy.execute_recovery(context) is False


class TestRollbackRecoveryStrategy:
    """Test automatic rollback strategy."""

    def test_can_recover_timeout_error(self):
        """Test can recover from timeout with rollback."""
        strategy = RollbackRecoveryStrategy(rollback_on_timeout=True)
        error = DeploymentTimeoutError("Timeout")
        assert strategy.can_recover(error) is True

    def test_can_recover_health_check_failure(self):
        """Test can recover from health check failure."""
        strategy = RollbackRecoveryStrategy(
            rollback_on_health_check_failure=True
        )
        error = HealthCheckError("Health check failed")
        assert strategy.can_recover(error) is True

    def test_cannot_recover_if_rollback_disabled(self):
        """Test cannot recover if rollback disabled."""
        strategy = RollbackRecoveryStrategy(rollback_on_timeout=False)
        error = DeploymentTimeoutError("Timeout")
        assert strategy.can_recover(error) is False

    def test_rollback_succeeds_with_old_version(self):
        """Test rollback succeeds when old version available."""
        strategy = RollbackRecoveryStrategy()
        context = {"old_version": "1.0.0"}
        assert strategy.execute_recovery(context) is True

    def test_rollback_fails_without_old_version(self):
        """Test rollback fails when no old version."""
        strategy = RollbackRecoveryStrategy()
        context = {"old_version": None}
        assert strategy.execute_recovery(context) is False


class TestCircuitBreakerStrategy:
    """Test circuit breaker strategy."""

    def test_allows_operation_initially(self):
        """Test circuit allows operation initially."""
        strategy = CircuitBreakerStrategy(failure_threshold=3)
        assert strategy.should_allow_operation("provider1") is True

    def test_opens_circuit_after_failures(self):
        """Test circuit opens after threshold failures."""
        strategy = CircuitBreakerStrategy(failure_threshold=2)

        # Record failures
        strategy.record_failure("provider1")
        strategy.record_failure("provider1")

        # Circuit should be open
        assert strategy.should_allow_operation("provider1") is False

    def test_allows_success_to_reset_failures(self):
        """Test success resets failure count."""
        strategy = CircuitBreakerStrategy(failure_threshold=2)

        # Record some failures
        strategy.record_failure("provider1")
        assert strategy.failure_counts["provider1"] == 1

        # Record success
        strategy.record_success("provider1")
        assert strategy.failure_counts["provider1"] == 0

    def test_circuit_recovers_after_timeout(self):
        """Test circuit recovers after timeout period."""
        strategy = CircuitBreakerStrategy(
            failure_threshold=1,
            recovery_timeout=0.1,
        )

        # Open circuit
        strategy.record_failure("provider1")
        assert strategy.should_allow_operation("provider1") is False

        # Wait for recovery timeout
        time.sleep(0.15)

        # Circuit should recover
        assert strategy.should_allow_operation("provider1") is True

    def test_multiple_resources_independent(self):
        """Test different resources have independent circuit states."""
        strategy = CircuitBreakerStrategy(failure_threshold=1)

        # Fail provider1
        strategy.record_failure("provider1")
        assert strategy.should_allow_operation("provider1") is False

        # provider2 should still work
        assert strategy.should_allow_operation("provider2") is True

    def test_can_recover_returns_false(self):
        """Test CircuitBreakerStrategy can_recover always returns False."""
        strategy = CircuitBreakerStrategy()
        error = ProviderError("Test")
        assert strategy.can_recover(error) is False

    def test_execute_recovery_returns_false(self):
        """Test CircuitBreakerStrategy execute_recovery always returns False."""
        strategy = CircuitBreakerStrategy()
        assert strategy.execute_recovery({}) is False


class TestRetryableOperation:
    """Test retryable operation wrapper."""

    def test_successful_operation(self):
        """Test successful operation executes once."""
        call_count = {"count": 0}

        def operation():
            call_count["count"] += 1
            return "success"

        op = RetryableOperation(operation, max_attempts=3)
        result = op.execute()

        assert result == "success"
        assert call_count["count"] == 1

    def test_operation_retries_on_failure(self):
        """Test operation retries on recoverable error."""
        call_count = {"count": 0}

        def operation():
            call_count["count"] += 1
            if call_count["count"] < 3:
                raise FraisierError("Recoverable", recoverable=True)
            return "success"

        strategy = RetryStrategy(max_attempts=3)
        op = RetryableOperation(
            operation,
            max_attempts=3,
            retry_strategy=strategy,
        )
        result = op.execute()

        assert result == "success"
        assert call_count["count"] == 3

    def test_operation_fails_on_non_recoverable(self):
        """Test operation fails on non-recoverable error."""
        def operation():
            raise FraisierError("Non-recoverable", recoverable=False)

        strategy = RetryStrategy(max_attempts=3)
        op = RetryableOperation(
            operation,
            max_attempts=3,
            retry_strategy=strategy,
        )

        with pytest.raises(FraisierError):
            op.execute()

    def test_operation_exhausts_retries(self):
        """Test operation fails after exhausting retries."""
        def operation():
            raise FraisierError("Always fails", recoverable=True)

        strategy = RetryStrategy(max_attempts=2)
        op = RetryableOperation(
            operation,
            max_attempts=2,
            retry_strategy=strategy,
        )

        with pytest.raises(FraisierError):
            op.execute()

    def test_operation_with_args_and_kwargs(self):
        """Test operation with arguments."""
        def operation(a, b, c=None):
            return f"{a}-{b}-{c}"

        op = RetryableOperation(
            operation,
            args=("x", "y"),
            kwargs={"c": "z"},
            max_attempts=1,
        )
        result = op.execute()

        assert result == "x-y-z"

    def test_on_retry_callback(self):
        """Test on_retry callback is called."""
        call_count = {"count": 0, "retries": 0}

        def operation():
            call_count["count"] += 1
            if call_count["count"] < 3:
                raise FraisierError("Recoverable", recoverable=True)
            return "success"

        def on_retry():
            call_count["retries"] += 1

        strategy = RetryStrategy(max_attempts=3)
        op = RetryableOperation(
            operation,
            max_attempts=3,
            retry_strategy=strategy,
            on_retry=on_retry,
        )
        op.execute()

        # Should retry twice (call_count = 1, 2, success)
        assert call_count["retries"] == 2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
