"""
User domain entity.

Core user entity for multi-tenant blog platform.
"""
from dataclasses import dataclass, field
from enum import Enum
from typing import Optional
from uuid import UUID

from ..common.base_classes import AggregateRoot, EntityId
from ..common.exceptions import DomainValidationError, BusinessRuleViolationError
from .value_objects import Email, Username, FullName


class UserId(EntityId['User']):
    """User unique identifier."""
    pass


class UserRole(Enum):
    """User role enumeration."""
    READER = "reader"
    AUTHOR = "author"
    EDITOR = "editor"
    ADMIN = "admin"
    
    def can_create_posts(self) -> bool:
        """Check if role can create posts."""
        return self in {self.AUTHOR, self.EDITOR, self.ADMIN}
    
    def can_edit_posts(self) -> bool:
        """Check if role can edit posts."""
        return self in {self.EDITOR, self.ADMIN}
    
    def can_delete_posts(self) -> bool:
        """Check if role can delete posts."""
        return self in {self.ADMIN}
    
    def can_manage_users(self) -> bool:
        """Check if role can manage users."""
        return self == self.ADMIN
    
    def can_moderate_comments(self) -> bool:
        """Check if role can moderate comments."""
        return self in {self.EDITOR, self.ADMIN}


class UserStatus(Enum):
    """User status enumeration."""
    ACTIVE = "active"
    INACTIVE = "inactive"
    SUSPENDED = "suspended"
    DELETED = "deleted"
    
    def is_active(self) -> bool:
        """Check if user is active."""
        return self == self.ACTIVE
    
    def can_login(self) -> bool:
        """Check if user can login."""
        return self in {self.ACTIVE, self.INACTIVE}
    
    def can_create_content(self) -> bool:
        """Check if user can create content."""
        return self == self.ACTIVE


@dataclass
class User(AggregateRoot):
    """
    User aggregate root.
    
    Represents a user within a multi-tenant blog organization.
    """
    
    # Required fields
    username: Username
    email: Email
    full_name: FullName
    organization_id: UUID  # Reference to organization
    
    # Optional fields with defaults
    role: UserRole = field(default=UserRole.READER)
    status: UserStatus = field(default=UserStatus.ACTIVE)
    bio: Optional[str] = field(default=None)
    avatar_url: Optional[str] = field(default=None)
    website_url: Optional[str] = field(default=None)
    last_login_at: Optional[str] = field(default=None)  # ISO datetime string
    
    def __post_init__(self):
        """Initialize computed fields."""
        self._validate_bio()
        self._validate_urls()
    
    def is_active(self) -> bool:
        """Check if user is active."""
        return self.status.is_active()
    
    def can_login(self) -> bool:
        """Check if user can login."""
        return self.status.can_login()
    
    def can_create_posts(self) -> bool:
        """Check if user can create posts."""
        return self.status.can_create_content() and self.role.can_create_posts()
    
    def can_edit_posts(self) -> bool:
        """Check if user can edit posts."""
        return self.status.can_create_content() and self.role.can_edit_posts()
    
    def can_delete_posts(self) -> bool:
        """Check if user can delete posts."""
        return self.status.can_create_content() and self.role.can_delete_posts()
    
    def can_manage_users(self) -> bool:
        """Check if user can manage other users."""
        return self.status.can_create_content() and self.role.can_manage_users()
    
    def can_moderate_comments(self) -> bool:
        """Check if user can moderate comments."""
        return self.status.can_create_content() and self.role.can_moderate_comments()
    
    def update_profile(self, 
                      full_name: Optional[FullName] = None,
                      bio: Optional[str] = None,
                      avatar_url: Optional[str] = None,
                      website_url: Optional[str] = None) -> None:
        """Update user profile information."""
        if full_name is not None:
            self.full_name = full_name
        
        if bio is not None:
            self.bio = bio
            self._validate_bio()
        
        if avatar_url is not None:
            self.avatar_url = avatar_url
            self._validate_urls()
        
        if website_url is not None:
            self.website_url = website_url
            self._validate_urls()
        
        self._update_timestamp()
    
    def change_role(self, new_role: UserRole) -> None:
        """Change user role (admin action)."""
        if new_role == self.role:
            return  # No change needed
        
        old_role = self.role
        self.role = new_role
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(UserRoleChangedEvent(self.id, old_role, new_role))
    
    def suspend(self, reason: str) -> None:
        """Suspend user account."""
        if self.status == UserStatus.SUSPENDED:
            raise BusinessRuleViolationError("User is already suspended")
        
        if self.status == UserStatus.DELETED:
            raise BusinessRuleViolationError("Cannot suspend deleted user")
        
        self.status = UserStatus.SUSPENDED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(UserSuspendedEvent(self.id, reason))
    
    def reactivate(self) -> None:
        """Reactivate suspended user account."""
        if self.status != UserStatus.SUSPENDED:
            raise BusinessRuleViolationError("Can only reactivate suspended users")
        
        self.status = UserStatus.ACTIVE
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(UserReactivatedEvent(self.id))
    
    def deactivate(self) -> None:
        """Deactivate user account (soft disable)."""
        if self.status == UserStatus.DELETED:
            raise BusinessRuleViolationError("Cannot deactivate deleted user")
        
        self.status = UserStatus.INACTIVE
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(UserDeactivatedEvent(self.id))
    
    def soft_delete(self) -> None:
        """Soft delete user account."""
        if self.status == UserStatus.DELETED:
            raise BusinessRuleViolationError("User is already deleted")
        
        self.status = UserStatus.DELETED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(UserDeletedEvent(self.id))
    
    def record_login(self, login_timestamp: str) -> None:
        """Record user login timestamp."""
        self.last_login_at = login_timestamp
        self._update_timestamp()
    
    def _validate_bio(self) -> None:
        """Validate user bio."""
        if self.bio and len(self.bio) > 500:
            raise DomainValidationError("Bio cannot exceed 500 characters")
    
    def _validate_urls(self) -> None:
        """Validate URLs."""
        urls_to_check = [
            ("avatar_url", self.avatar_url),
            ("website_url", self.website_url)
        ]
        
        for field_name, url in urls_to_check:
            if url and len(url) > 500:
                raise DomainValidationError(f"{field_name} cannot exceed 500 characters")
            
            # Basic URL validation (starts with http/https)
            if url and not (url.startswith('http://') or url.startswith('https://')):
                raise DomainValidationError(f"{field_name} must be a valid HTTP/HTTPS URL")