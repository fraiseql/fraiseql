"""
Tag domain entity.

Tag entity for categorizing and organizing blog content.
"""
from dataclasses import dataclass, field
from typing import Optional
from uuid import UUID

from ..common.base_classes import AggregateRoot, EntityId
from ..common.exceptions import DomainValidationError, BusinessRuleViolationError
from .value_objects import TagName, TagDescription, TagColor


class TagId(EntityId['Tag']):
    """Tag unique identifier."""
    pass


@dataclass
class Tag(AggregateRoot):
    """
    Tag aggregate root.

    Represents a tag for categorizing blog posts within an organization.
    """

    # Required fields
    name: TagName
    organization_id: UUID  # Reference to Organization

    # Optional fields with defaults
    description: TagDescription = field(default_factory=lambda: TagDescription(""))
    color: TagColor = field(default_factory=lambda: TagColor("gray"))
    is_featured: bool = field(default=False)
    post_count: int = field(default=0)  # Cached count, updated by domain services

    def __post_init__(self):
        """Initialize computed fields."""
        self._validate_post_count()

    def is_empty(self) -> bool:
        """Check if tag has no posts."""
        return self.post_count == 0

    def has_description(self) -> bool:
        """Check if tag has a description."""
        return not self.description.is_empty

    def update_info(self,
                   name: Optional[TagName] = None,
                   description: Optional[TagDescription] = None,
                   color: Optional[TagColor] = None) -> None:
        """Update tag information."""
        changed = False

        if name is not None and name != self.name:
            # Business rule: check for name conflicts would be handled by domain service
            self.name = name
            changed = True

        if description is not None and description != self.description:
            self.description = description
            changed = True

        if color is not None and color != self.color:
            self.color = color
            changed = True

        if changed:
            self._update_timestamp()

    def mark_as_featured(self) -> None:
        """Mark tag as featured."""
        if self.is_featured:
            return  # Already featured

        self.is_featured = True
        self._update_timestamp()

        # Domain event would be added here
        # self.add_domain_event(TagFeaturedEvent(self.id))

    def unmark_as_featured(self) -> None:
        """Remove featured status from tag."""
        if not self.is_featured:
            return  # Not featured

        self.is_featured = False
        self._update_timestamp()

        # Domain event would be added here
        # self.add_domain_event(TagUnfeaturedEvent(self.id))

    def increment_post_count(self) -> None:
        """Increment post count (when post is tagged)."""
        self.post_count += 1
        self._update_timestamp()

    def decrement_post_count(self) -> None:
        """Decrement post count (when post is untagged)."""
        if self.post_count > 0:
            self.post_count -= 1
            self._update_timestamp()

    def update_post_count(self, count: int) -> None:
        """Update post count directly (from domain service calculation)."""
        if count < 0:
            raise DomainValidationError("Post count cannot be negative")

        if count != self.post_count:
            self.post_count = count
            self._update_timestamp()

    def can_be_deleted(self) -> bool:
        """Check if tag can be deleted."""
        # Business rule: tags with posts might require special handling
        # This could be configurable based on organization preferences
        return self.post_count == 0

    def soft_delete(self) -> None:
        """Soft delete the tag."""
        if not self.can_be_deleted():
            raise BusinessRuleViolationError(
                f"Cannot delete tag '{self.name}' because it is used by {self.post_count} posts"
            )

        # Domain event would be added here
        # self.add_domain_event(TagDeletedEvent(self.id))

    def merge_into(self, target_tag: 'Tag') -> None:
        """Merge this tag into another tag (domain service operation)."""
        if target_tag.id == self.id:
            raise BusinessRuleViolationError("Cannot merge tag into itself")

        if target_tag.organization_id != self.organization_id:
            raise BusinessRuleViolationError("Cannot merge tags from different organizations")

        # The actual merging logic would be handled by domain service
        # This method just validates the business rules

        # Domain event would be added here
        # self.add_domain_event(TagMergeInitiatedEvent(self.id, target_tag.id, self.post_count))

    @property
    def slug(self) -> str:
        """Get URL-friendly slug for the tag."""
        return self.name.slug

    def _validate_post_count(self) -> None:
        """Validate post count."""
        if self.post_count < 0:
            raise DomainValidationError("Post count cannot be negative")


# Domain Service Example (would typically be in separate file)
class TagDomainService:
    """Domain service for tag-related operations that span multiple aggregates."""

    @staticmethod
    def can_merge_tags(source_tag: Tag, target_tag: Tag) -> bool:
        """Check if source tag can be merged into target tag."""
        if source_tag.id == target_tag.id:
            return False

        if source_tag.organization_id != target_tag.organization_id:
            return False

        # Could add more business rules here
        # e.g., similar names, compatible colors, etc.

        return True

    @staticmethod
    def suggest_similar_tags(tag: Tag, existing_tags: list[Tag]) -> list[Tag]:
        """Suggest similar tags based on name similarity."""
        similar_tags = []
        tag_name_lower = tag.name.value.lower()

        for existing_tag in existing_tags:
            if existing_tag.id == tag.id:
                continue

            existing_name_lower = existing_tag.name.value.lower()

            # Simple similarity check - could use more sophisticated algorithms
            if (tag_name_lower in existing_name_lower or
                existing_name_lower in tag_name_lower):
                similar_tags.append(existing_tag)

        return similar_tags
