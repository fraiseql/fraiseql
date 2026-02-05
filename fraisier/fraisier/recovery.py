"""Recovery strategies for handling deployment errors.

Provides pluggable strategies for automatic error recovery:
- Retry with exponential backoff
- Fallback to alternative provider
- Automatic rollback on failure
- Circuit breaker pattern
"""

import time
from abc import ABC, abstractmethod
from collections.abc import Callable
from typing import Any

from .errors import FraisierError


class RecoveryStrategy(ABC):
    """Base recovery strategy for errors."""

    @abstractmethod
    def can_recover(self, error: Exception) -> bool:
        """Check if this strategy can handle the error.

        Args:
            error: Exception to evaluate

        Returns:
            True if this strategy can recover from the error
        """

    @abstractmethod
    def execute_recovery(self, context: dict[str, Any]) -> bool:
        """Execute recovery procedure.

        Args:
            context: Error context with:
                - error: The exception
                - operation: Name of failed operation
                - provider: Provider name (if applicable)
                - attempt: Current attempt number
                - deployment_id: Deployment ID (if applicable)

        Returns:
            True if recovery succeeded, False otherwise
        """


class RetryStrategy(RecoveryStrategy):
    """Retry failed operations with exponential backoff.

    Usage:
        strategy = RetryStrategy(max_attempts=3, base_delay=1.0, backoff_factor=2.0)
        if strategy.can_recover(error):
            success = strategy.execute_recovery(context)
    """

    def __init__(
        self,
        max_attempts: int = 3,
        base_delay: float = 1.0,
        backoff_factor: float = 2.0,
        max_delay: float = 60.0,
    ):
        """Initialize retry strategy.

        Args:
            max_attempts: Maximum number of retry attempts
            base_delay: Initial delay in seconds
            backoff_factor: Multiplier for exponential backoff
            max_delay: Maximum delay between retries
        """
        self.max_attempts = max_attempts
        self.base_delay = base_delay
        self.backoff_factor = backoff_factor
        self.max_delay = max_delay

    def can_recover(self, error: Exception) -> bool:
        """Retry recoverable errors."""
        if isinstance(error, FraisierError):
            return error.recoverable
        return False

    def execute_recovery(self, context: dict[str, Any]) -> bool:
        """Execute retry with backoff.

        Note: This returns a decision, actual retry is handled by caller.
        """
        attempt = context.get("attempt", 1)
        if attempt >= self.max_attempts:
            return False

        # Calculate delay with exponential backoff
        delay = min(
            self.base_delay * (self.backoff_factor ** (attempt - 1)),
            self.max_delay,
        )

        # Wait before retry
        time.sleep(delay)
        return True


class FallbackStrategy(RecoveryStrategy):
    """Fallback to alternative provider on failure.

    Usage:
        strategy = FallbackStrategy(fallback_provider="docker_compose")
        if strategy.can_recover(error):
            success = strategy.execute_recovery(context)
    """

    def __init__(self, fallback_provider: str):
        """Initialize fallback strategy.

        Args:
            fallback_provider: Provider type to fallback to
        """
        self.fallback_provider = fallback_provider

    def can_recover(self, error: Exception) -> bool:
        """Fallback on provider errors."""
        from .errors import ProviderError

        return isinstance(error, ProviderError)

    def execute_recovery(self, context: dict[str, Any]) -> bool:
        """Return fallback provider decision.

        Note: Actual provider switching handled by caller.
        """
        current_provider = context.get("provider")
        return current_provider != self.fallback_provider


class RollbackRecoveryStrategy(RecoveryStrategy):
    """Automatic rollback on deployment failure.

    Usage:
        strategy = RollbackRecoveryStrategy(
            rollback_on_timeout=True,
            rollback_on_health_check_failure=True
        )
        if strategy.can_recover(error):
            success = strategy.execute_recovery(context)
    """

    def __init__(
        self,
        rollback_on_timeout: bool = True,
        rollback_on_health_check_failure: bool = True,
        rollback_on_deployment_error: bool = False,
    ):
        """Initialize rollback strategy.

        Args:
            rollback_on_timeout: Rollback if deployment times out
            rollback_on_health_check_failure: Rollback if health check fails
            rollback_on_deployment_error: Rollback on any deployment error
        """
        self.rollback_on_timeout = rollback_on_timeout
        self.rollback_on_health_check_failure = rollback_on_health_check_failure
        self.rollback_on_deployment_error = rollback_on_deployment_error

    def can_recover(self, error: Exception) -> bool:
        """Determine if rollback is appropriate for error."""
        from .errors import DeploymentTimeoutError, HealthCheckError

        if self.rollback_on_timeout and isinstance(error, DeploymentTimeoutError):
            return True

        if (
            self.rollback_on_health_check_failure
            and isinstance(error, HealthCheckError)
        ):
            return True

        if (
            self.rollback_on_deployment_error
            and isinstance(error, FraisierError)
        ):
            return True

        return False

    def execute_recovery(self, context: dict[str, Any]) -> bool:
        """Decide whether to rollback.

        Note: Actual rollback execution handled by caller.
        """
        # Check if we have version to rollback to
        old_version = context.get("old_version")
        return old_version is not None


class CircuitBreakerStrategy(RecoveryStrategy):
    """Circuit breaker to prevent cascading failures.

    Tracks failures and temporarily disables provider after threshold.

    Usage:
        strategy = CircuitBreakerStrategy(
            failure_threshold=5,
            recovery_timeout=300
        )
        if strategy.should_allow_operation(provider):
            try:
                result = operation()
            except Exception as e:
                strategy.record_failure(provider, e)
    """

    def __init__(self, failure_threshold: int = 5, recovery_timeout: float = 300.0):
        """Initialize circuit breaker.

        Args:
            failure_threshold: Failures before opening circuit
            recovery_timeout: Seconds to wait before attempting recovery
        """
        self.failure_threshold = failure_threshold
        self.recovery_timeout = recovery_timeout
        self.failure_counts: dict[str, int] = {}
        self.open_times: dict[str, float] = {}

    def should_allow_operation(self, resource: str) -> bool:
        """Check if operation on resource should be allowed.

        Args:
            resource: Provider, database, or other resource name

        Returns:
            True if operation should proceed
        """
        # Check if circuit is open
        if resource in self.open_times:
            time_since_open = time.time() - self.open_times[resource]
            if time_since_open >= self.recovery_timeout:
                # Try to recover
                del self.open_times[resource]
                self.failure_counts[resource] = 0
                return True
            else:
                # Circuit still open
                return False

        return True

    def record_failure(self, resource: str) -> None:
        """Record failure for resource.

        Args:
            resource: Resource that failed
        """
        self.failure_counts[resource] = self.failure_counts.get(resource, 0) + 1

        if self.failure_counts[resource] >= self.failure_threshold:
            # Open circuit
            self.open_times[resource] = time.time()

    def record_success(self, resource: str) -> None:
        """Record success for resource.

        Args:
            resource: Resource that succeeded
        """
        self.failure_counts[resource] = 0

    def can_recover(self, error: Exception) -> bool:
        """Circuit breaker returns no recovery (it prevents operations)."""
        return False

    def execute_recovery(self, context: dict[str, Any]) -> bool:
        """Circuit breaker has no recovery execution."""
        return False


class RetryableOperation:
    """Wrapper for operations that can be retried.

    Usage:
        op = RetryableOperation(
            func=deploy,
            args=(provider, service, version),
            retry_strategy=RetryStrategy(max_attempts=3),
            on_retry=lambda: print("Retrying...")
        )
        result = op.execute()
    """

    def __init__(
        self,
        func: Callable,
        args: tuple = (),
        kwargs: dict | None = None,
        retry_strategy: RecoveryStrategy | None = None,
        on_retry: Callable | None = None,
        max_attempts: int = 3,
    ):
        """Initialize retryable operation.

        Args:
            func: Function to execute
            args: Positional arguments
            kwargs: Keyword arguments
            retry_strategy: Recovery strategy to use
            on_retry: Callback before each retry
            max_attempts: Maximum attempts
        """
        self.func = func
        self.args = args
        self.kwargs = kwargs or {}
        self.retry_strategy = retry_strategy or RetryStrategy(max_attempts)
        self.on_retry = on_retry
        self.max_attempts = max_attempts

    def execute(self) -> Any:
        """Execute operation with retries.

        Returns:
            Result from func

        Raises:
            Last exception if all retries exhausted
        """
        last_error = None

        for attempt in range(1, self.max_attempts + 1):
            try:
                return self.func(*self.args, **self.kwargs)
            except Exception as e:
                last_error = e

                context = {
                    "attempt": attempt,
                    "error": e,
                    "operation": self.func.__name__,
                }

                # Check if recovery is possible
                if not self.retry_strategy.can_recover(e):
                    raise

                # Check if we should retry
                if attempt >= self.max_attempts:
                    raise

                # Attempt recovery
                if self.on_retry:
                    self.on_retry()

                if not self.retry_strategy.execute_recovery(context):
                    raise

        # Should not reach here
        if last_error:
            raise last_error

        raise RuntimeError("Unexpected: retryable operation failed without exception")
