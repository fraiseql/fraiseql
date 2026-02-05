"""Custom exception hierarchy for Fraisier.

Provides structured error types for different failure scenarios with:
- Error codes for programmatic handling
- Context preservation for debugging
- Recoverable flag for automated recovery
- Hierarchical structure for flexible exception handling
"""

from typing import Any


class FraisierError(Exception):
    """Base exception for all Fraisier errors.

    All Fraisier errors inherit from this and provide:
    - Standard error code for identification
    - Optional context dict for debugging
    - Recoverable flag for automated recovery
    """

    code: str = "FRAISIER_ERROR"
    recoverable: bool = False

    def __init__(
        self,
        message: str,
        code: str | None = None,
        context: dict[str, Any] | None = None,
        recoverable: bool | None = None,
        cause: Exception | None = None,
    ):
        """Initialize Fraisier error.

        Args:
            message: Human-readable error message
            code: Machine-readable error code (defaults to class code)
            context: Additional context dict for debugging
            recoverable: Whether error can be automatically recovered from
            cause: Original exception that caused this error
        """
        self.message = message
        self.code = code or self.__class__.code
        self.context = context or {}
        if recoverable is not None:
            self.recoverable = recoverable
        self.cause = cause

        # Include cause in message if present
        msg = message
        if cause:
            msg = f"{message} (caused by {type(cause).__name__}: {str(cause)})"

        super().__init__(msg)

    def to_dict(self) -> dict[str, Any]:
        """Serialize error to dict for logging/API responses."""
        return {
            "error_type": self.__class__.__name__,
            "code": self.code,
            "message": self.message,
            "context": self.context,
            "recoverable": self.recoverable,
        }


class ConfigurationError(FraisierError):
    """Configuration loading or validation errors."""

    code = "CONFIG_ERROR"


class DeploymentError(FraisierError):
    """Deployment execution errors."""

    code = "DEPLOYMENT_ERROR"


class DeploymentTimeoutError(DeploymentError):
    """Deployment operation timed out."""

    code = "DEPLOYMENT_TIMEOUT"
    recoverable = True  # Can retry with longer timeout


class HealthCheckError(DeploymentError):
    """Health check failed after deployment."""

    code = "HEALTH_CHECK_FAILED"
    recoverable = True  # Can retry or fallback


class ProviderError(FraisierError):
    """Provider-related errors."""

    code = "PROVIDER_ERROR"


class ProviderConnectionError(ProviderError):
    """Failed to connect to provider."""

    code = "PROVIDER_CONNECTION_ERROR"
    recoverable = True  # Provider may become available


class ProviderUnavailableError(ProviderError):
    """Provider is temporarily unavailable."""

    code = "PROVIDER_UNAVAILABLE"
    recoverable = True  # Provider may become available


class ProviderConfigurationError(ProviderError):
    """Provider configuration is invalid."""

    code = "PROVIDER_CONFIG_ERROR"


class RollbackError(DeploymentError):
    """Rollback operation failed."""

    code = "ROLLBACK_FAILED"


class DatabaseError(FraisierError):
    """Database operation errors."""

    code = "DATABASE_ERROR"


class DatabaseConnectionError(DatabaseError):
    """Failed to connect to database."""

    code = "DATABASE_CONNECTION_ERROR"
    recoverable = True  # Database may become available


class DatabaseTransactionError(DatabaseError):
    """Database transaction failed."""

    code = "DATABASE_TRANSACTION_ERROR"
    recoverable = True  # Transaction may succeed on retry


class DeploymentLockError(DeploymentError):
    """Deployment lock acquisition failed."""

    code = "DEPLOYMENT_LOCKED"
    recoverable = True  # Lock will eventually be released


class NotFoundError(FraisierError):
    """Requested resource not found."""

    code = "NOT_FOUND"


class ValidationError(FraisierError):
    """Input validation failed."""

    code = "VALIDATION_ERROR"


class GitProviderError(FraisierError):
    """Git provider related errors."""

    code = "GIT_PROVIDER_ERROR"


class WebhookError(FraisierError):
    """Webhook processing errors."""

    code = "WEBHOOK_ERROR"
