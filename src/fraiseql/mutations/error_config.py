"""Configurable error detection for mutations."""

from dataclasses import dataclass, field
from typing import Pattern


@dataclass
class MutationErrorConfig:
    """Configurable error detection for mutations.

    v1.8.0 Breaking Change:
    ----------------------
    - Removed `error_as_data_prefixes` (all errors are now Error type)
    - `noop:*` statuses now return Error type with code 422
    - Success type ALWAYS has non-null entity
    - Error type includes REST-like `code` field (422, 404, 409, 500)

    Migration Guide:
    ---------------
    OLD (v1.7.x):
        noop:invalid_contract_id → CreateMachineSuccess with machine=null

    NEW (v1.8.0):
        noop:invalid_contract_id → CreateMachineError with code=422

    See docs/migrations/v1.8.0.md for details.
    """

    # Success keywords (unchanged)
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
            "created",  # Added for enterprise compatibility
            "cancelled",  # Added for enterprise compatibility
        },
    )

    # Error prefixes - NOW INCLUDES noop:, blocked:, etc.
    error_prefixes: set[str] = field(
        default_factory=lambda: {
            # v1.8.0: Moved from error_as_data_prefixes
            "noop:",  # Validation/business rule failures (422)
            "blocked:",  # Blocked operations (422)
            "skipped:",  # Skipped operations (422)
            "ignored:",  # Ignored operations (422)
            # Traditional errors
            "error:",
            "failed:",  # System failures (500)
            "validation_error:",
            "unauthorized:",  # Auth failures (401)
            "forbidden:",  # Permission failures (403)
            "not_found:",  # Missing resources (404)
            "timeout:",  # Timeouts (408)
            "conflict:",  # Conflicts (409)
        },
    )

    # REMOVED in v1.8.0: error_as_data_prefixes
    # All errors are now Error type

    # Error keywords (unchanged)
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

    # DEPRECATED in v1.8.0: always_return_as_data
    # Use success_keywords and error_prefixes instead
    always_return_as_data: bool = False

    def is_error_status(self, status: str) -> bool:
        """Check if a status should be treated as a GraphQL error.

        v1.8.0: This method now returns True for noop:* statuses.

        Args:
            status: The status string from the mutation result

        Returns:
            True if this should be Error type, False if Success type
        """
        if not status:
            return False

        if self.always_return_as_data:
            # DEPRECATED: For backward compatibility only
            import warnings

            warnings.warn(
                "always_return_as_data is deprecated in v1.8.0. "
                "Use success_keywords and error_prefixes instead.",
                DeprecationWarning,
                stacklevel=2,
            )
            return False

        status_lower = status.lower()

        # Check success keywords first
        if status_lower in self.success_keywords:
            return False

        # v1.8.0: REMOVED error_as_data_prefixes check
        # All non-success prefixes are errors

        # Check error prefixes (includes noop: now)
        for prefix in self.error_prefixes:
            if status_lower.startswith(prefix):
                return True

        # Check error keywords
        if any(keyword in status_lower for keyword in self.error_keywords):
            return True

        # Check custom pattern if provided
        if self.error_pattern and self.error_pattern.match(status):
            return True

        # Default: not an error (unknown statuses are success for backward compat)
        return False

    def get_error_code(self, status: str) -> int:
        """Map status string to REST-like error code.

        v1.8.0: New method for mapping statuses to application-level codes.

        Args:
            status: The status string from the mutation result

        Returns:
            Application-level error code (422, 404, 409, 500, etc.)
        """
        if not status:
            return 500

        status_lower = status.lower()

        # Validation/business rule failures
        if status_lower.startswith(("noop:", "blocked:", "skipped:", "ignored:")):
            return 422  # Unprocessable Entity

        # Resource not found
        if status_lower.startswith("not_found:"):
            return 404  # Not Found

        # Authentication failures
        if status_lower.startswith("unauthorized:"):
            return 401  # Unauthorized

        # Permission failures
        if status_lower.startswith("forbidden:"):
            return 403  # Forbidden

        # Resource conflicts
        if status_lower.startswith("conflict:"):
            return 409  # Conflict

        # Timeouts
        if status_lower.startswith("timeout:"):
            return 408  # Request Timeout

        # System failures
        if status_lower.startswith("failed:"):
            return 500  # Internal Server Error

        # Unknown errors
        return 500


# Updated default configuration
DEFAULT_ERROR_CONFIG = MutationErrorConfig(
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
        "created",  # Added for enterprise compatibility
        "cancelled",  # Added for enterprise compatibility
    },
    error_prefixes={
        # v1.8.0: Validation/business rule failures
        "noop:",
        "blocked:",
        "skipped:",
        "ignored:",
        # Traditional errors
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

# DEPRECATED in v1.8.0: STRICT_STATUS_CONFIG
# Use DEFAULT_ERROR_CONFIG instead
STRICT_STATUS_CONFIG = DEFAULT_ERROR_CONFIG


# DEPRECATED in v1.8.0: ALWAYS_DATA_CONFIG
# All errors are now Error type
def ALWAYS_DATA_CONFIG() -> MutationErrorConfig:  # noqa: D103
    import warnings

    warnings.warn(
        "ALWAYS_DATA_CONFIG is deprecated in v1.8.0. "
        "All errors now return Error type with appropriate codes.",
        DeprecationWarning,
        stacklevel=2,
    )
    return MutationErrorConfig(always_return_as_data=True)
