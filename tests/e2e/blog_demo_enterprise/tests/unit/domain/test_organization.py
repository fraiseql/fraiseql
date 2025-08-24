"""
Unit tests for Organization domain entity.

Tests the pure business logic of Organization entity without database dependencies:
- Organization creation with validation
- Business rule enforcement
- Subscription plan management
- Status transitions
"""
import pytest
from datetime import datetime
from uuid import UUID, uuid4
from typing import Dict, Any

# These imports will fail until we create the domain layer
from blog.domain.management.organization import Organization, OrganizationId, SubscriptionPlan
from blog.domain.management.value_objects import OrganizationName, OrganizationIdentifier, ContactEmail
from blog.domain.common.exceptions import DomainValidationError


class TestOrganization:
    """Test Organization entity business logic."""
    
    def test_create_organization_with_valid_data(self):
        """Test creating an organization with valid data."""
        org_id = OrganizationId(uuid4())
        name = OrganizationName("TechBlog Corp")
        identifier = OrganizationIdentifier("techblog")
        email = ContactEmail("admin@techblog.com")
        plan = SubscriptionPlan.PROFESSIONAL
        
        organization = Organization(
            id=org_id,
            name=name,
            identifier=identifier,
            contact_email=email,
            subscription_plan=plan
        )
        
        assert organization.id == org_id
        assert organization.name == name
        assert organization.identifier == identifier
        assert organization.contact_email == email
        assert organization.subscription_plan == plan
        assert organization.status.is_active()
        assert not organization.is_trial()
    
    def test_create_organization_with_invalid_name(self):
        """Test organization creation fails with invalid name."""
        with pytest.raises(DomainValidationError, match="Organization name cannot be empty"):
            OrganizationName("")
    
    def test_create_organization_with_invalid_identifier(self):
        """Test organization creation fails with invalid identifier."""
        with pytest.raises(DomainValidationError, match="Invalid organization identifier format"):
            OrganizationIdentifier("Invalid Identifier!")
    
    def test_create_organization_with_invalid_email(self):
        """Test organization creation fails with invalid email."""
        with pytest.raises(DomainValidationError, match="Invalid email format"):
            ContactEmail("invalid-email")
    
    def test_organization_can_upgrade_subscription(self):
        """Test organization can upgrade subscription plan."""
        organization = self._create_test_organization(SubscriptionPlan.STARTER)
        
        organization.upgrade_subscription(SubscriptionPlan.PROFESSIONAL)
        
        assert organization.subscription_plan == SubscriptionPlan.PROFESSIONAL
    
    def test_organization_cannot_downgrade_subscription(self):
        """Test organization cannot downgrade subscription plan."""
        organization = self._create_test_organization(SubscriptionPlan.PROFESSIONAL)
        
        with pytest.raises(DomainValidationError, match="Cannot downgrade subscription"):
            organization.upgrade_subscription(SubscriptionPlan.STARTER)
    
    def test_organization_can_be_suspended(self):
        """Test organization can be suspended."""
        organization = self._create_test_organization()
        
        organization.suspend("Payment overdue")
        
        assert organization.status.is_suspended()
        assert not organization.can_create_posts()
        assert not organization.can_create_users()
    
    def test_suspended_organization_can_be_reactivated(self):
        """Test suspended organization can be reactivated."""
        organization = self._create_test_organization()
        organization.suspend("Payment overdue")
        
        organization.reactivate()
        
        assert organization.status.is_active()
        assert organization.can_create_posts()
        assert organization.can_create_users()
    
    def test_organization_enforces_user_limits(self):
        """Test organization enforces user limits based on subscription."""
        starter_org = self._create_test_organization(SubscriptionPlan.STARTER)
        
        assert starter_org.max_users == 5
        assert starter_org.can_add_user(current_user_count=4)
        assert not starter_org.can_add_user(current_user_count=5)
    
    def test_organization_enforces_post_limits(self):
        """Test organization enforces monthly post limits."""
        starter_org = self._create_test_organization(SubscriptionPlan.STARTER)
        
        assert starter_org.max_posts_per_month == 50
        assert starter_org.can_create_post(current_month_posts=49)
        assert not starter_org.can_create_post(current_month_posts=50)
    
    def test_enterprise_organization_has_unlimited_resources(self):
        """Test enterprise organizations have unlimited resources."""
        enterprise_org = self._create_test_organization(SubscriptionPlan.ENTERPRISE)
        
        assert enterprise_org.max_users == float('inf')
        assert enterprise_org.max_posts_per_month == float('inf')
        assert enterprise_org.can_add_user(current_user_count=1000)
        assert enterprise_org.can_create_post(current_month_posts=1000)
    
    def _create_test_organization(self, plan: SubscriptionPlan = SubscriptionPlan.STARTER) -> Organization:
        """Helper to create a test organization."""
        return Organization(
            id=OrganizationId(uuid4()),
            name=OrganizationName("Test Corp"),
            identifier=OrganizationIdentifier("testcorp"),
            contact_email=ContactEmail("admin@testcorp.com"),
            subscription_plan=plan
        )