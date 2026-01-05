"""Domain events for the enterprise blog system.

Events represent things that have happened in the domain and are used
for decoupling between bounded contexts.
"""

from dataclasses import dataclass
from datetime import datetime
from uuid import UUID

from .base_classes import DomainEvent


# Content Domain Events
@dataclass
class PostCreatedEvent(DomainEvent):
    """Event emitted when a post is created."""

    post_id: ID
    organization_id: ID
    author_id: ID
    title: str
    status: str


@dataclass
class PostPublishedEvent(DomainEvent):
    """Event emitted when a post is published."""

    post_id: ID
    organization_id: ID
    author_id: ID
    title: str
    published_at: datetime


@dataclass
class PostUnpublishedEvent(DomainEvent):
    """Event emitted when a post is unpublished."""

    post_id: ID
    organization_id: ID
    reason: str | None = None


@dataclass
class PostDeletedEvent(DomainEvent):
    """Event emitted when a post is deleted."""

    post_id: ID
    organization_id: ID
    deleted_by: UUID
    reason: str | None = None


@dataclass
class CommentAddedEvent(DomainEvent):
    """Event emitted when a comment is added to a post."""

    comment_id: ID
    post_id: ID
    organization_id: ID
    author_id: ID
    content: str
    parent_id: ID | None = None


@dataclass
class CommentApprovedEvent(DomainEvent):
    """Event emitted when a comment is approved."""

    comment_id: ID
    post_id: ID
    organization_id: ID
    approved_by: UUID


@dataclass
class CommentRejectedEvent(DomainEvent):
    """Event emitted when a comment is rejected."""

    comment_id: ID
    post_id: ID
    organization_id: ID
    rejected_by: UUID
    reason: str | None = None


# User Domain Events
@dataclass
class UserRegisteredEvent(DomainEvent):
    """Event emitted when a new user registers."""

    user_id: ID
    organization_id: ID
    username: str
    email: str
    role: str


@dataclass
class UserActivatedEvent(DomainEvent):
    """Event emitted when a user is activated."""

    user_id: ID
    organization_id: ID
    activated_by: UUID | None = None


@dataclass
class UserDeactivatedEvent(DomainEvent):
    """Event emitted when a user is deactivated."""

    user_id: ID
    organization_id: ID
    deactivated_by: UUID
    reason: str | None = None


@dataclass
class UserRoleChangedEvent(DomainEvent):
    """Event emitted when a user's role is changed."""

    user_id: ID
    organization_id: ID
    old_role: str
    new_role: str
    changed_by: UUID


# Management Domain Events
@dataclass
class OrganizationCreatedEvent(DomainEvent):
    """Event emitted when an organization is created."""

    organization_id: ID
    name: str
    slug: str
    created_by: UUID


@dataclass
class OrganizationSubscriptionChangedEvent(DomainEvent):
    """Event emitted when organization subscription changes."""

    organization_id: ID
    old_tier: str
    new_tier: str
    changed_by: UUID


# Taxonomy Domain Events
@dataclass
class TagCreatedEvent(DomainEvent):
    """Event emitted when a tag is created."""

    tag_id: ID
    organization_id: ID
    name: str
    created_by: UUID


@dataclass
class CategoryCreatedEvent(DomainEvent):
    """Event emitted when a category is created."""

    category_id: ID
    organization_id: ID
    name: str
    parent_id: ID | None = None
    created_by: UUID | None = None


# Analytics Events
@dataclass
class PostViewedEvent(DomainEvent):
    """Event emitted when a post is viewed."""

    post_id: ID
    organization_id: ID
    viewer_id: ID | None = None
    ip_address: str | None = None
    user_agent: str | None = None


@dataclass
class PostLikedEvent(DomainEvent):
    """Event emitted when a post is liked."""

    post_id: ID
    organization_id: ID
    user_id: ID


@dataclass
class PostSharedEvent(DomainEvent):
    """Event emitted when a post is shared."""

    post_id: ID
    organization_id: ID
    platform: str  # social media platform or method
    user_id: ID | None = None
    referrer: str | None = None
