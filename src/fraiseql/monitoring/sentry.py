"""Sentry error tracking integration for FraiseQL.

Provides enterprise-grade error tracking with automatic context capture,
performance monitoring, and release tracking.

Example:
    >>> from fraiseql.monitoring.sentry import init_sentry
    >>>
    >>> # Initialize in your FastAPI app
    >>> app = FastAPI()
    >>> init_sentry(
    ...     dsn="https://...@sentry.io/...",
    ...     environment="production",
    ...     traces_sample_rate=0.1
    ... )
"""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)

__all__ = [
    "capture_exception",
    "capture_message",
    "init_sentry",
    "set_context",
    "set_user",
]


def init_sentry(
    dsn: str | None = None,
    environment: str = "production",
    traces_sample_rate: float = 0.1,
    profiles_sample_rate: float = 0.1,
    release: str | None = None,
    server_name: str | None = None,
    **kwargs: Any,
) -> bool:
    """Initialize Sentry error tracking and performance monitoring.

    Args:
        dsn: Sentry DSN (Data Source Name). If None, Sentry is disabled.
        environment: Deployment environment (production, staging, development)
        traces_sample_rate: Percentage of traces to capture (0.0-1.0)
        profiles_sample_rate: Percentage of profiles to capture (0.0-1.0)
        release: Release version (e.g., "fraiseql@0.11.0")
        server_name: Server/instance name for grouping
        **kwargs: Additional Sentry SDK options

    Returns:
        bool: True if Sentry was initialized successfully, False otherwise

    Example:
        >>> init_sentry(
        ...     dsn=os.getenv("SENTRY_DSN"),
        ...     environment="production",
        ...     traces_sample_rate=0.1,
        ...     release="fraiseql@0.11.0"
        ... )
    """
    if not dsn:
        logger.info("Sentry DSN not provided - error tracking disabled")
        return False

    try:
        import sentry_sdk
        from sentry_sdk.integrations.fastapi import FastApiIntegration
        from sentry_sdk.integrations.logging import LoggingIntegration
        from sentry_sdk.integrations.sqlalchemy import SqlalchemyIntegration

        # Logging integration - capture ERROR and above
        sentry_logging = LoggingIntegration(
            level=logging.INFO,  # Breadcrumbs from INFO
            event_level=logging.ERROR,  # Errors from ERROR
        )

        sentry_sdk.init(
            dsn=dsn,
            environment=environment,
            traces_sample_rate=traces_sample_rate,
            profiles_sample_rate=profiles_sample_rate,
            release=release,
            server_name=server_name,
            integrations=[
                FastApiIntegration(transaction_style="endpoint"),
                sentry_logging,
                SqlalchemyIntegration(),
            ],
            # Capture request bodies for POST requests
            max_request_body_size="medium",  # Or "always", "never", "small", "large"
            # Send default PII (user IP, cookies, etc.)
            send_default_pii=True,
            # Add custom tags
            default_integrations=True,
            # Performance monitoring
            enable_tracing=True,
            **kwargs,
        )

        logger.info(
            f"Sentry initialized successfully - environment: {environment}, "
            f"traces_sample_rate: {traces_sample_rate}"
        )
        return True

    except ImportError:
        logger.warning(
            "sentry-sdk not installed - error tracking disabled. "
            "Install with: pip install sentry-sdk[fastapi]"
        )
        return False

    except Exception as e:
        logger.error(f"Failed to initialize Sentry: {e}")
        return False


def capture_exception(
    error: Exception,
    level: str = "error",
    extra: dict[str, Any] | None = None,
) -> str | None:
    """Manually capture an exception to Sentry.

    Args:
        error: Exception to capture
        level: Severity level (fatal, error, warning, info, debug)
        extra: Additional context to attach

    Returns:
        Event ID if successful, None otherwise

    Example:
        >>> try:
        ...     risky_operation()
        ... except Exception as e:
        ...     event_id = capture_exception(e, extra={"user_id": 123})
    """
    try:
        import sentry_sdk

        with sentry_sdk.push_scope() as scope:
            if extra:
                for key, value in extra.items():
                    scope.set_extra(key, value)
            scope.level = level

            event_id = sentry_sdk.capture_exception(error)
            return event_id

    except ImportError:
        logger.debug("sentry-sdk not available - exception not captured")
        return None


def capture_message(
    message: str,
    level: str = "info",
    extra: dict[str, Any] | None = None,
) -> str | None:
    """Manually capture a message to Sentry.

    Args:
        message: Message to capture
        level: Severity level (fatal, error, warning, info, debug)
        extra: Additional context to attach

    Returns:
        Event ID if successful, None otherwise

    Example:
        >>> capture_message(
        ...     "User uploaded large file",
        ...     level="warning",
        ...     extra={"file_size": 100_000_000}
        ... )
    """
    try:
        import sentry_sdk

        with sentry_sdk.push_scope() as scope:
            if extra:
                for key, value in extra.items():
                    scope.set_extra(key, value)
            scope.level = level

            event_id = sentry_sdk.capture_message(message)
            return event_id

    except ImportError:
        logger.debug("sentry-sdk not available - message not captured")
        return None


def set_context(name: str, context: dict[str, Any]) -> None:
    """Set custom context for all future events in this scope.

    Args:
        name: Context name (e.g., "graphql", "database", "user_action")
        context: Dictionary of context data

    Example:
        >>> set_context("graphql", {
        ...     "query": "{ users { id name } }",
        ...     "variables": {"limit": 10},
        ...     "complexity": 5
        ... })
    """
    try:
        import sentry_sdk

        sentry_sdk.set_context(name, context)

    except ImportError:
        pass


def set_user(
    user_id: str | int | None = None,
    email: str | None = None,
    username: str | None = None,
    **kwargs: Any,
) -> None:
    """Set user information for error reports.

    Args:
        user_id: User ID
        email: User email
        username: Username
        **kwargs: Additional user attributes

    Example:
        >>> set_user(
        ...     user_id=123,
        ...     email="user@example.com",
        ...     subscription_tier="premium"
        ... )
    """
    try:
        import sentry_sdk

        user_data = {"id": user_id, "email": email, "username": username, **kwargs}
        # Remove None values
        user_data = {k: v for k, v in user_data.items() if v is not None}

        sentry_sdk.set_user(user_data)

    except ImportError:
        pass
