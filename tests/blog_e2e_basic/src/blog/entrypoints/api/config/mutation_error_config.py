"""Mutation error configuration for Blog Demo Application.

Following PrintOptim Backend error configuration patterns for consistent,
comprehensive error handling across all mutations.
"""

from typing import Dict, List, Any
from dataclasses import dataclass


@dataclass
class ErrorConfig:
    """Configuration for mutation error responses."""

    message_template: str
    error_code: str
    include_details: bool = True
    include_suggestions: bool = False
    include_related_entities: bool = False


# Default error configuration following PrintOptim Backend patterns
DEFAULT_ERROR_CONFIG: Dict[str, ErrorConfig] = {
    # Validation Errors
    "required_field": ErrorConfig(
        message_template="Required field '{field}' is missing",
        error_code="REQUIRED_FIELD_MISSING",
        include_details=True
    ),

    "invalid_format": ErrorConfig(
        message_template="Invalid format for field '{field}': {details}",
        error_code="INVALID_FORMAT",
        include_details=True
    ),

    "value_too_long": ErrorConfig(
        message_template="Value for field '{field}' exceeds maximum length of {max_length}",
        error_code="VALUE_TOO_LONG",
        include_details=True
    ),

    "value_too_short": ErrorConfig(
        message_template="Value for field '{field}' is below minimum length of {min_length}",
        error_code="VALUE_TOO_SHORT",
        include_details=True
    ),

    "invalid_email": ErrorConfig(
        message_template="Invalid email address format: {email}",
        error_code="INVALID_EMAIL_FORMAT",
        include_details=True
    ),

    # Duplicate Errors
    "duplicate_identifier": ErrorConfig(
        message_template="Identifier '{identifier}' already exists",
        error_code="DUPLICATE_IDENTIFIER",
        include_details=True,
        include_related_entities=True
    ),

    "duplicate_email": ErrorConfig(
        message_template="Email address '{email}' is already registered",
        error_code="DUPLICATE_EMAIL",
        include_details=True,
        include_related_entities=True,
        include_suggestions=True
    ),

    "duplicate_title": ErrorConfig(
        message_template="Post title '{title}' already exists",
        error_code="DUPLICATE_TITLE",
        include_details=True,
        include_suggestions=True
    ),

    # Not Found Errors
    "author_not_found": ErrorConfig(
        message_template="Author with identifier '{identifier}' not found",
        error_code="AUTHOR_NOT_FOUND",
        include_details=True,
        include_suggestions=True
    ),

    "post_not_found": ErrorConfig(
        message_template="Post with identifier '{identifier}' not found",
        error_code="POST_NOT_FOUND",
        include_details=True
    ),

    "tag_not_found": ErrorConfig(
        message_template="Tag with identifier '{identifier}' not found",
        error_code="TAG_NOT_FOUND",
        include_details=True
    ),

    "comment_not_found": ErrorConfig(
        message_template="Comment with ID '{id}' not found",
        error_code="COMMENT_NOT_FOUND",
        include_details=True
    ),

    # Business Logic Errors
    "post_not_publishable": ErrorConfig(
        message_template="Post does not meet publication requirements",
        error_code="POST_NOT_PUBLISHABLE",
        include_details=True,
        include_suggestions=True
    ),

    "post_already_published": ErrorConfig(
        message_template="Post is already published",
        error_code="POST_ALREADY_PUBLISHED",
        include_details=True
    ),

    "comment_not_approvable": ErrorConfig(
        message_template="Comment cannot be approved in current state",
        error_code="COMMENT_NOT_APPROVABLE",
        include_details=True
    ),

    "circular_tag_hierarchy": ErrorConfig(
        message_template="Creating tag hierarchy would create a circular reference",
        error_code="CIRCULAR_TAG_HIERARCHY",
        include_details=True
    ),

    # Authorization Errors
    "insufficient_permissions": ErrorConfig(
        message_template="Insufficient permissions to {action} {resource}",
        error_code="INSUFFICIENT_PERMISSIONS",
        include_details=True
    ),

    "not_author": ErrorConfig(
        message_template="Only the author can perform this action on the post",
        error_code="NOT_AUTHOR",
        include_details=True
    ),

    "not_authenticated": ErrorConfig(
        message_template="Authentication required for this action",
        error_code="NOT_AUTHENTICATED",
        include_details=False
    ),

    # System Errors
    "database_error": ErrorConfig(
        message_template="Database operation failed: {details}",
        error_code="DATABASE_ERROR",
        include_details=True
    ),

    "transaction_failed": ErrorConfig(
        message_template="Transaction failed and was rolled back",
        error_code="TRANSACTION_FAILED",
        include_details=True
    ),

    "configuration_error": ErrorConfig(
        message_template="System configuration error: {details}",
        error_code="CONFIGURATION_ERROR",
        include_details=True
    ),

    "internal_error": ErrorConfig(
        message_template="An internal error occurred. Please try again later.",
        error_code="INTERNAL_ERROR",
        include_details=False
    ),

    # NOOP Status Mappings
    "noop_no_changes": ErrorConfig(
        message_template="No changes detected - entity already in desired state",
        error_code="NOOP_NO_CHANGES",
        include_details=True
    ),

    "noop_duplicate_ignored": ErrorConfig(
        message_template="Duplicate creation ignored - entity already exists",
        error_code="NOOP_DUPLICATE_IGNORED",
        include_details=True,
        include_related_entities=True
    ),

    "noop_invalid_transition": ErrorConfig(
        message_template="Invalid state transition - operation ignored",
        error_code="NOOP_INVALID_TRANSITION",
        include_details=True
    ),
}


def get_error_config(error_type: str) -> ErrorConfig:
    """Get error configuration for a specific error type."""
    return DEFAULT_ERROR_CONFIG.get(
        error_type,
        ErrorConfig(
            message_template="Unknown error occurred",
            error_code="UNKNOWN_ERROR",
            include_details=False
        )
    )


def format_error_message(error_type: str, **kwargs) -> str:
    """Format error message using template and provided parameters."""
    config = get_error_config(error_type)
    try:
        return config.message_template.format(**kwargs)
    except KeyError:
        # Fall back to original template if parameters don't match
        return config.message_template


def get_suggested_actions(error_type: str, **context) -> List[str]:
    """Get suggested actions for error resolution."""
    suggestions = []

    if error_type == "duplicate_email":
        suggestions = [
            "Use a different email address",
            "Check if you already have an account",
            "Try logging in instead of creating a new account"
        ]
    elif error_type == "author_not_found":
        suggestions = [
            "Check the author identifier for typos",
            "Ensure the author exists in the system",
            "Create the author first before creating posts"
        ]
    elif error_type == "post_not_publishable":
        suggestions = [
            "Ensure the post has a title and content",
            "Check that the content meets minimum length requirements",
            "Verify the post has an assigned author"
        ]
    elif error_type == "duplicate_title":
        suggestions = [
            "Choose a different title for your post",
            "Add a subtitle or modifier to make it unique",
            "Check if you're trying to create a duplicate post"
        ]

    return suggestions
