"""
Tests for domain value objects.

Testing the validation and behavior of all domain value objects.
"""
import pytest

from blog.domain.users.value_objects import Email, Username, FullName
from blog.domain.content.value_objects import Slug, Title, Content, PostStatus
from blog.domain.taxonomy.value_objects import TagName, TagDescription, TagColor
from blog.domain.management.value_objects import OrganizationName, OrganizationIdentifier, ContactEmail
from blog.domain.common.exceptions import DomainValidationError


class TestUserValueObjects:
    """Test user domain value objects."""
    
    def test_email_valid(self):
        """Test valid email addresses."""
        email = Email("user@example.com")
        assert str(email) == "user@example.com"
        assert email.domain == "example.com"
        assert email.local_part == "user"
    
    def test_email_normalization(self):
        """Test email normalization."""
        email = Email("  User@EXAMPLE.COM  ")
        assert str(email) == "user@example.com"
    
    def test_email_invalid_format(self):
        """Test invalid email formats."""
        with pytest.raises(DomainValidationError, match="Invalid email format"):
            Email("invalid-email")
        
        with pytest.raises(DomainValidationError, match="Invalid email format"):
            Email("@example.com")
        
        with pytest.raises(DomainValidationError, match="Invalid email format"):
            Email("user@")
    
    def test_email_empty(self):
        """Test empty email."""
        with pytest.raises(DomainValidationError, match="Email cannot be empty"):
            Email("")
        
        with pytest.raises(DomainValidationError, match="Email cannot be empty"):
            Email("   ")
    
    def test_username_valid(self):
        """Test valid usernames."""
        username = Username("john_doe")
        assert str(username) == "john_doe"
    
    def test_username_normalization(self):
        """Test username normalization."""
        username = Username("  JohnDoe123  ")
        assert str(username) == "johndoe123"
    
    def test_username_invalid_characters(self):
        """Test invalid username characters."""
        with pytest.raises(DomainValidationError, match="can only contain"):
            Username("john.doe@")
        
        with pytest.raises(DomainValidationError, match="can only contain"):
            Username("john doe")
    
    def test_username_length_validation(self):
        """Test username length validation."""
        with pytest.raises(DomainValidationError, match="must be between 2 and 30"):
            Username("a")
        
        with pytest.raises(DomainValidationError, match="must be between 2 and 30"):
            Username("a" * 31)
    
    def test_username_reserved(self):
        """Test reserved usernames."""
        with pytest.raises(DomainValidationError, match="is a reserved username"):
            Username("admin")
        
        with pytest.raises(DomainValidationError, match="is a reserved username"):
            Username("API")  # Should be normalized to 'api'
    
    def test_fullname_valid(self):
        """Test valid full names."""
        name = FullName("John Doe")
        assert str(name) == "John Doe"
        assert name.first_name == "John"
        assert name.last_name == "Doe"
    
    def test_fullname_normalization(self):
        """Test full name normalization."""
        name = FullName("  John    Doe  Smith  ")
        assert str(name) == "John Doe Smith"
        assert name.first_name == "John"
        assert name.last_name == "Smith"
    
    def test_fullname_single_name(self):
        """Test single name handling."""
        name = FullName("Madonna")
        assert name.first_name == "Madonna"
        assert name.last_name == ""


class TestContentValueObjects:
    """Test content domain value objects."""
    
    def test_slug_valid(self):
        """Test valid slugs."""
        slug = Slug("hello-world")
        assert str(slug) == "hello-world"
    
    def test_slug_from_title(self):
        """Test slug generation from title."""
        slug = Slug.from_title("Hello World! This is a Test")
        assert str(slug) == "hello-world-this-is-a-test"
    
    def test_slug_from_title_with_special_chars(self):
        """Test slug generation with special characters."""
        slug = Slug.from_title("C++ Programming & Web Development")
        assert str(slug) == "c-programming-web-development"
    
    def test_slug_invalid_format(self):
        """Test invalid slug formats."""
        with pytest.raises(DomainValidationError, match="can only contain"):
            Slug("Hello_World")
        
        with pytest.raises(DomainValidationError, match="can only contain"):
            Slug("-hello-world")  # Cannot start with hyphen
    
    def test_title_valid(self):
        """Test valid titles."""
        title = Title("My Blog Post")
        assert str(title) == "My Blog Post"
    
    def test_title_normalization(self):
        """Test title normalization."""
        title = Title("  My   Blog    Post  ")
        assert str(title) == "My Blog Post"
    
    def test_title_length_validation(self):
        """Test title length validation."""
        with pytest.raises(DomainValidationError, match="must be between 1 and 200"):
            Title("a" * 201)
    
    def test_content_valid(self):
        """Test valid content."""
        content = Content("This is my blog post content.")
        assert content.word_count == 6
        assert content.character_count == 29
    
    def test_content_excerpt(self):
        """Test content excerpt generation."""
        long_content = Content("This is a very long piece of content that should be truncated when generating an excerpt for display purposes in lists and previews.")
        excerpt = long_content.excerpt(50)
        assert len(excerpt) <= 53  # 50 + "..."
        assert excerpt.endswith("...")
    
    def test_post_status_valid(self):
        """Test valid post statuses."""
        status = PostStatus("published")
        assert status.is_published()
        assert not status.is_draft()
        
        draft = PostStatus("DRAFT")  # Should normalize
        assert draft.is_draft()
        assert str(draft) == "draft"
    
    def test_post_status_invalid(self):
        """Test invalid post status."""
        with pytest.raises(DomainValidationError, match="Invalid post status"):
            PostStatus("invalid-status")


class TestTaxonomyValueObjects:
    """Test taxonomy domain value objects."""
    
    def test_tag_name_valid(self):
        """Test valid tag names."""
        tag = TagName("Web Development")
        assert str(tag) == "Web Development"
        assert tag.slug == "web-development"
    
    def test_tag_name_with_special_chars(self):
        """Test tag names with allowed special characters."""
        tag = TagName("Web-Development_v2.0")  # Only use allowed characters
        assert tag.slug == "web-development-v2-0"
    
    def test_tag_name_reserved(self):
        """Test reserved tag names."""
        with pytest.raises(DomainValidationError, match="is a reserved tag name"):
            TagName("admin")
    
    def test_tag_description_optional(self):
        """Test tag description is optional."""
        desc = TagDescription("")
        assert desc.is_empty
        
        desc2 = TagDescription("A useful tag for web development")
        assert not desc2.is_empty
    
    def test_tag_color_predefined(self):
        """Test predefined color names."""
        color = TagColor("blue")
        assert str(color) == "#007bff"  # TagColor keeps predefined colors as lowercase
        assert not color.is_light  # Blue is dark
    
    def test_tag_color_hex(self):
        """Test hex color values."""
        color = TagColor("#ff0000")
        assert str(color) == "#FF0000"
        assert color.rgb_tuple == (255, 0, 0)
    
    def test_tag_color_light_detection(self):
        """Test light color detection."""
        light_color = TagColor("#ffffff")
        dark_color = TagColor("#000000")
        
        assert light_color.is_light
        assert not dark_color.is_light
    
    def test_tag_color_invalid(self):
        """Test invalid color formats."""
        with pytest.raises(DomainValidationError, match="Invalid color format"):
            TagColor("invalid-color")
        
        with pytest.raises(DomainValidationError, match="Invalid color format"):
            TagColor("#gggggg")


class TestManagementValueObjects:
    """Test management domain value objects."""
    
    def test_organization_name_valid(self):
        """Test valid organization names."""
        name = OrganizationName("TechBlog Corp")
        assert str(name) == "TechBlog Corp"
    
    def test_organization_name_normalization(self):
        """Test organization name normalization."""
        name = OrganizationName("  TechBlog   Corp  ")
        assert str(name) == "TechBlog Corp"
    
    def test_organization_identifier_valid(self):
        """Test valid organization identifiers."""
        identifier = OrganizationIdentifier("techblog")
        assert str(identifier) == "techblog"
    
    def test_organization_identifier_normalization(self):
        """Test organization identifier normalization."""
        identifier = OrganizationIdentifier("  TechBlog-Corp  ")
        assert str(identifier) == "techblog-corp"
    
    def test_organization_identifier_reserved(self):
        """Test reserved organization identifiers."""
        with pytest.raises(DomainValidationError, match="is a reserved identifier"):
            OrganizationIdentifier("admin")
    
    def test_contact_email_valid(self):
        """Test valid contact emails."""
        email = ContactEmail("admin@techblog.com")
        assert str(email) == "admin@techblog.com"
        assert email.domain == "techblog.com"