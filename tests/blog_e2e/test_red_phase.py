"""RED Phase - Failing E2E Tests for Blog Application

This test suite implements the RED phase of micro TDD, creating comprehensive
failing tests that define the expected behavior of the blog application's
error handling system.

Focus: Testing FraiseQL mutation error patterns and database-first architecture.
"""

import pytest
import pytest_asyncio


class TestBlogPostCreationErrors:
    """Test comprehensive error scenarios for blog post creation.
    
    These tests demonstrate the FraiseQL error handling system with
    database-first validation and structured error responses.
    """
    
    @pytest_asyncio.fixture(autouse=True)
    async def setup(self, clean_database):
        """Ensure clean state for each test."""
        pass
    
    async def test_create_post_success_case(self, graphql_client):
        """Test successful post creation (will fail until GREEN phase)."""
        # First create an author
        author_result = await graphql_client.create_author(
            identifier="success-author",
            name="Success Author",
            email="success@example.com"
        )
        
        # Verify author creation succeeds (will fail without implementation)
        assert author_result["data"]["createAuthor"]["__typename"] == "CreateAuthorSuccess"
        
        # Then create a post
        post_result = await graphql_client.create_post(
            identifier="success-post",
            title="Successful Post",
            content="This post should be created successfully",
            authorIdentifier="success-author",
            status="draft"
        )
        
        # Verify post creation
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostSuccess"
        post = post_result["data"]["createPost"]["post"]
        assert post["identifier"] == "success-post"
        assert post["title"] == "Successful Post"
        assert post["status"] == "draft"
        assert post["authorId"] is not None
    
    async def test_create_post_missing_author_error(self, graphql_client):
        """Test error when creating post with non-existent author."""
        post_result = await graphql_client.create_post(
            identifier="orphaned-post",
            title="Post Without Author",
            content="This post references a missing author",
            authorIdentifier="non-existent-author",
            status="draft"
        )
        
        # Should return CreatePostError with specific error details
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = post_result["data"]["createPost"]
        
        # Verify error structure following PrintOptim patterns
        assert error["errorCode"] == "MISSING_AUTHOR"
        assert "non-existent-author" in error["message"]
        assert error["missingAuthor"]["identifier"] == "non-existent-author"
        assert error["originalPayload"] is not None
        assert error["originalPayload"]["authorIdentifier"] == "non-existent-author"
    
    async def test_create_post_duplicate_identifier_error(self, graphql_client):
        """Test error when creating post with duplicate identifier."""
        # First create an author
        await graphql_client.create_author(
            identifier="dup-test-author",
            name="Duplicate Test Author",
            email="dup@example.com"
        )
        
        # Create first post successfully
        first_result = await graphql_client.create_post(
            identifier="duplicate-post",
            title="First Post",
            content="This is the first post",
            authorIdentifier="dup-test-author"
        )
        assert first_result["data"]["createPost"]["__typename"] == "CreatePostSuccess"
        
        # Try to create second post with same identifier
        duplicate_result = await graphql_client.create_post(
            identifier="duplicate-post",  # Same identifier
            title="Duplicate Post",
            content="This should fail due to duplicate identifier",
            authorIdentifier="dup-test-author"
        )
        
        # Should return CreatePostError with conflict information
        assert duplicate_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = duplicate_result["data"]["createPost"]
        
        assert error["errorCode"] == "DUPLICATE_IDENTIFIER"
        assert "duplicate-post" in error["message"]
        assert error["conflictPost"]["identifier"] == "duplicate-post"
        assert error["conflictPost"]["title"] == "First Post"  # Original post info
    
    async def test_create_post_invalid_status_error(self, graphql_client):
        """Test error when creating post with invalid status."""
        # Create author first
        await graphql_client.create_author(
            identifier="status-test-author",
            name="Status Test Author",
            email="status@example.com"
        )
        
        # Try to create post with invalid status
        post_result = await graphql_client.create_post(
            identifier="invalid-status-post",
            title="Post with Invalid Status",
            content="This post has an invalid status",
            authorIdentifier="status-test-author",
            status="invalid-status"  # Invalid status
        )
        
        # Should return validation error
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = post_result["data"]["createPost"]
        
        assert error["errorCode"] == "INVALID_STATUS"
        assert "invalid-status" in error["message"]
        assert "draft, published, archived" in error["message"]
    
    async def test_create_post_missing_required_fields_error(self, graphql_client):
        """Test error when creating post with missing required fields."""
        post_result = await graphql_client.create_post(
            # Missing identifier
            title="Post Without Identifier",
            content="This post is missing required fields",
            authorIdentifier="some-author"
        )
        
        # Should return validation error
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = post_result["data"]["createPost"]
        
        assert error["errorCode"] == "MISSING_REQUIRED_FIELDS"
        assert "identifier" in error["message"]
    
    async def test_create_post_invalid_tags_error(self, graphql_client):
        """Test error when creating post with non-existent tags."""
        # Create author first
        await graphql_client.create_author(
            identifier="tag-test-author",
            name="Tag Test Author",
            email="tags@example.com"
        )
        
        # Try to create post with invalid tags
        post_result = await graphql_client.create_post(
            identifier="tagged-post",
            title="Post with Invalid Tags",
            content="This post references non-existent tags",
            authorIdentifier="tag-test-author",
            tagIdentifiers=["non-existent-tag", "another-missing-tag"]
        )
        
        # Should return error with tag validation details
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = post_result["data"]["createPost"]
        
        assert error["errorCode"] == "INVALID_TAGS"
        assert error["invalidTags"] == ["non-existent-tag", "another-missing-tag"]
        assert "non-existent-tag" in error["message"]
    
    async def test_create_post_content_too_long_error(self, graphql_client):
        """Test error when post content exceeds maximum length."""
        # Create author first
        await graphql_client.create_author(
            identifier="long-content-author",
            name="Long Content Author",
            email="long@example.com"
        )
        
        # Create post with content that's too long (assume 10000 char limit)
        long_content = "x" * 10001
        
        post_result = await graphql_client.create_post(
            identifier="long-post",
            title="Post with Too Long Content",
            content=long_content,
            authorIdentifier="long-content-author"
        )
        
        # Should return validation error
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError" 
        error = post_result["data"]["createPost"]
        
        assert error["errorCode"] == "CONTENT_TOO_LONG"
        assert "10000" in error["message"]
        assert len(error["originalPayload"]["content"]) > 10000
    
    async def test_create_post_publish_date_validation_error(self, graphql_client):
        """Test error when publish date is in the past for published posts.""" 
        # Create author first
        await graphql_client.create_author(
            identifier="publish-test-author",
            name="Publish Test Author",
            email="publish@example.com"
        )
        
        # Try to create published post with past publish date
        post_result = await graphql_client.create_post(
            identifier="past-publish-post",
            title="Post with Past Publish Date",
            content="This post has invalid publish timing",
            authorIdentifier="publish-test-author",
            status="published",
            publishAt="2020-01-01T00:00:00Z"  # Past date
        )
        
        # Should return validation error
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = post_result["data"]["createPost"]
        
        assert error["errorCode"] == "INVALID_PUBLISH_DATE"
        assert "past" in error["message"].lower()


class TestAuthorCreationErrors:
    """Test error scenarios for author creation."""
    
    @pytest_asyncio.fixture(autouse=True)
    async def setup(self, clean_database):
        """Ensure clean state for each test."""
        pass
    
    async def test_create_author_success_case(self, graphql_client):
        """Test successful author creation (will fail until GREEN phase)."""
        result = await graphql_client.create_author(
            identifier="test-author",
            name="Test Author", 
            email="test@example.com",
            bio="A test author for E2E testing"
        )
        
        # Verify successful creation
        assert result["data"]["createAuthor"]["__typename"] == "CreateAuthorSuccess"
        author = result["data"]["createAuthor"]["author"]
        assert author["identifier"] == "test-author"
        assert author["name"] == "Test Author"
        assert author["email"] == "test@example.com"
        assert result["data"]["createAuthor"]["message"] == "Author created successfully"
    
    async def test_create_author_duplicate_identifier_error(self, graphql_client):
        """Test error when creating author with duplicate identifier."""
        # Create first author
        first_result = await graphql_client.create_author(
            identifier="duplicate-author",
            name="First Author",
            email="first@example.com"
        )
        assert first_result["data"]["createAuthor"]["__typename"] == "CreateAuthorSuccess"
        
        # Try to create second author with same identifier
        duplicate_result = await graphql_client.create_author(
            identifier="duplicate-author",  # Same identifier
            name="Second Author",
            email="second@example.com" 
        )
        
        # Should return error with conflict information
        assert duplicate_result["data"]["createAuthor"]["__typename"] == "CreateAuthorError"
        error = duplicate_result["data"]["createAuthor"]
        
        assert error["errorCode"] == "DUPLICATE_IDENTIFIER"
        assert "duplicate-author" in error["message"]
        assert error["conflictAuthor"]["identifier"] == "duplicate-author"
        assert error["conflictAuthor"]["name"] == "First Author"  # Original author
    
    async def test_create_author_duplicate_email_error(self, graphql_client):
        """Test error when creating author with duplicate email."""
        # Create first author
        first_result = await graphql_client.create_author(
            identifier="first-author",
            name="First Author",
            email="duplicate@example.com"
        )
        assert first_result["data"]["createAuthor"]["__typename"] == "CreateAuthorSuccess"
        
        # Try to create second author with same email
        duplicate_result = await graphql_client.create_author(
            identifier="second-author",
            name="Second Author",
            email="duplicate@example.com"  # Same email
        )
        
        # Should return error with conflict information
        assert duplicate_result["data"]["createAuthor"]["__typename"] == "CreateAuthorError"
        error = duplicate_result["data"]["createAuthor"]
        
        assert error["errorCode"] == "DUPLICATE_EMAIL"
        assert "duplicate@example.com" in error["message"]
        assert error["conflictAuthor"]["email"] == "duplicate@example.com"
    
    async def test_create_author_invalid_email_error(self, graphql_client):
        """Test error when creating author with invalid email format."""
        result = await graphql_client.create_author(
            identifier="invalid-email-author",
            name="Invalid Email Author",
            email="not-an-email"  # Invalid email format
        )
        
        # Should return validation error
        assert result["data"]["createAuthor"]["__typename"] == "CreateAuthorError"
        error = result["data"]["createAuthor"]
        
        assert error["errorCode"] == "INVALID_EMAIL"
        assert "not-an-email" in error["message"]
        assert "format" in error["message"].lower()
    
    async def test_create_author_missing_required_fields_error(self, graphql_client):
        """Test error when creating author with missing required fields."""
        result = await graphql_client.create_author(
            # Missing identifier and name
            email="incomplete@example.com"
        )
        
        # Should return validation error
        assert result["data"]["createAuthor"]["__typename"] == "CreateAuthorError"
        error = result["data"]["createAuthor"]
        
        assert error["errorCode"] == "MISSING_REQUIRED_FIELDS"
        assert "identifier" in error["message"] or "name" in error["message"]


class TestTagCreationErrors:
    """Test error scenarios for tag creation."""
    
    @pytest_asyncio.fixture(autouse=True)
    async def setup(self, clean_database):
        """Ensure clean state for each test."""
        pass
    
    async def test_create_tag_circular_hierarchy_error(self, graphql_client):
        """Test error when creating circular tag hierarchy."""
        # This test will be implemented in GREEN phase
        # For now, just define the expected behavior
        
        # Create parent tag
        mutation = """
            mutation CreateTag($input: CreateTagInput!) {
                createTag(input: $input) {
                    __typename
                    ... on CreateTagSuccess {
                        tag { id identifier name }
                    }
                    ... on CreateTagError {
                        errorCode message
                    }
                }
            }
        """
        
        parent_result = await graphql_client.execute(mutation, {
            "input": {
                "identifier": "parent-tag",
                "name": "Parent Tag"
            }
        })
        
        assert parent_result["data"]["createTag"]["__typename"] == "CreateTagSuccess"
        
        # Create child tag
        child_result = await graphql_client.execute(mutation, {
            "input": {
                "identifier": "child-tag", 
                "name": "Child Tag",
                "parentIdentifier": "parent-tag"
            }
        })
        
        assert child_result["data"]["createTag"]["__typename"] == "CreateTagSuccess"
        
        # Try to make parent a child of its own child (circular reference)
        circular_result = await graphql_client.execute(mutation, {
            "input": {
                "identifier": "parent-tag",  # Update existing tag
                "name": "Parent Tag Updated",
                "parentIdentifier": "child-tag"  # Circular reference
            }
        })
        
        # Should return error
        assert circular_result["data"]["createTag"]["__typename"] == "CreateTagError"
        error = circular_result["data"]["createTag"]
        assert error["errorCode"] == "CIRCULAR_HIERARCHY"
        assert "circular" in error["message"].lower()


class TestCommentCreationErrors:
    """Test error scenarios for comment creation."""
    
    @pytest_asyncio.fixture(autouse=True) 
    async def setup(self, clean_database, sample_author):
        """Setup with sample author and post."""
        self.sample_author = sample_author
    
    async def test_create_comment_missing_post_error(self, graphql_client):
        """Test error when creating comment for non-existent post."""
        mutation = """
            mutation CreateComment($input: CreateCommentInput!) {
                createComment(input: $input) {
                    __typename
                    ... on CreateCommentSuccess {
                        comment { id content }
                    }
                    ... on CreateCommentError {
                        errorCode message missingPost { identifier }
                    }
                }
            }
        """
        
        result = await graphql_client.execute(mutation, {
            "input": {
                "postIdentifier": "non-existent-post",
                "content": "This comment is orphaned",
                "authorName": "Anonymous",
                "authorEmail": "anon@example.com"
            }
        })
        
        # Should return error
        assert result["data"]["createComment"]["__typename"] == "CreateCommentError"
        error = result["data"]["createComment"]
        assert error["errorCode"] == "MISSING_POST"
        assert error["missingPost"]["identifier"] == "non-existent-post"
    
    async def test_create_comment_spam_detection_error(self, graphql_client):
        """Test spam detection in comment creation."""
        # This test demonstrates content validation patterns
        mutation = """
            mutation CreateComment($input: CreateCommentInput!) {
                createComment(input: $input) {
                    __typename
                    ... on CreateCommentError {
                        errorCode message spamReasons
                    }
                }
            }
        """
        
        # Create comment with spam-like content
        spam_content = "BUY NOW!!! CHEAP VIAGRA!!! CLICK HERE!!! " * 10
        
        result = await graphql_client.execute(mutation, {
            "input": {
                "postIdentifier": "some-post",
                "content": spam_content,
                "authorName": "Spammer",
                "authorEmail": "spam@spam.com"
            }
        })
        
        # Should return spam detection error
        assert result["data"]["createComment"]["__typename"] == "CreateCommentError"
        error = result["data"]["createComment"]
        assert error["errorCode"] == "SPAM_DETECTED"
        assert error["spamReasons"] is not None
        assert len(error["spamReasons"]) > 0


class TestErrorMetadataStructure:
    """Test that error responses contain rich metadata following PrintOptim patterns."""
    
    async def test_error_contains_extra_metadata(self, graphql_client, clean_database):
        """Test that errors include comprehensive metadata for debugging."""
        # Try to create post without author (will trigger error)
        post_result = await graphql_client.create_post(
            identifier="metadata-test-post",
            title="Metadata Test Post", 
            content="Testing error metadata structure",
            authorIdentifier="non-existent-author"
        )
        
        # Verify error structure includes all expected metadata fields
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostError"
        error = post_result["data"]["createPost"]
        
        # Core error fields
        assert error["message"] is not None
        assert error["errorCode"] is not None
        
        # PrintOptim-style metadata
        assert error["originalPayload"] is not None
        assert error["originalPayload"]["identifier"] == "metadata-test-post"
        assert error["originalPayload"]["authorIdentifier"] == "non-existent-author"
        
        # Specific error context
        assert error["missingAuthor"] is not None
        assert error["missingAuthor"]["identifier"] == "non-existent-author"
    
    async def test_error_response_structure_consistency(self, graphql_client, clean_database):
        """Test that all error responses follow consistent structure."""
        # Test multiple error types to ensure consistent structure
        
        # 1. Missing author error
        missing_author_result = await graphql_client.create_post(
            identifier="test1",
            title="Test 1",
            content="Test content",
            authorIdentifier="missing-author"
        )
        
        # 2. Duplicate identifier error (create two posts)
        await graphql_client.create_author(identifier="dup-author", name="Dup Author", email="dup@example.com")
        await graphql_client.create_post(
            identifier="duplicate-test",
            title="First Post",
            content="First post content",
            authorIdentifier="dup-author"
        )
        
        duplicate_result = await graphql_client.create_post(
            identifier="duplicate-test",  # Same identifier
            title="Second Post", 
            content="Second post content",
            authorIdentifier="dup-author"
        )
        
        # Both should have CreatePostError typename
        assert missing_author_result["data"]["createPost"]["__typename"] == "CreatePostError"
        assert duplicate_result["data"]["createPost"]["__typename"] == "CreatePostError"
        
        # Both should have core error fields
        for result in [missing_author_result, duplicate_result]:
            error = result["data"]["createPost"]
            assert "message" in error
            assert "errorCode" in error
            assert "originalPayload" in error
            assert error["originalPayload"] is not None
            
        # Verify specific error codes are different but structure is same
        assert missing_author_result["data"]["createPost"]["errorCode"] == "MISSING_AUTHOR"
        assert duplicate_result["data"]["createPost"]["errorCode"] == "DUPLICATE_IDENTIFIER"