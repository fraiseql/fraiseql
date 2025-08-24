"""
Comment domain entity.

Comment entity for hierarchical commenting on blog posts.
"""
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional, List
from uuid import UUID

from ..common.base_classes import AggregateRoot, EntityId
from ..common.exceptions import DomainValidationError, BusinessRuleViolationError
from .value_objects import Content


class CommentId(EntityId['Comment']):
    """Comment unique identifier."""
    pass


class CommentStatus(Enum):
    """Comment status enumeration."""
    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    SPAM = "spam"
    DELETED = "deleted"
    
    def is_visible(self) -> bool:
        """Check if comment should be visible to public."""
        return self == self.APPROVED
    
    def is_pending_moderation(self) -> bool:
        """Check if comment is pending moderation."""
        return self == self.PENDING
    
    def can_be_replied_to(self) -> bool:
        """Check if comment can receive replies."""
        return self in {self.APPROVED}


@dataclass
class Comment(AggregateRoot):
    """
    Comment aggregate root.
    
    Represents a comment on a blog post with hierarchical structure support.
    """
    
    # Required fields
    content: Content
    post_id: UUID  # Reference to Post
    author_id: UUID  # Reference to User
    organization_id: UUID  # Reference to Organization
    
    # Optional fields with defaults
    parent_comment_id: Optional[UUID] = field(default=None)  # For hierarchical comments
    status: CommentStatus = field(default=CommentStatus.PENDING)
    author_name: Optional[str] = field(default=None)  # For guest comments
    author_email: Optional[str] = field(default=None)  # For guest comments
    author_website: Optional[str] = field(default=None)  # For guest comments
    ip_address: Optional[str] = field(default=None)  # For spam protection
    user_agent: Optional[str] = field(default=None)  # For spam protection
    
    def __post_init__(self):
        """Initialize computed fields."""
        self._validate_hierarchy()
        self._validate_guest_fields()
    
    def is_visible(self) -> bool:
        """Check if comment is visible to public."""
        return self.status.is_visible()
    
    def is_pending_moderation(self) -> bool:
        """Check if comment is pending moderation."""
        return self.status.is_pending_moderation()
    
    def is_reply(self) -> bool:
        """Check if comment is a reply to another comment."""
        return self.parent_comment_id is not None
    
    def is_top_level(self) -> bool:
        """Check if comment is top-level (not a reply)."""
        return self.parent_comment_id is None
    
    def can_be_replied_to(self) -> bool:
        """Check if this comment can receive replies."""
        return self.status.can_be_replied_to()
    
    def is_guest_comment(self) -> bool:
        """Check if comment is from a guest (non-registered user)."""
        return self.author_name is not None
    
    def approve(self) -> None:
        """Approve the comment for public visibility."""
        if self.status == CommentStatus.APPROVED:
            return  # Already approved
        
        if self.status == CommentStatus.DELETED:
            raise BusinessRuleViolationError("Cannot approve deleted comment")
        
        self.status = CommentStatus.APPROVED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(CommentApprovedEvent(self.id))
    
    def reject(self, reason: str = "") -> None:
        """Reject the comment."""
        if self.status == CommentStatus.DELETED:
            raise BusinessRuleViolationError("Cannot reject deleted comment")
        
        self.status = CommentStatus.REJECTED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(CommentRejectedEvent(self.id, reason))
    
    def mark_as_spam(self) -> None:
        """Mark comment as spam."""
        if self.status == CommentStatus.DELETED:
            raise BusinessRuleViolationError("Cannot mark deleted comment as spam")
        
        self.status = CommentStatus.SPAM
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(CommentMarkedAsSpamEvent(self.id))
    
    def soft_delete(self) -> None:
        """Soft delete the comment."""
        if self.status == CommentStatus.DELETED:
            return  # Already deleted
        
        self.status = CommentStatus.DELETED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(CommentDeletedEvent(self.id))
    
    def update_content(self, new_content: Content) -> None:
        """Update comment content (author can edit within time limit)."""
        if self.status == CommentStatus.DELETED:
            raise BusinessRuleViolationError("Cannot update deleted comment")
        
        if new_content == self.content:
            return  # No change
        
        self.content = new_content
        
        # Business rule: editing approved comment might require re-moderation
        if self.status == CommentStatus.APPROVED:
            self.status = CommentStatus.PENDING
        
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(CommentUpdatedEvent(self.id))
    
    def set_guest_info(self, name: str, email: str, website: Optional[str] = None) -> None:
        """Set guest author information for non-registered users."""
        if not name or not name.strip():
            raise DomainValidationError("Guest name cannot be empty")
        
        if len(name.strip()) > 100:
            raise DomainValidationError("Guest name cannot exceed 100 characters")
        
        if not email or not email.strip():
            raise DomainValidationError("Guest email cannot be empty")
        
        # Basic email validation
        if '@' not in email or len(email) > 254:
            raise DomainValidationError("Invalid guest email format")
        
        self.author_name = name.strip()
        self.author_email = email.strip().lower()
        
        if website:
            if len(website) > 500:
                raise DomainValidationError("Website URL cannot exceed 500 characters")
            
            if not (website.startswith('http://') or website.startswith('https://')):
                raise DomainValidationError("Website must be a valid HTTP/HTTPS URL")
            
            self.author_website = website
    
    def set_tracking_info(self, ip_address: str, user_agent: str) -> None:
        """Set tracking information for spam protection."""
        if ip_address and len(ip_address) > 45:  # IPv6 can be up to 45 chars
            raise DomainValidationError("IP address format invalid")
        
        if user_agent and len(user_agent) > 500:
            raise DomainValidationError("User agent string too long")
        
        self.ip_address = ip_address
        self.user_agent = user_agent
    
    def _validate_hierarchy(self) -> None:
        """Validate comment hierarchy rules."""
        # Business rule: prevent deep nesting (could be configurable)
        # This would typically require repository access to check parent depth
        # For now, just ensure we have consistent parent reference
        pass
    
    def _validate_guest_fields(self) -> None:
        """Validate guest comment fields consistency."""
        # Business rule: either we have author_id OR guest info, not both
        has_user = self.author_id is not None
        has_guest_info = self.author_name is not None
        
        if has_user and has_guest_info:
            raise DomainValidationError("Comment cannot have both user ID and guest information")
        
        if not has_user and not has_guest_info:
            raise DomainValidationError("Comment must have either user ID or guest information")