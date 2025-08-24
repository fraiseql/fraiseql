"""
Tests for User domain entity.

Testing the business logic and behavior of the User aggregate root.
"""
import pytest
from uuid import uuid4

from blog.domain.users.user import User, UserId, UserRole, UserStatus
from blog.domain.users.value_objects import Email, Username, FullName
from blog.domain.common.exceptions import DomainValidationError, BusinessRuleViolationError


class TestUser:
    """Test User domain entity."""
    
    def test_create_user_with_valid_data(self):
        """Test creating a user with valid data."""
        user_id = UserId(uuid4())
        username = Username("johndoe")
        email = Email("john@example.com")
        full_name = FullName("John Doe")
        org_id = uuid4()
        
        user = User(
            id=user_id,
            username=username,
            email=email,
            full_name=full_name,
            organization_id=org_id
        )
        
        assert user.username == username
        assert user.email == email
        assert user.full_name == full_name
        assert user.role == UserRole.READER
        assert user.status == UserStatus.ACTIVE
        assert user.is_active()
        assert user.can_login()
    
    def test_user_role_permissions(self):
        """Test user role permissions."""
        user = self._create_test_user(role=UserRole.AUTHOR)
        
        assert user.can_create_posts()
        assert not user.can_edit_posts()
        assert not user.can_delete_posts()
        assert not user.can_manage_users()
        
        user.change_role(UserRole.ADMIN)
        assert user.can_create_posts()
        assert user.can_edit_posts()
        assert user.can_delete_posts()
        assert user.can_manage_users()
    
    def test_user_status_effects(self):
        """Test user status effects on permissions."""
        user = self._create_test_user(role=UserRole.AUTHOR)
        
        # Active user can create content
        assert user.can_create_posts()
        
        # Suspended user cannot create content
        user.suspend("Violation of terms")
        assert not user.can_create_posts()
        assert not user.can_login()
        
        # Reactivated user can create content again
        user.reactivate()
        assert user.can_create_posts()
        assert user.can_login()
    
    def test_user_profile_update(self):
        """Test user profile updates."""
        user = self._create_test_user()
        
        new_name = FullName("Jane Smith")
        user.update_profile(
            full_name=new_name,
            bio="Software developer",
            website_url="https://example.com"
        )
        
        assert user.full_name == new_name
        assert user.bio == "Software developer"
        assert user.website_url == "https://example.com"
    
    def test_user_bio_validation(self):
        """Test user bio length validation."""
        user = self._create_test_user()
        
        with pytest.raises(DomainValidationError, match="Bio cannot exceed 500 characters"):
            user.update_profile(bio="x" * 501)
    
    def test_user_url_validation(self):
        """Test URL validation."""
        user = self._create_test_user()
        
        with pytest.raises(DomainValidationError, match="must be a valid HTTP/HTTPS URL"):
            user.update_profile(website_url="invalid-url")
        
        with pytest.raises(DomainValidationError, match="cannot exceed 500 characters"):
            user.update_profile(avatar_url="https://" + "x" * 500 + ".com")
    
    def test_user_suspension_business_rules(self):
        """Test user suspension business rules."""
        user = self._create_test_user()
        
        # Can suspend active user
        user.suspend("Terms violation")
        assert user.status == UserStatus.SUSPENDED
        
        # Cannot suspend already suspended user
        with pytest.raises(BusinessRuleViolationError, match="already suspended"):
            user.suspend("Another reason")
        
        # Cannot suspend deleted user
        user.soft_delete()
        with pytest.raises(BusinessRuleViolationError, match="Cannot suspend deleted user"):
            user.suspend("Reason")
    
    def test_user_reactivation_business_rules(self):
        """Test user reactivation business rules."""
        user = self._create_test_user()
        
        # Cannot reactivate non-suspended user
        with pytest.raises(BusinessRuleViolationError, match="Can only reactivate suspended users"):
            user.reactivate()
        
        # Can reactivate suspended user
        user.suspend("Test")
        user.reactivate()
        assert user.status == UserStatus.ACTIVE
    
    def test_user_role_change_tracking(self):
        """Test user role changes."""
        user = self._create_test_user(role=UserRole.READER)
        
        # Change role
        user.change_role(UserRole.AUTHOR)
        assert user.role == UserRole.AUTHOR
        
        # No change if same role
        old_version = user.version
        user.change_role(UserRole.AUTHOR)
        assert user.version == old_version  # No timestamp update
    
    def test_user_login_tracking(self):
        """Test user login timestamp tracking."""
        user = self._create_test_user()
        
        login_time = "2025-01-01T12:00:00Z"
        user.record_login(login_time)
        assert user.last_login_at == login_time
    
    def _create_test_user(self, role: UserRole = UserRole.READER) -> User:
        """Create a test user with default values."""
        return User(
            id=UserId(uuid4()),
            username=Username("testuser"),
            email=Email("test@example.com"),
            full_name=FullName("Test User"),
            organization_id=uuid4(),
            role=role
        )