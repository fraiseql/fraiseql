"""Configurable error detection for mutations."""

import re
from dataclasses import dataclass, field
from typing import Pattern


@dataclass
class MutationErrorConfig:
    """Configuration for how mutation statuses are interpreted.

    This allows projects to define their own patterns for what constitutes
    an error vs successful data return.
    """

    # Statuses that are always treated as success (returned in data field)
    success_keywords: set[str] = field(
        default_factory=lambda: {
            "success",
            "completed",
            "ok",
            "done",
            "new",
            "existing",
            "updated",
            "deleted",
            "synced",
        },
    )

    # Status prefixes that indicate errors (returned in errors field)
    error_prefixes: set[str] = field(
        default_factory=lambda: {
            "error:",
            "failed:",
            "validation_error:",
            "unauthorized:",
            "forbidden:",
            "not_found:",
            "timeout:",
            "conflict:",
        },
    )

    # Status prefixes that are treated as success despite looking like errors
    # (returned in data field with error-like information)
    error_as_data_prefixes: set[str] = field(
        default_factory=lambda: {
            "noop:",
            "blocked:",
            "skipped:",
            "ignored:",
        },
    )

    # Additional keywords that indicate errors
    error_keywords: set[str] = field(
        default_factory=lambda: {
            "error",
            "failed",
            "fail",
            "invalid",
            "timeout",
        },
    )

    # Custom regex pattern for error detection (optional)
    error_pattern: Pattern[str] | None = None

    # Whether to treat all non-success statuses as data (never raise GraphQL errors)
    always_return_as_data: bool = False

    def is_error_status(self, status: str) -> bool:
        """Check if a status should be treated as a GraphQL error.

        Args:
            status: The status string from the mutation result

        Returns:
            True if this should be a GraphQL error, False if it should be in data
        """
        if not status:
            return False

        if self.always_return_as_data:
            return False

        status_lower = status.lower()

        # Check success keywords first
        if status_lower in self.success_keywords:
            return False

        # Check error-as-data prefixes (these are NOT GraphQL errors)
        for prefix in self.error_as_data_prefixes:
            if status_lower.startswith(prefix):
                return False

        # Check error prefixes
        for prefix in self.error_prefixes:
            if status_lower.startswith(prefix):
                return True

        # Check error keywords
        if any(keyword in status_lower for keyword in self.error_keywords):
            return True

        # Check custom pattern if provided
        if self.error_pattern and self.error_pattern.match(status):
            return True

        # Default: not an error
        return False


# Default configuration
DEFAULT_ERROR_CONFIG = MutationErrorConfig()

# PrintOptim-compatible configuration
PRINTOPTIM_ERROR_CONFIG = MutationErrorConfig(
    success_keywords={
        "success",
        "completed",
        "ok",
        "done",
        "new",
        "existing",
        "updated",
        "deleted",
        "synced",
    },
    error_prefixes={
        "failed:",  # Only failed: prefix triggers GraphQL errors
    },
    error_as_data_prefixes={
        "noop:",  # noop: statuses are returned as data
        "blocked:",  # blocked: statuses are returned as data
    },
    error_keywords=set(),  # Don't use generic keywords
    # Match the exact constraint from tb_entity_change_log
    error_pattern=re.compile(r"^failed:[a-z_]+$"),
)

# Configuration for always returning errors as data
ALWAYS_DATA_CONFIG = MutationErrorConfig(
    always_return_as_data=True,
)
