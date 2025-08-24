"""Enhanced GraphQL types with Error Arrays for Blog E2E Test Suite.

This module demonstrates the PrintOptim Backend pattern for handling multiple
validation errors as structured arrays, following the architecture found in
the actual PrintOptim Backend codebase.

Key Features:
- Multiple validation errors returned as structured arrays
- PrintOptim-style error objects with code, identifier, message, details
- Enhanced error handling with field-level validation details
- Comprehensive error metadata for debugging
"""

import uuid
from typing import Any

import fraiseql
from fraiseql import UNSET


# ============================================================================
# ERROR TYPES - Following PrintOptim Backend patterns
# ============================================================================

@fraiseql.type
class Error:
    """Structured error object following PrintOptim Backend patterns.

    This matches the error structure expected by frontend applications:
    - code: HTTP-style status code (422 for validation, 409 for conflicts)
    - identifier: Machine-readable error identifier for programmatic handling
    - message: Human-readable error message for display
    - details: Additional structured context for debugging/client handling
    """

    code: int
    identifier: str
    message: str
    details: dict[str, Any] | None = None


@fraiseql.type
class MutationResultBase:
    """Enhanced base class for mutation results with error arrays.

    Following PrintOptim Backend's MutationResultBase pattern with support
    for structured error arrays that provide comprehensive validation feedback.
    """

    status: str = "success"
    message: str | None = None
    errors: list[Error] | None = None  # Array of structured error objects


# ============================================================================
# AUTHOR TYPES - Enhanced with comprehensive error handling
# ============================================================================

@fraiseql.input
class CreateAuthorEnhancedInput:
    """Enhanced input for creating authors with comprehensive validation."""

    identifier: str
    name: str
    email: str
    bio: str | None = UNSET
    avatar_url: str | None = UNSET
    social_links: dict[str, Any] | None = UNSET


@fraiseql.type
class Author:
    """Author entity for GraphQL responses."""

    id: uuid.UUID
    identifier: str
    name: str
    email: str
    bio: str | None = None
    avatar_url: str | None = None
    post_count: int = 0
    last_post_at: str | None = None
    created_at: str
    updated_at: str


@fraiseql.success
class CreateAuthorEnhancedSuccess(MutationResultBase):
    """Enhanced success response with empty errors array."""

    author: Author | None = None
    message: str = "Author created successfully"
    errors: list[Error] = []  # Always empty array for success


@fraiseql.failure
class CreateAuthorEnhancedError(MutationResultBase):
    """Enhanced error response with structured error arrays.

    This demonstrates multiple validation errors being returned as an array,
    following PrintOptim Backend patterns for comprehensive error feedback.
    """

    message: str
    errors: list[Error]  # Array of structured error objects

    # Additional context fields for specific error types
    conflict_author: Author | None = None  # For duplicate conflicts
    validation_summary: dict[str, Any] | None = None  # Summary of validation issues


# ============================================================================
# POST TYPES - Enhanced with comprehensive error handling
# ============================================================================

@fraiseql.input
class CreatePostEnhancedInput:
    """Enhanced input for creating posts with comprehensive validation."""

    identifier: str
    title: str
    content: str
    excerpt: str | None = UNSET
    featured_image_url: str | None = UNSET
    author_identifier: str
    tag_identifiers: list[str] | None = UNSET
    status: str = "draft"
    publish_at: str | None = UNSET


@fraiseql.type
class Post:
    """Post entity for GraphQL responses."""

    id: uuid.UUID
    identifier: str
    title: str
    content: str
    excerpt: str | None = None
    featured_image_url: str | None = None
    author_id: uuid.UUID
    author_name: str | None = None
    status: str
    published_at: str | None = None
    tags: list[dict[str, Any]] | None = None
    comment_count: int = 0
    tag_count: int = 0
    created_at: str
    updated_at: str


@fraiseql.success
class CreatePostEnhancedSuccess(MutationResultBase):
    """Enhanced success response with empty errors array."""

    post: Post | None = None
    message: str = "Post created successfully"
    errors: list[Error] = []  # Always empty array for success


@fraiseql.failure
class CreatePostEnhancedError(MutationResultBase):
    """Enhanced error response with structured error arrays.

    Demonstrates comprehensive validation error handling including:
    - Multiple field validation errors
    - Security validation errors
    - Reference validation errors
    - Business rule violation errors
    """

    message: str
    errors: list[Error]  # Array of structured error objects

    # Additional context fields
    conflict_post: Post | None = None
    missing_author: dict[str, str] | None = None
    invalid_tags: list[str] | None = None
    security_violations: list[str] | None = None
    validation_summary: dict[str, Any] | None = None


# ============================================================================
# ENHANCED MUTATION BASE CLASS - Following PrintOptim patterns
# ============================================================================

class BlogEnhancedMutationBase:
    """Enhanced base class for blog mutations with comprehensive error handling.

    This follows PrintOptim Backend's PrintOptimMutation pattern but enhanced
    to demonstrate proper error array handling and structured error responses.
    """

    def __init_subclass__(
        cls,
        function: str,
        schema: str = "app",
        context_params: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> None:
        """Initialize subclass with automatic mutation decorator application."""
        super().__init_subclass__(**kwargs)

        # Validate required type annotations
        if not hasattr(cls, "__annotations__"):
            raise TypeError(
                f"{cls.__name__} must define input, success, and failure type annotations"
            )

        annotations = cls.__annotations__
        required = {"input", "success", "failure"}
        missing = required - set(annotations.keys())

        if missing:
            raise TypeError(
                f"{cls.__name__} missing required type annotations: {', '.join(sorted(missing))}"
            )

        # Apply FraiseQL mutation decorator with enhanced error configuration
        fraiseql.mutation(
            function=function,
            schema=schema,
            context_params=context_params or {},
            error_config=fraiseql.DEFAULT_ERROR_CONFIG  # Enhanced error handling
        )(cls)


# ============================================================================
# ENHANCED MUTATION IMPLEMENTATIONS
# ============================================================================

class CreateAuthorEnhanced(
    BlogEnhancedMutationBase,
    function="create_author_enhanced",  # Points to enhanced SQL function
    context_params={"user_id": "input_created_by"}
):
    """Enhanced author creation with comprehensive error validation."""

    input: CreateAuthorEnhancedInput
    success: CreateAuthorEnhancedSuccess
    failure: CreateAuthorEnhancedError


class CreatePostEnhanced(
    BlogEnhancedMutationBase,
    function="create_post_enhanced",  # Points to enhanced SQL function
    context_params={"user_id": "input_created_by"}
):
    """Enhanced post creation with comprehensive error validation."""

    input: CreatePostEnhancedInput
    success: CreatePostEnhancedSuccess
    failure: CreatePostEnhancedError


# ============================================================================
# RESPONSE MAPPING FUNCTIONS - Enhanced for error arrays
# ============================================================================

def map_author_from_enhanced_result(result: dict) -> Author | None:
    """Map enhanced database result to Author GraphQL type."""
    if not result or not result.get("object_data"):
        return None

    data = result["object_data"]
    return Author(
        id=result["id"],
        identifier=data.get("identifier", ""),
        name=data.get("name", ""),
        email=data.get("email", ""),
        bio=data.get("bio"),
        avatar_url=data.get("avatar_url"),
        post_count=data.get("post_count", 0),
        last_post_at=data.get("last_post_at"),
        created_at=data.get("created_at", ""),
        updated_at=data.get("updated_at", "")
    )


def map_post_from_enhanced_result(result: dict) -> Post | None:
    """Map enhanced database result to Post GraphQL type."""
    if not result or not result.get("object_data"):
        return None

    data = result["object_data"]
    return Post(
        id=result["id"],
        identifier=data.get("identifier", ""),
        title=data.get("title", ""),
        content=data.get("content", ""),
        excerpt=data.get("excerpt"),
        featured_image_url=data.get("featured_image_url"),
        author_id=data.get("author_id", result["id"]),
        author_name=data.get("author_name"),
        status=data.get("status", "draft"),
        published_at=data.get("published_at"),
        tags=data.get("tags", []),
        comment_count=data.get("comment_count", 0),
        tag_count=data.get("tag_count", 0),
        created_at=data.get("created_at", ""),
        updated_at=data.get("updated_at", "")
    )


def map_errors_from_result(result: dict) -> list[Error]:
    """Map database errors JSONB array to GraphQL Error objects.

    This function handles the conversion from PostgreSQL JSONB error arrays
    to structured GraphQL Error objects, following PrintOptim Backend patterns.

    Expected database format:
    {
        "errors": [
            {
                "code": 422,
                "identifier": "missing_required_field",
                "message": "Missing required field: name",
                "details": {"field": "name", "constraint": "required"}
            },
            ...
        ]
    }
    """
    if not result or not result.get("errors"):
        return []

    errors_data = result["errors"]
    if isinstance(errors_data, str):
        # Handle case where errors might be JSON string
        import json
        try:
            errors_data = json.loads(errors_data)
        except (json.JSONDecodeError, TypeError):
            return []

    if not isinstance(errors_data, list):
        return []

    mapped_errors = []
    for error_data in errors_data:
        if isinstance(error_data, dict):
            mapped_errors.append(Error(
                code=error_data.get("code", 500),
                identifier=error_data.get("identifier", "unknown_error"),
                message=error_data.get("message", "An error occurred"),
                details=error_data.get("details")
            ))

    return mapped_errors


def create_validation_summary(errors: list[Error]) -> dict[str, Any]:
    """Create validation summary from error array for enhanced error responses."""
    if not errors:
        return {}

    field_errors = {}
    constraint_violations = {}
    security_issues = []

    for error in errors:
        if error.details and isinstance(error.details, dict):
            field = error.details.get("field")
            constraint = error.details.get("constraint")

            if field:
                if field not in field_errors:
                    field_errors[field] = []
                field_errors[field].append(error.message)

            if constraint:
                if constraint not in constraint_violations:
                    constraint_violations[constraint] = 0
                constraint_violations[constraint] += 1

                if constraint == "security":
                    security_issues.append(error.details.get("violation", "unknown"))

    return {
        "total_errors": len(errors),
        "field_errors": field_errors,
        "constraint_violations": constraint_violations,
        "security_issues": security_issues if security_issues else None,
        "has_conflicts": any(e.code == 409 for e in errors),
        "has_validation_errors": any(e.code == 422 for e in errors)
    }


# ============================================================================
# DEMONSTRATION FUNCTIONS - For testing error array patterns
# ============================================================================

def create_sample_validation_errors() -> list[Error]:
    """Create sample validation errors to demonstrate array structure."""
    return [
        Error(
            code=422,
            identifier="missing_required_field",
            message="Missing required field: identifier",
            details={"field": "identifier", "constraint": "required"}
        ),
        Error(
            code=422,
            identifier="missing_required_field",
            message="Missing required field: name",
            details={"field": "name", "constraint": "required"}
        ),
        Error(
            code=422,
            identifier="invalid_email_format",
            message="Invalid email format: not-an-email",
            details={
                "field": "email",
                "constraint": "format",
                "value": "not-an-email"
            }
        ),
        Error(
            code=422,
            identifier="identifier_too_long",
            message="Identifier too long: 75 characters (maximum 50)",
            details={
                "field": "identifier",
                "constraint": "max_length",
                "max_length": 50,
                "current_length": 75
            }
        )
    ]


def create_sample_security_errors() -> list[Error]:
    """Create sample security validation errors."""
    return [
        Error(
            code=422,
            identifier="unsafe_html",
            message="Content contains potentially unsafe HTML: script tags not allowed",
            details={
                "field": "content",
                "constraint": "security",
                "violation": "script_tag"
            }
        ),
        Error(
            code=422,
            identifier="path_traversal",
            message="Content contains potential path traversal attack",
            details={
                "field": "content",
                "constraint": "security",
                "violation": "path_traversal"
            }
        )
    ]


def create_sample_conflict_errors() -> list[Error]:
    """Create sample conflict errors."""
    return [
        Error(
            code=409,
            identifier="duplicate_identifier",
            message='Author with identifier "existing-author" already exists',
            details={
                "field": "identifier",
                "constraint": "unique",
                "conflict_id": "12345678-1234-1234-1234-123456789012",
                "conflict_identifier": "existing-author"
            }
        ),
        Error(
            code=409,
            identifier="duplicate_email",
            message='Author with email "existing@example.com" already exists',
            details={
                "field": "email",
                "constraint": "unique",
                "conflict_id": "12345678-1234-1234-1234-123456789012",
                "conflict_email": "existing@example.com"
            }
        )
    ]
