"""Blog Post domain model.

Following Domain-Driven Design principles with rich domain models
and business logic encapsulation.
"""

import uuid
from datetime import datetime
from typing import Optional, List
from dataclasses import dataclass, field
from enum import Enum

from ...core.exceptions import BlogValidationError, BlogBusinessLogicError


class PostStatus(Enum):
    """Post publication status."""
    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"


@dataclass
class PostMetadata:
    """Post metadata for SEO and analytics."""
    meta_title: Optional[str] = None
    meta_description: Optional[str] = None
    featured_image_url: Optional[str] = None
    reading_time_minutes: Optional[int] = None
    view_count: int = 0
    like_count: int = 0
    share_count: int = 0


@dataclass
class Post:
    """Blog Post domain model with rich business logic."""

    # Identity
    id: uuid.UUID
    identifier: str  # URL slug

    # Content
    title: str
    content: str
    excerpt: Optional[str] = None

    # Relationships
    author_id: uuid.UUID
    tag_ids: List[uuid.UUID] = field(default_factory=list)

    # Publication
    status: PostStatus = PostStatus.DRAFT
    published_at: Optional[datetime] = None

    # Metadata
    metadata: PostMetadata = field(default_factory=PostMetadata)

    # Audit fields
    created_at: datetime = field(default_factory=datetime.now)
    created_by: uuid.UUID = None
    updated_at: datetime = field(default_factory=datetime.now)
    updated_by: uuid.UUID = None
    version: int = 1

    def __post_init__(self):
        """Validate post after initialization."""
        self.validate()

    def validate(self) -> None:
        """Validate post data according to business rules."""
        if not self.title or len(self.title.strip()) == 0:
            raise BlogValidationError("Post title is required", field="title")

        if len(self.title) > 200:
            raise BlogValidationError(
                "Post title must be 200 characters or less",
                field="title",
                value=len(self.title)
            )

        if not self.content or len(self.content.strip()) == 0:
            raise BlogValidationError("Post content is required", field="content")

        if len(self.content) > 50000:  # Configurable limit
            raise BlogValidationError(
                "Post content exceeds maximum length",
                field="content",
                value=len(self.content)
            )

        if not self.identifier or len(self.identifier.strip()) == 0:
            raise BlogValidationError("Post identifier (slug) is required", field="identifier")

        # Validate identifier format (URL-safe)
        import re
        if not re.match(r'^[a-z0-9-]+$', self.identifier):
            raise BlogValidationError(
                "Post identifier must contain only lowercase letters, numbers, and hyphens",
                field="identifier",
                value=self.identifier
            )

    def can_publish(self) -> bool:
        """Check if post meets publication requirements."""
        try:
            self.validate()
            return (
                self.status == PostStatus.DRAFT and
                len(self.title.strip()) >= 5 and
                len(self.content.strip()) >= 100 and
                self.author_id is not None
            )
        except BlogValidationError:
            return False

    def publish(self, published_by: uuid.UUID) -> None:
        """Publish the post with business logic validation."""
        if not self.can_publish():
            raise BlogBusinessLogicError(
                "Post does not meet publication requirements",
                constraint="publication_requirements"
            )

        if self.status == PostStatus.PUBLISHED:
            raise BlogBusinessLogicError(
                "Post is already published",
                constraint="already_published"
            )

        self.status = PostStatus.PUBLISHED
        self.published_at = datetime.now()
        self.updated_at = datetime.now()
        self.updated_by = published_by
        self.version += 1

    def unpublish(self, unpublished_by: uuid.UUID) -> None:
        """Unpublish (archive) the post."""
        if self.status != PostStatus.PUBLISHED:
            raise BlogBusinessLogicError(
                "Only published posts can be unpublished",
                constraint="unpublish_published_only"
            )

        self.status = PostStatus.ARCHIVED
        self.updated_at = datetime.now()
        self.updated_by = unpublished_by
        self.version += 1

    def update_content(
        self,
        title: Optional[str] = None,
        content: Optional[str] = None,
        excerpt: Optional[str] = None,
        updated_by: uuid.UUID = None
    ) -> None:
        """Update post content with validation."""
        if title is not None:
            self.title = title
        if content is not None:
            self.content = content
        if excerpt is not None:
            self.excerpt = excerpt

        # Re-validate after changes
        self.validate()

        self.updated_at = datetime.now()
        if updated_by:
            self.updated_by = updated_by
        self.version += 1

    def add_tag(self, tag_id: uuid.UUID) -> None:
        """Add a tag to the post."""
        if tag_id not in self.tag_ids:
            self.tag_ids.append(tag_id)
            self.updated_at = datetime.now()
            self.version += 1

    def remove_tag(self, tag_id: uuid.UUID) -> None:
        """Remove a tag from the post."""
        if tag_id in self.tag_ids:
            self.tag_ids.remove(tag_id)
            self.updated_at = datetime.now()
            self.version += 1

    def increment_view_count(self) -> None:
        """Increment the view count."""
        self.metadata.view_count += 1
        self.updated_at = datetime.now()
        self.version += 1

    def calculate_reading_time(self) -> int:
        """Calculate estimated reading time in minutes."""
        # Assume average reading speed of 200 words per minute
        word_count = len(self.content.split())
        reading_time = max(1, word_count // 200)
        self.metadata.reading_time_minutes = reading_time
        return reading_time

    def to_dict(self) -> dict:
        """Convert post to dictionary for serialization."""
        return {
            "id": str(self.id),
            "identifier": self.identifier,
            "title": self.title,
            "content": self.content,
            "excerpt": self.excerpt,
            "author_id": str(self.author_id),
            "tag_ids": [str(tag_id) for tag_id in self.tag_ids],
            "status": self.status.value,
            "published_at": self.published_at.isoformat() if self.published_at else None,
            "metadata": {
                "meta_title": self.metadata.meta_title,
                "meta_description": self.metadata.meta_description,
                "featured_image_url": self.metadata.featured_image_url,
                "reading_time_minutes": self.metadata.reading_time_minutes,
                "view_count": self.metadata.view_count,
                "like_count": self.metadata.like_count,
                "share_count": self.metadata.share_count,
            },
            "created_at": self.created_at.isoformat(),
            "created_by": str(self.created_by) if self.created_by else None,
            "updated_at": self.updated_at.isoformat(),
            "updated_by": str(self.updated_by) if self.updated_by else None,
            "version": self.version,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "Post":
        """Create post from dictionary."""
        return cls(
            id=uuid.UUID(data["id"]),
            identifier=data["identifier"],
            title=data["title"],
            content=data["content"],
            excerpt=data.get("excerpt"),
            author_id=uuid.UUID(data["author_id"]),
            tag_ids=[uuid.UUID(tag_id) for tag_id in data.get("tag_ids", [])],
            status=PostStatus(data.get("status", PostStatus.DRAFT.value)),
            published_at=datetime.fromisoformat(data["published_at"]) if data.get("published_at") else None,
            metadata=PostMetadata(
                meta_title=data.get("metadata", {}).get("meta_title"),
                meta_description=data.get("metadata", {}).get("meta_description"),
                featured_image_url=data.get("metadata", {}).get("featured_image_url"),
                reading_time_minutes=data.get("metadata", {}).get("reading_time_minutes"),
                view_count=data.get("metadata", {}).get("view_count", 0),
                like_count=data.get("metadata", {}).get("like_count", 0),
                share_count=data.get("metadata", {}).get("share_count", 0),
            ),
            created_at=datetime.fromisoformat(data["created_at"]),
            created_by=uuid.UUID(data["created_by"]) if data.get("created_by") else None,
            updated_at=datetime.fromisoformat(data["updated_at"]),
            updated_by=uuid.UUID(data["updated_by"]) if data.get("updated_by") else None,
            version=data.get("version", 1),
        )
