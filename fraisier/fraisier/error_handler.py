"""Centralized error handling with recovery strategies.

Manages error handling, recovery attempts, and error reporting
across all Fraisier operations.
"""

import logging
from typing import Any, TypeVar

from .errors import FraisierError
from .recovery import RecoveryStrategy

T = TypeVar("T")


class ErrorHandler:
    """Centralized error handling with recovery strategies.

    Manages:
    - Error classification and routing
    - Recovery strategy selection
    - Retry decisions
    - Error logging and metrics
    """

    def __init__(self, logger: logging.Logger | None = None):
        """Initialize error handler.

        Args:
            logger: Logger instance for error reporting
        """
        self.logger = logger or logging.getLogger(__name__)
        self.recovery_strategies: dict[str, list[RecoveryStrategy]] = {}
        self.error_counts: dict[str, int] = {}
        self.error_history: list[dict[str, Any]] = []

    def register_strategy(
        self, error_type: str, strategy: RecoveryStrategy
    ) -> None:
        """Register recovery strategy for error type.

        Strategies are tried in registration order.

        Args:
            error_type: Exception class name or error code
            strategy: Recovery strategy to use
        """
        if error_type not in self.recovery_strategies:
            self.recovery_strategies[error_type] = []

        self.recovery_strategies[error_type].append(strategy)

    def handle_error(
        self,
        error: Exception,
        context: dict[str, Any] | None = None,
        reraise: bool = True,
    ) -> bool:
        """Handle error with recovery attempt.

        Args:
            error: Exception to handle
            context: Error context
            reraise: Whether to raise if recovery fails

        Returns:
            True if recovered, False if not

        Raises:
            Exception if recovery fails and reraise=True
        """
        context = context or {}
        context["error"] = error

        # Track error
        self._track_error(error)

        # Log error
        self._log_error(error, context)

        # Attempt recovery
        recovered = self._attempt_recovery(error, context)

        if not recovered and reraise:
            raise

        return recovered

    def should_retry(self, error: Exception, attempt: int) -> bool:
        """Determine if operation should be retried.

        Args:
            error: Exception that occurred
            attempt: Current attempt number

        Returns:
            True if operation should be retried
        """
        if not isinstance(error, FraisierError):
            return False

        if not error.recoverable:
            return False

        # Max 3 attempts by default
        return attempt < 3

    def _attempt_recovery(
        self, error: Exception, context: dict[str, Any]
    ) -> bool:
        """Attempt recovery using registered strategies.

        Args:
            error: Exception to recover from
            context: Error context

        Returns:
            True if recovery succeeded
        """
        # Get strategies for this error type
        strategies = self._get_strategies(error)

        for strategy in strategies:
            try:
                if strategy.can_recover(error):
                    if strategy.execute_recovery(context):
                        self.logger.info(
                            f"Recovered from {error.__class__.__name__} "
                            f"using {strategy.__class__.__name__}",
                            extra={"context": context},
                        )
                        return True
            except Exception as recovery_error:
                self.logger.warning(
                    f"Recovery strategy {strategy.__class__.__name__} failed: "
                    f"{recovery_error}",
                    extra={"context": context},
                )

        return False

    def _get_strategies(self, error: Exception) -> list[RecoveryStrategy]:
        """Get applicable recovery strategies for error.

        Args:
            error: Exception to find strategies for

        Returns:
            List of applicable strategies
        """
        strategies = []

        # Try by exception class name
        error_type = error.__class__.__name__
        if error_type in self.recovery_strategies:
            strategies.extend(self.recovery_strategies[error_type])

        # Try by error code if FraisierError
        if isinstance(error, FraisierError):
            error_code = error.code
            if error_code in self.recovery_strategies:
                strategies.extend(self.recovery_strategies[error_code])

        return strategies

    def _track_error(self, error: Exception) -> None:
        """Track error statistics.

        Args:
            error: Exception that occurred
        """
        error_type = error.__class__.__name__
        self.error_counts[error_type] = self.error_counts.get(error_type, 0) + 1

        # Keep history (limited to recent 1000)
        history_entry = {
            "error_type": error_type,
            "message": str(error),
            "timestamp": __import__("time").time(),
        }

        self.error_history.append(history_entry)
        if len(self.error_history) > 1000:
            self.error_history.pop(0)

    def _log_error(self, error: Exception, context: dict[str, Any]) -> None:
        """Log error with context.

        Args:
            error: Exception that occurred
            context: Error context
        """
        error_dict = {}
        if isinstance(error, FraisierError):
            error_dict = error.to_dict()
        else:
            error_dict = {
                "error_type": error.__class__.__name__,
                "message": str(error),
            }

        self.logger.error(
            f"Error: {error_dict['error_type']}",
            extra={"error": error_dict, "context": context},
            exc_info=True,
        )

    def get_error_stats(self) -> dict[str, Any]:
        """Get error statistics.

        Returns:
            Dict with error counts and trends
        """
        return {
            "total_errors": sum(self.error_counts.values()),
            "errors_by_type": self.error_counts.copy(),
            "recent_errors": self.error_history[-10:] if self.error_history else [],
        }

    def reset_stats(self) -> None:
        """Reset error statistics."""
        self.error_counts.clear()
        self.error_history.clear()


class ContextualErrorHandler:
    """Error handler with context accumulation.

    Builds up context as it processes errors, useful for
    correlating related errors.

    Usage:
        handler = ContextualErrorHandler()
        with handler.context(deployment_id="deploy-123", service="api"):
            try:
                deploy()
            except Exception as e:
                handler.handle_error(e)
    """

    def __init__(self, base_handler: ErrorHandler | None = None):
        """Initialize contextual handler.

        Args:
            base_handler: Base error handler to delegate to
        """
        self.base_handler = base_handler or ErrorHandler()
        self._context_stack: list[dict[str, Any]] = []

    def context(self, **kwargs) -> "ContextManager":
        """Enter context with additional error context.

        Args:
            **kwargs: Context variables

        Returns:
            Context manager
        """
        return ContextManager(self, kwargs)

    def get_context(self) -> dict[str, Any]:
        """Get accumulated context.

        Returns:
            Merged context from all active scopes
        """
        merged = {}
        for ctx in self._context_stack:
            merged.update(ctx)
        return merged

    def handle_error(
        self,
        error: Exception,
        additional_context: dict[str, Any] | None = None,
    ) -> bool:
        """Handle error with accumulated context.

        Args:
            error: Exception to handle
            additional_context: Extra context to add

        Returns:
            True if recovered
        """
        context = self.get_context()
        if additional_context:
            context.update(additional_context)

        return self.base_handler.handle_error(error, context)


class ContextManager:
    """Context manager for error handling context."""

    def __init__(self, handler: ContextualErrorHandler, context: dict[str, Any]):
        """Initialize context manager.

        Args:
            handler: Contextual handler
            context: Context dict
        """
        self.handler = handler
        self.context = context

    def __enter__(self):
        """Enter context."""
        self.handler._context_stack.append(self.context)
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Exit context."""
        self.handler._context_stack.pop()
        return False  # Don't suppress exceptions
