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

    post_id: UUID
    organization_id: UUID
    author_id: UUID
    title: str
    status: str


@dataclass
class PostPublishedEvent(DomainEvent):
    """Event emitted when a post is published."""

    post_id: UUID
    organization_id: UUID
    author_id: UUID
    title: str
    published_at: datetime


@dataclass
class PostUnpublishedEvent(DomainEvent):
    """Event emitted when a post is unpublished."""

    post_id: UUID
    organization_id: UUID
    reason: str | None = None


@dataclass
class PostDeletedEvent(DomainEvent):
    """Event emitted when a post is deleted."""

    post_id: UUID
    organization_id: UUID
    deleted_by: UUID
    reason: str | None = None


@dataclass
class CommentAddedEvent(DomainEvent):
    """Event emitted when a comment is added to a post."""

    comment_id: UUID
    post_id: UUID
    organization_id: UUID
    author_id: UUID
    content: str
    parent_id: UUID | None = None


@dataclass
class CommentApprovedEvent(DomainEvent):
    """Event emitted when a comment is approved."""

    comment_id: UUID
    post_id: UUID
    organization_id: UUID
    approved_by: UUID


@dataclass
class CommentRejectedEvent(DomainEvent):
    """Event emitted when a comment is rejected."""

    comment_id: UUID
    post_id: UUID
    organization_id: UUID
    rejected_by: UUID
    reason: str | None = None


# User Domain Events
@dataclass
class UserRegisteredEvent(DomainEvent):
    """Event emitted when a new user registers."""

    user_id: UUID
    organization_id: UUID
    username: str
    email: str
    role: str


@dataclass
class UserActivatedEvent(DomainEvent):
    """Event emitted when a user is activated."""

    user_id: UUID
    organization_id: UUID
    activated_by: UUID | None = None


@dataclass
class UserDeactivatedEvent(DomainEvent):
    """Event emitted when a user is deactivated."""

    user_id: UUID
    organization_id: UUID
    deactivated_by: UUID
    reason: str | None = None


@dataclass
class UserRoleChangedEvent(DomainEvent):
    """Event emitted when a user's role is changed."""

    user_id: UUID
    organization_id: UUID
    old_role: str
    new_role: str
    changed_by: UUID


# Management Domain Events
@dataclass
class OrganizationCreatedEvent(DomainEvent):
    """Event emitted when an organization is created."""

    organization_id: UUID
    name: str
    slug: str
    created_by: UUID


@dataclass
class OrganizationSubscriptionChangedEvent(DomainEvent):
    """Event emitted when organization subscription changes."""

    organization_id: UUID
    old_tier: str
    new_tier: str
    changed_by: UUID


# Taxonomy Domain Events
@dataclass
class TagCreatedEvent(DomainEvent):
    """Event emitted when a tag is created."""

    tag_id: UUID
    organization_id: UUID
    name: str
    created_by: UUID


@dataclass
class CategoryCreatedEvent(DomainEvent):
    """Event emitted when a category is created."""

    category_id: UUID
    organization_id: UUID
    name: str
    parent_id: UUID | None = None
    created_by: UUID | None = None


# Analytics Events
@dataclass
class PostViewedEvent(DomainEvent):
    """Event emitted when a post is viewed."""

    post_id: UUID
    organization_id: UUID
    viewer_id: UUID | None = None
    ip_address: str | None = None
    user_agent: str | None = None


@dataclass
class PostLikedEvent(DomainEvent):
    """Event emitted when a post is liked."""

    post_id: UUID
    organization_id: UUID
    user_id: UUID


@dataclass
class PostSharedEvent(DomainEvent):
    """Event emitted when a post is shared."""

    post_id: UUID
    organization_id: UUID
    platform: str  # social media platform or method
    user_id: UUID | None = None
    referrer: str | None = None
