"""
Post domain entity.

Core blog post entity for multi-tenant blog platform.
"""
from dataclasses import dataclass, field
from typing import Optional, Set
from uuid import UUID

from ..common.base_classes import AggregateRoot, EntityId
from ..common.exceptions import DomainValidationError, BusinessRuleViolationError
from .value_objects import Title, Slug, Content, PostStatus


class PostId(EntityId['Post']):
    """Post unique identifier."""
    pass


@dataclass
class Post(AggregateRoot):
    """
    Post aggregate root.
    
    Represents a blog post within a multi-tenant organization.
    """
    
    # Required fields
    title: Title
    slug: Slug
    content: Content
    author_id: UUID  # Reference to User
    organization_id: UUID  # Reference to Organization
    
    # Optional fields with defaults
    status: PostStatus = field(default=PostStatus("draft"))
    excerpt: Optional[str] = field(default=None)
    featured_image_url: Optional[str] = field(default=None)
    published_at: Optional[str] = field(default=None)  # ISO datetime string
    meta_description: Optional[str] = field(default=None)
    meta_keywords: Set[str] = field(default_factory=set)
    reading_time_minutes: Optional[int] = field(default=None)
    
    def __post_init__(self):
        """Initialize computed fields."""
        self._validate_excerpt()
        self._validate_meta_fields()
        self._calculate_reading_time()
        if not self.excerpt:
            self.excerpt = self._generate_excerpt()
    
    def is_published(self) -> bool:
        """Check if post is published."""
        return self.status.is_published()
    
    def is_draft(self) -> bool:
        """Check if post is draft."""
        return self.status.is_draft()
    
    def can_be_published(self) -> bool:
        """Check if post can be published."""
        # Business rule: post must have title, content, and not be deleted
        return (self.title.value.strip() and 
                self.content.value.strip() and 
                not self.status.is_deleted())
    
    def publish(self, published_at: str) -> None:
        """Publish the post."""
        if not self.can_be_published():
            raise BusinessRuleViolationError("Post cannot be published in current state")
        
        if self.is_published():
            raise BusinessRuleViolationError("Post is already published")
        
        self.status = PostStatus("published")
        self.published_at = published_at
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(PostPublishedEvent(self.id, published_at))
    
    def unpublish(self) -> None:
        """Unpublish the post (revert to draft)."""
        if not self.is_published():
            raise BusinessRuleViolationError("Post is not published")
        
        self.status = PostStatus("draft")
        self.published_at = None
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(PostUnpublishedEvent(self.id))
    
    def archive(self) -> None:
        """Archive the post."""
        if self.status.is_deleted():
            raise BusinessRuleViolationError("Cannot archive deleted post")
        
        if self.status.is_archived():
            return  # Already archived
        
        self.status = PostStatus("archived")
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(PostArchivedEvent(self.id))
    
    def soft_delete(self) -> None:
        """Soft delete the post."""
        if self.status.is_deleted():
            return  # Already deleted
        
        self.status = PostStatus("deleted")
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(PostDeletedEvent(self.id))
    
    def update_content(self, 
                      title: Optional[Title] = None,
                      content: Optional[Content] = None,
                      excerpt: Optional[str] = None,
                      meta_description: Optional[str] = None) -> None:
        """Update post content."""
        if self.status.is_deleted():
            raise BusinessRuleViolationError("Cannot update deleted post")
        
        content_changed = False
        
        if title is not None and title != self.title:
            old_title = self.title
            self.title = title
            # Auto-generate new slug if title changed significantly
            new_slug = Slug.from_title(title.value)
            if new_slug != self.slug:
                self.slug = new_slug
            content_changed = True
        
        if content is not None and content != self.content:
            self.content = content
            self._calculate_reading_time()
            # Auto-generate excerpt if not provided and content changed
            if not excerpt and not self.excerpt:
                self.excerpt = self._generate_excerpt()
            content_changed = True
        
        if excerpt is not None:
            self.excerpt = excerpt
            self._validate_excerpt()
        
        if meta_description is not None:
            self.meta_description = meta_description
            self._validate_meta_fields()
        
        if content_changed:
            # If published post is significantly changed, consider unpublishing
            # This could be a business rule depending on requirements
            self._update_timestamp()
    
    def update_slug(self, new_slug: Slug) -> None:
        """Update post slug (admin action)."""
        if new_slug == self.slug:
            return  # No change
        
        # Business rule: slug changes should be careful for SEO
        old_slug = self.slug
        self.slug = new_slug
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(PostSlugChangedEvent(self.id, old_slug, new_slug))
    
    def update_featured_image(self, image_url: Optional[str]) -> None:
        """Update featured image."""
        if image_url and len(image_url) > 500:
            raise DomainValidationError("Featured image URL cannot exceed 500 characters")
        
        if image_url and not (image_url.startswith('http://') or image_url.startswith('https://')):
            raise DomainValidationError("Featured image URL must be a valid HTTP/HTTPS URL")
        
        self.featured_image_url = image_url
        self._update_timestamp()
    
    def update_meta_keywords(self, keywords: Set[str]) -> None:
        """Update meta keywords for SEO."""
        if len(keywords) > 10:
            raise DomainValidationError("Cannot have more than 10 meta keywords")
        
        # Validate each keyword
        for keyword in keywords:
            if not keyword.strip():
                raise DomainValidationError("Meta keywords cannot be empty")
            if len(keyword.strip()) > 50:
                raise DomainValidationError("Meta keyword cannot exceed 50 characters")
        
        self.meta_keywords = {k.strip().lower() for k in keywords}
        self._update_timestamp()
    
    def _generate_excerpt(self) -> str:
        """Generate excerpt from content."""
        return self.content.excerpt(150)
    
    def _calculate_reading_time(self) -> None:
        """Calculate estimated reading time."""
        words_per_minute = 200  # Average reading speed
        word_count = self.content.word_count
        self.reading_time_minutes = max(1, round(word_count / words_per_minute))
    
    def _validate_excerpt(self) -> None:
        """Validate excerpt length."""
        if self.excerpt and len(self.excerpt) > 300:
            raise DomainValidationError("Excerpt cannot exceed 300 characters")
    
    def _validate_meta_fields(self) -> None:
        """Validate meta description and related fields."""
        if self.meta_description and len(self.meta_description) > 160:
            raise DomainValidationError("Meta description cannot exceed 160 characters")