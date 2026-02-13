"""Structured logging for Fraisier.

Provides JSON-formatted structured logging with context tracking,
sensitive data redaction, and operational event logging.
"""

import json
import logging
from datetime import datetime
from typing import Any


class JSONFormatter(logging.Formatter):
    """Format log records as JSON for structured logging.

    Example output:
        {
            "timestamp": "2026-01-22T15:30:45.123456",
            "level": "INFO",
            "logger": "fraisier.deployers",
            "message": "Deployment started",
            "context": {"deployment_id": "deploy-123", "fraise": "api"},
            "trace_id": "trace-abc123"
        }
    """

    def format(self, record: logging.LogRecord) -> str:
        """Format log record as JSON.

        Args:
            record: Log record to format

        Returns:
            JSON-formatted log string
        """
        log_obj = {
            "timestamp": datetime.utcnow().isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }

        # Add context if present
        if hasattr(record, "context") and record.context:
            log_obj["context"] = record.context

        # Add trace_id if present
        if hasattr(record, "trace_id") and record.trace_id:
            log_obj["trace_id"] = record.trace_id

        # Add exception info if present
        if record.exc_info:
            log_obj["exception"] = {
                "type": record.exc_info[0].__name__,
                "message": str(record.exc_info[1]),
                "traceback": self.formatException(record.exc_info),
            }

        # Add extra fields
        if hasattr(record, "extra_fields"):
            log_obj.update(record.extra_fields)

        return json.dumps(log_obj)


class ContextualLogger:
    """Logger with built-in context tracking.

    Accumulates context as operations proceed, making logs easier
    to correlate and debug.

    Usage:
        logger = ContextualLogger("fraisier.deployers")
        with logger.context(deployment_id="deploy-123", service="api"):
            logger.info("Starting deployment")
            logger.info("Checking health")  # Inherits context
    """

    def __init__(
        self,
        name: str,
        logger: logging.Logger | None = None,
        redact_keys: set[str] | None = None,
    ):
        """Initialize contextual logger.

        Args:
            name: Logger name
            logger: Existing logger instance (defaults to getLogger(name))
            redact_keys: Keys to redact from logs (for sensitive data)
        """
        self.name = name
        self.logger = logger or logging.getLogger(name)
        self._context_stack: list[dict[str, Any]] = []
        self.redact_keys = redact_keys or {
            "password",
            "api_key",
            "secret",
            "token",
            "auth",
        }

    def _get_context(self) -> dict[str, Any]:
        """Get merged context from all active scopes.

        Returns:
            Merged context dict
        """
        merged = {}
        for ctx in self._context_stack:
            merged.update(ctx)
        return merged

    def _redact_dict(self, data: dict[str, Any]) -> dict[str, Any]:
        """Redact sensitive keys from dict.

        Args:
            data: Dict to redact

        Returns:
            Dict with sensitive values replaced
        """
        redacted = {}
        for key, value in data.items():
            if any(sensitive in key.lower() for sensitive in self.redact_keys):
                redacted[key] = "***REDACTED***"
            elif isinstance(value, dict):
                redacted[key] = self._redact_dict(value)
            else:
                redacted[key] = value
        return redacted

    def context(self, **kwargs) -> "LogContext":
        """Enter context with additional logging context.

        Args:
            **kwargs: Context variables

        Returns:
            Context manager
        """
        return LogContext(self, kwargs)

    def debug(self, message: str, **kwargs):
        """Log debug message with context.

        Args:
            message: Log message
            **kwargs: Additional context
        """
        self._log("debug", message, kwargs)

    def info(self, message: str, **kwargs):
        """Log info message with context.

        Args:
            message: Log message
            **kwargs: Additional context
        """
        self._log("info", message, kwargs)

    def warning(self, message: str, **kwargs):
        """Log warning message with context.

        Args:
            message: Log message
            **kwargs: Additional context
        """
        self._log("warning", message, kwargs)

    def error(self, message: str, **kwargs):
        """Log error message with context.

        Args:
            message: Log message
            **kwargs: Additional context
        """
        self._log("error", message, kwargs)

    def critical(self, message: str, **kwargs):
        """Log critical message with context.

        Args:
            message: Log message
            **kwargs: Additional context
        """
        self._log("critical", message, kwargs)

    def _log(self, level: str, message: str, kwargs: dict[str, Any]):
        """Internal logging method.

        Args:
            level: Log level (debug, info, warning, error, critical)
            message: Log message
            kwargs: Additional context and extra fields
        """
        # Get accumulated context
        context = self._get_context()

        # Separate standard context from extra fields
        extra_fields = {}
        for key in list(kwargs.keys()):
            if not key.startswith("_"):
                context[key] = kwargs.pop(key)

        # Redact sensitive data
        safe_context = self._redact_dict(context)

        # Build extra dict for logging
        extra = {
            "context": safe_context,
            "extra_fields": extra_fields,
        }

        # Add trace_id if present
        if "_trace_id" in kwargs:
            extra["trace_id"] = kwargs["_trace_id"]

        # Call logger
        log_method = getattr(self.logger, level)

        # Check for exception
        exc_info = kwargs.get("_exc_info", False)
        log_method(message, extra=extra, exc_info=exc_info)


class LogContext:
    """Context manager for logger context.

    Usage:
        with logger.context(deployment_id="123", service="api"):
            logger.info("Deploying")  # Logs include context
    """

    def __init__(self, contextual_logger: ContextualLogger, context: dict[str, Any]):
        """Initialize log context.

        Args:
            contextual_logger: Parent ContextualLogger
            context: Context dict
        """
        self.logger = contextual_logger
        self.context = context

    def __enter__(self):
        """Enter context."""
        self.logger._context_stack.append(self.context)
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Exit context."""
        self.logger._context_stack.pop()
        return False


def setup_structured_logging(
    log_level: str = "INFO",
    json_output: bool = True,
    log_file: str | None = None,
) -> logging.Logger:
    """Setup structured logging for Fraisier.

    Args:
        log_level: Logging level (DEBUG, INFO, WARNING, ERROR, CRITICAL)
        json_output: Whether to use JSON formatting
        log_file: Optional log file path

    Returns:
        Configured root logger
    """
    root_logger = logging.getLogger("fraisier")

    # Set level
    root_logger.setLevel(getattr(logging, log_level.upper()))

    # Remove existing handlers
    root_logger.handlers.clear()

    # Console handler
    console_handler = logging.StreamHandler()
    console_handler.setLevel(getattr(logging, log_level.upper()))

    if json_output:
        console_handler.setFormatter(JSONFormatter())
    else:
        formatter = logging.Formatter(
            "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
        )
        console_handler.setFormatter(formatter)

    root_logger.addHandler(console_handler)

    # File handler if specified
    if log_file:
        file_handler = logging.FileHandler(log_file)
        file_handler.setLevel(getattr(logging, log_level.upper()))

        if json_output:
            file_handler.setFormatter(JSONFormatter())
        else:
            formatter = logging.Formatter(
                "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
            )
            file_handler.setFormatter(formatter)

        root_logger.addHandler(file_handler)

    return root_logger


def get_contextual_logger(name: str) -> ContextualLogger:
    """Get a contextual logger instance.

    Args:
        name: Logger name (e.g., "fraisier.deployers")

    Returns:
        ContextualLogger instance
    """
    logger = logging.getLogger(name)
    return ContextualLogger(name, logger)
