"""Domain exceptions for the enterprise blog system.

These exceptions represent business rule violations and domain-specific errors.
"""

from typing import Any
from uuid import UUID

from .base_classes import DomainException


# Generic Domain Exceptions
class EntityNotFoundError(DomainException):
    """Exception raised when an entity is not found."""

    def __init__(self, entity_type: str, entity_id: UUID):
        super().__init__(
            f"{entity_type} with id {entity_id} was not found",
            {"entity_type": entity_type, "entity_id": str(entity_id)},
        )
        self.entity_type = entity_type
        self.entity_id = entity_id


class UnauthorizedAccessError(DomainException):
    """Exception raised when unauthorized access is attempted."""

    def __init__(self, user_id: UUID, resource: str, action: str):
        super().__init__(
            f"User {user_id} is not authorized to {action} {resource}",
            {"user_id": str(user_id), "resource": resource, "action": action},
        )


class DuplicateEntityError(DomainException):
    """Exception raised when attempting to create a duplicate entity."""

    def __init__(self, entity_type: str, field: str, value: Any):
        super().__init__(
            f"{entity_type} with {field}='{value}' already exists",
            {"entity_type": entity_type, "field": field, "value": str(value)},
        )


# Content Domain Exceptions
class PostPublishingError(DomainException):
    """Exception raised when a post cannot be published."""

    def __init__(self, post_id: UUID, reason: str):
        super().__init__(
            f"Post {post_id} cannot be published: {reason}",
            {"post_id": str(post_id), "reason": reason},
        )


class InvalidPostStatusTransitionError(DomainException):
    """Exception raised for invalid post status transitions."""

    def __init__(self, post_id: UUID, current_status: str, target_status: str):
        super().__init__(
            f"Cannot transition post {post_id} from {current_status} to {target_status}",
            {
                "post_id": str(post_id),
                "current_status": current_status,
                "target_status": target_status,
            },
        )


class CommentModerationError(DomainException):
    """Exception raised when comment moderation fails."""

    def __init__(self, comment_id: UUID, reason: str):
        super().__init__(
            f"Comment {comment_id} moderation failed: {reason}",
            {"comment_id": str(comment_id), "reason": reason},
        )


# User Domain Exceptions
class UserRegistrationError(DomainException):
    """Exception raised when user registration fails."""

    def __init__(self, reason: str, details: dict[str, Any] | None = None):
        super().__init__(f"User registration failed: {reason}", details)


class InvalidCredentialsError(DomainException):
    """Exception raised when authentication fails."""

    def __init__(self, username: str):
        super().__init__(f"Invalid credentials for user {username}", {"username": username})


class UserDeactivationError(DomainException):
    """Exception raised when user cannot be deactivated."""

    def __init__(self, user_id: UUID, reason: str):
        super().__init__(
            f"User {user_id} cannot be deactivated: {reason}",
            {"user_id": str(user_id), "reason": reason},
        )


class InsufficientPermissionsError(DomainException):
    """Exception raised when user lacks required permissions."""

    def __init__(self, user_id: UUID, required_permission: str):
        super().__init__(
            f"User {user_id} lacks required permission: {required_permission}",
            {"user_id": str(user_id), "permission": required_permission},
        )


# Organization Domain Exceptions
class OrganizationLimitExceededError(DomainException):
    """Exception raised when organization limits are exceeded."""

    def __init__(self, organization_id: UUID, limit_type: str, current: int, maximum: int):
        super().__init__(
            f"Organization {organization_id} has exceeded {limit_type} limit: {current}/{maximum}",
            {
                "organization_id": str(organization_id),
                "limit_type": limit_type,
                "current": current,
                "maximum": maximum,
            },
        )


class SubscriptionTierError(DomainException):
    """Exception raised for subscription tier related errors."""

    def __init__(self, organization_id: UUID, current_tier: str, required_tier: str):
        super().__init__(
            f"Organization {organization_id} tier '{current_tier}' insufficient, requires '{required_tier}'",
            {
                "organization_id": str(organization_id),
                "current_tier": current_tier,
                "required_tier": required_tier,
            },
        )


# Validation Exceptions
class ValidationError(DomainException):
    """Exception raised for validation errors."""

    def __init__(self, field: str, value: Any, rule: str):
        super().__init__(
            f"Validation failed for field '{field}': {rule}",
            {"field": field, "value": str(value), "rule": rule},
        )


class BusinessRuleViolationError(DomainException):
    """Exception raised when business rules are violated."""

    def __init__(self, rule_name: str, details: dict[str, Any]):
        super().__init__(f"Business rule violation: {rule_name}", {"rule": rule_name, **details})
