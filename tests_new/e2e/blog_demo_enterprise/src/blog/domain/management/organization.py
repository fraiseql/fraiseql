"""
Organization domain entity.

Core business entity for multi-tenant blog organizations.
"""
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, Any, Optional
from uuid import UUID

from ..common.base_classes import AggregateRoot, EntityId
from ..common.exceptions import DomainValidationError, BusinessRuleViolationError
from .value_objects import OrganizationName, OrganizationIdentifier, ContactEmail


class OrganizationId(EntityId['Organization']):
    """Organization unique identifier."""
    pass


class SubscriptionPlan(Enum):
    """Subscription plan enumeration."""
    STARTER = "starter"
    PROFESSIONAL = "professional"
    ENTERPRISE = "enterprise" 
    CUSTOM = "custom"
    
    @property
    def max_users(self) -> float:
        """Maximum users allowed for this plan."""
        limits = {
            self.STARTER: 5,
            self.PROFESSIONAL: 50,
            self.ENTERPRISE: float('inf'),
            self.CUSTOM: float('inf')
        }
        return limits[self]
    
    @property
    def max_posts_per_month(self) -> float:
        """Maximum posts per month for this plan."""
        limits = {
            self.STARTER: 50,
            self.PROFESSIONAL: 500,
            self.ENTERPRISE: float('inf'),
            self.CUSTOM: float('inf')
        }
        return limits[self]
    
    @property
    def max_storage_mb(self) -> float:
        """Maximum storage in MB for this plan."""
        limits = {
            self.STARTER: 100,
            self.PROFESSIONAL: 1000,
            self.ENTERPRISE: 10000,
            self.CUSTOM: float('inf')
        }
        return limits[self]
    
    def can_upgrade_to(self, other: 'SubscriptionPlan') -> bool:
        """Check if can upgrade to another plan."""
        hierarchy = {
            self.STARTER: 0,
            self.PROFESSIONAL: 1,
            self.ENTERPRISE: 2,
            self.CUSTOM: 3
        }
        return hierarchy[other] > hierarchy[self]


class OrganizationStatus(Enum):
    """Organization status enumeration."""
    TRIAL = "trial"
    ACTIVE = "active"
    SUSPENDED = "suspended"
    CANCELLED = "cancelled"
    
    def is_active(self) -> bool:
        """Check if organization is in active state."""
        return self in {self.TRIAL, self.ACTIVE}
    
    def is_suspended(self) -> bool:
        """Check if organization is suspended."""
        return self == self.SUSPENDED
    
    def can_create_content(self) -> bool:
        """Check if organization can create content."""
        return self in {self.TRIAL, self.ACTIVE}


@dataclass
class Organization(AggregateRoot):
    """
    Organization aggregate root.
    
    Represents a multi-tenant blog organization with subscription management,
    user limits, and business rules enforcement.
    """
    
    # Required fields - using field() for all to work with dataclass inheritance
    name: OrganizationName
    identifier: OrganizationIdentifier
    contact_email: ContactEmail
    subscription_plan: SubscriptionPlan
    
    # Optional fields with defaults
    status: OrganizationStatus = field(default=OrganizationStatus.ACTIVE)
    website_url: Optional[str] = field(default=None)
    settings: Dict[str, Any] = field(default_factory=lambda: {
        'theme': 'default',
        'allow_user_registration': True,
        'moderation_required': False,
        'custom_domain': None
    })
    limits: Dict[str, Any] = field(default_factory=dict)
    
    def __post_init__(self):
        """Initialize computed fields."""
        self._sync_limits_with_plan()
    
    def is_trial(self) -> bool:
        """Check if organization is in trial mode."""
        return self.status == OrganizationStatus.TRIAL
    
    def can_create_posts(self) -> bool:
        """Check if organization can create posts."""
        return self.status.can_create_content()
    
    def can_create_users(self) -> bool:
        """Check if organization can create users."""
        return self.status.can_create_content()
    
    @property
    def max_users(self) -> float:
        """Get maximum users allowed."""
        return self.subscription_plan.max_users
    
    @property
    def max_posts_per_month(self) -> float:
        """Get maximum posts per month allowed."""
        return self.subscription_plan.max_posts_per_month
    
    @property
    def max_storage_mb(self) -> float:
        """Get maximum storage allowed in MB."""
        return self.subscription_plan.max_storage_mb
    
    def can_add_user(self, current_user_count: int) -> bool:
        """Check if organization can add another user."""
        if not self.can_create_users():
            return False
        return current_user_count < self.max_users
    
    def can_create_post(self, current_month_posts: int) -> bool:
        """Check if organization can create another post this month."""
        if not self.can_create_posts():
            return False
        return current_month_posts < self.max_posts_per_month
    
    def upgrade_subscription(self, new_plan: SubscriptionPlan) -> None:
        """Upgrade subscription plan."""
        if not self.subscription_plan.can_upgrade_to(new_plan):
            raise DomainValidationError(
                f"Cannot downgrade subscription from {self.subscription_plan.value} to {new_plan.value}"
            )
        
        if self.status == OrganizationStatus.CANCELLED:
            raise BusinessRuleViolationError("Cannot upgrade subscription for cancelled organization")
        
        old_plan = self.subscription_plan
        self.subscription_plan = new_plan
        self._sync_limits_with_plan()
        self._update_timestamp()
        
        # Domain event would be added here in full implementation
        # self.add_domain_event(SubscriptionUpgradedEvent(self.id, old_plan, new_plan))
    
    def suspend(self, reason: str) -> None:
        """Suspend organization."""
        if self.status == OrganizationStatus.CANCELLED:
            raise BusinessRuleViolationError("Cannot suspend cancelled organization")
        
        self.status = OrganizationStatus.SUSPENDED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(OrganizationSuspendedEvent(self.id, reason))
    
    def reactivate(self) -> None:
        """Reactivate suspended organization."""
        if self.status != OrganizationStatus.SUSPENDED:
            raise BusinessRuleViolationError("Can only reactivate suspended organizations")
        
        self.status = OrganizationStatus.ACTIVE
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(OrganizationReactivatedEvent(self.id))
    
    def cancel(self, reason: str) -> None:
        """Cancel organization."""
        if self.status == OrganizationStatus.CANCELLED:
            raise BusinessRuleViolationError("Organization is already cancelled")
        
        self.status = OrganizationStatus.CANCELLED
        self._update_timestamp()
        
        # Domain event would be added here
        # self.add_domain_event(OrganizationCancelledEvent(self.id, reason))
    
    def update_contact_info(self, contact_email: Optional[ContactEmail] = None, 
                           website_url: Optional[str] = None) -> None:
        """Update organization contact information."""
        if contact_email is not None:
            self.contact_email = contact_email
        
        if website_url is not None:
            self.website_url = website_url
        
        self._update_timestamp()
    
    def update_settings(self, settings: Dict[str, Any]) -> None:
        """Update organization settings."""
        self.settings.update(settings)
        self._update_timestamp()
    
    def _sync_limits_with_plan(self) -> None:
        """Sync limits with current subscription plan."""
        self.limits = {
            'max_users': self.subscription_plan.max_users,
            'max_posts_per_month': self.subscription_plan.max_posts_per_month,
            'max_storage_mb': self.subscription_plan.max_storage_mb,
            'max_api_requests_per_day': self._get_api_limit()
        }
    
    def _get_api_limit(self) -> float:
        """Get API request limit for current plan."""
        api_limits = {
            SubscriptionPlan.STARTER: 1000,
            SubscriptionPlan.PROFESSIONAL: 10000,
            SubscriptionPlan.ENTERPRISE: 100000,
            SubscriptionPlan.CUSTOM: float('inf')
        }
        return api_limits[self.subscription_plan]