"""
Tests for Post domain entity.

Testing the business logic and behavior of the Post aggregate root.
"""
import pytest
from uuid import uuid4

from blog.domain.content.post import Post, PostId
from blog.domain.content.value_objects import Title, Slug, Content, PostStatus
from blog.domain.common.exceptions import DomainValidationError, BusinessRuleViolationError


class TestPost:
    """Test Post domain entity."""
    
    def test_create_post_with_valid_data(self):
        """Test creating a post with valid data."""
        post_id = PostId(uuid4())
        title = Title("My First Blog Post")
        slug = Slug("my-first-blog-post")
        content = Content("This is the content of my first blog post.")
        author_id = uuid4()
        org_id = uuid4()
        
        post = Post(
            id=post_id,
            title=title,
            slug=slug,
            content=content,
            author_id=author_id,
            organization_id=org_id
        )
        
        assert post.title == title
        assert post.slug == slug
        assert post.content == content
        assert post.status.is_draft()
        assert not post.is_published()
        assert post.reading_time_minutes == 1  # Short content
        assert post.excerpt is not None  # Auto-generated
    
    def test_post_publishing_workflow(self):
        """Test post publishing workflow."""
        post = self._create_test_post()
        
        # Initially draft
        assert post.is_draft()
        assert post.can_be_published()
        
        # Publish post
        published_at = "2025-01-01T12:00:00Z"
        post.publish(published_at)
        
        assert post.is_published()
        assert post.published_at == published_at
        
        # Cannot publish already published post
        with pytest.raises(BusinessRuleViolationError, match="already published"):
            post.publish("2025-01-02T12:00:00Z")
    
    def test_post_unpublishing(self):
        """Test post unpublishing."""
        post = self._create_test_post()
        
        # Cannot unpublish draft
        with pytest.raises(BusinessRuleViolationError, match="not published"):
            post.unpublish()
        
        # Publish then unpublish
        post.publish("2025-01-01T12:00:00Z")
        post.unpublish()
        
        assert post.is_draft()
        assert post.published_at is None
    
    def test_post_archiving(self):
        """Test post archiving."""
        post = self._create_test_post()
        
        post.archive()
        assert post.status.is_archived()
        
        # Cannot archive deleted post
        post.soft_delete()
        with pytest.raises(BusinessRuleViolationError, match="Cannot archive deleted post"):
            post.archive()
    
    def test_post_content_update(self):
        """Test post content updates."""
        post = self._create_test_post()
        
        new_title = Title("Updated Blog Post Title")
        # Create much longer content to ensure reading time changes
        long_text = " ".join(["word"] * 300)  # 300 words should be more than 1 minute
        new_content = Content(f"This is updated content. {long_text}")
        
        old_reading_time = post.reading_time_minutes
        post.update_content(title=new_title, content=new_content)
        
        assert post.title == new_title
        assert post.content == new_content
        # Reading time should be recalculated and increased
        assert post.reading_time_minutes > old_reading_time
    
    def test_post_slug_auto_generation(self):
        """Test automatic slug generation when title changes."""
        post = self._create_test_post()
        
        new_title = Title("Completely Different Title")
        old_slug = post.slug
        
        post.update_content(title=new_title)
        
        # Slug should change when title changes significantly
        assert post.slug != old_slug
        assert str(post.slug) == "completely-different-title"
    
    def test_post_excerpt_generation(self):
        """Test automatic excerpt generation."""
        long_content = Content("This is a very long piece of content. " * 20)
        
        post = Post(
            id=PostId(uuid4()),
            title=Title("Test Post"),
            slug=Slug("test-post"),
            content=long_content,
            author_id=uuid4(),
            organization_id=uuid4()
        )
        
        # Excerpt should be generated and truncated
        assert post.excerpt is not None
        assert len(post.excerpt) <= 153  # 150 + "..."
    
    def test_post_meta_keywords_validation(self):
        """Test meta keywords validation."""
        post = self._create_test_post()
        
        # Valid keywords
        keywords = {"python", "web development", "tutorial"}
        post.update_meta_keywords(keywords)
        assert post.meta_keywords == {"python", "web development", "tutorial"}
        
        # Too many keywords
        too_many = {f"keyword{i}" for i in range(15)}
        with pytest.raises(DomainValidationError, match="Cannot have more than 10"):
            post.update_meta_keywords(too_many)
        
        # Empty keyword
        with pytest.raises(DomainValidationError, match="cannot be empty"):
            post.update_meta_keywords({"valid", ""})
        
        # Too long keyword
        with pytest.raises(DomainValidationError, match="cannot exceed 50 characters"):
            post.update_meta_keywords({"a" * 51})
    
    def test_post_featured_image_validation(self):
        """Test featured image URL validation."""
        post = self._create_test_post()
        
        # Valid URL
        post.update_featured_image("https://example.com/image.jpg")
        assert post.featured_image_url == "https://example.com/image.jpg"
        
        # Invalid URL format
        with pytest.raises(DomainValidationError, match="must be a valid HTTP/HTTPS URL"):
            post.update_featured_image("invalid-url")
        
        # Too long URL
        with pytest.raises(DomainValidationError, match="cannot exceed 500 characters"):
            post.update_featured_image("https://" + "x" * 500 + ".com")
    
    def test_post_meta_description_validation(self):
        """Test meta description validation."""
        post = self._create_test_post()
        
        # Valid meta description
        post.update_content(meta_description="A short description")
        assert post.meta_description == "A short description"
        
        # Too long meta description
        with pytest.raises(DomainValidationError, match="cannot exceed 160 characters"):
            post.update_content(meta_description="x" * 161)
    
    def test_post_excerpt_validation(self):
        """Test custom excerpt validation."""
        post = self._create_test_post()
        
        # Valid excerpt
        post.update_content(excerpt="Custom excerpt")
        assert post.excerpt == "Custom excerpt"
        
        # Too long excerpt
        with pytest.raises(DomainValidationError, match="cannot exceed 300 characters"):
            post.update_content(excerpt="x" * 301)
    
    def test_post_deletion_prevents_updates(self):
        """Test that deleted posts cannot be updated."""
        post = self._create_test_post()
        
        post.soft_delete()
        
        with pytest.raises(BusinessRuleViolationError, match="Cannot update deleted post"):
            post.update_content(title=Title("New Title"))
    
    def test_reading_time_calculation(self):
        """Test reading time calculation."""
        # Short content (< 200 words) should be 1 minute
        short_content = Content("Short content")
        post = Post(
            id=PostId(uuid4()),
            title=Title("Test"),
            slug=Slug("test"),
            content=short_content,
            author_id=uuid4(),
            organization_id=uuid4()
        )
        assert post.reading_time_minutes == 1
        
        # Longer content should calculate correctly
        long_content = Content(" ".join(["word"] * 400))  # 400 words
        post.update_content(content=long_content)
        assert post.reading_time_minutes == 2  # 400/200 = 2
    
    def _create_test_post(self) -> Post:
        """Create a test post with default values."""
        return Post(
            id=PostId(uuid4()),
            title=Title("Test Blog Post"),
            slug=Slug("test-blog-post"),
            content=Content("This is test content for the blog post."),
            author_id=uuid4(),
            organization_id=uuid4()
        )