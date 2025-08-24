"""
Multi-tenant content integration tests for blog_demo_enterprise.

Tests content (posts, comments, tags) with tenant isolation:
- Posts are isolated by organization
- Comments are tenant-aware
- Tags are scoped to organizations
"""

import uuid

import pytest


class TestMultiTenantPosts:
    """Test multi-tenant post creation and isolation."""

    @pytest.mark.asyncio
    async def test_create_post_with_tenant_isolation(self, enterprise_gql_client):
        """Test creating a post within a tenant context."""
        # First create an organization
        org_mutation = """
            mutation CreateOrganization($input: CreateOrganizationInput!) {
                createOrganization(input: $input) {
                    __typename
                    ... on CreateOrganizationSuccess {
                        organization {
                            id
                            name
                            identifier
                        }
                    }
                }
            }
        """

        org_result = await enterprise_gql_client.execute(
            org_mutation,
            variables={
                "input": {
                    "name": "Content Blog Corp",
                    "identifier": "contentblog",
                    "contactEmail": "admin@contentblog.com",
                }
            },
        )

        org_id = org_result["data"]["createOrganization"]["organization"]["id"]

        # Now create a post within this organization's context
        post_mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    __typename
                    ... on CreatePostSuccess {
                        post {
                            id
                            title
                            slug
                            status
                            organizationId
                            author {
                                name
                                organizationId
                            }
                            createdAt
                        }
                        message
                    }
                    ... on CreatePostError {
                        message
                        errorCode
                    }
                }
            }
        """

        post_result = await enterprise_gql_client.execute(
            post_mutation,
            variables={
                "input": {
                    "title": "Multi-Tenant Blog Post",
                    "content": "This post belongs to the ContentBlog organization",
                    "status": "published",
                }
            },
            context={"tenant_id": org_id, "user_id": str(uuid.uuid4())},
        )

        # Should successfully create post with tenant isolation
        assert "errors" not in post_result
        assert post_result["data"]["createPost"]["__typename"] == "CreatePostSuccess"

        post = post_result["data"]["createPost"]["post"]
        assert post["title"] == "Multi-Tenant Blog Post"
        assert post["organizationId"] == org_id
        assert post["author"]["organizationId"] == org_id
        assert post["status"] == "published"


class TestTenantContentIsolation:
    """Test that content is properly isolated between tenants."""

    @pytest.mark.asyncio
    async def test_posts_isolated_by_tenant(self, enterprise_gql_client, multi_tenant_content_data):
        """Test that posts from different tenants are isolated."""
        org1_id, org2_id, posts_data = multi_tenant_content_data

        # Query posts for tenant 1
        query = """
            query GetPosts {
                posts {
                    id
                    title
                    organizationId
                    author {
                        name
                        organizationId
                    }
                }
            }
        """

        # Query as org1 - should only see org1 posts
        result1 = await enterprise_gql_client.execute(query, context={"tenant_id": org1_id})

        org1_posts = result1["data"]["posts"]
        assert len(org1_posts) == 2  # Based on test data
        for post in org1_posts:
            assert post["organizationId"] == org1_id
            assert post["author"]["organizationId"] == org1_id

        # Query as org2 - should only see org2 posts
        result2 = await enterprise_gql_client.execute(query, context={"tenant_id": org2_id})

        org2_posts = result2["data"]["posts"]
        assert len(org2_posts) == 1  # Based on test data
        for post in org2_posts:
            assert post["organizationId"] == org2_id
            assert post["author"]["organizationId"] == org2_id

    @pytest.mark.asyncio
    async def test_comments_isolated_by_tenant(
        self, enterprise_gql_client, multi_tenant_content_data
    ):
        """Test that comments are tenant-isolated."""
        org1_id, org2_id, posts_data = multi_tenant_content_data

        # Create a comment on an org1 post
        comment_mutation = """
            mutation CreateComment($input: CreateCommentInput!) {
                createComment(input: $input) {
                    __typename
                    ... on CreateCommentSuccess {
                        comment {
                            id
                            content
                            organizationId
                            post {
                                title
                                organizationId
                            }
                        }
                    }
                }
            }
        """

        # Try to comment on an org1 post while in org2 context - should fail
        result = await enterprise_gql_client.execute(
            comment_mutation,
            variables={
                "input": {
                    "postId": posts_data["org1_post_id"],
                    "content": "This comment should fail due to tenant isolation",
                }
            },
            context={"tenant_id": org2_id, "user_id": str(uuid.uuid4())},
        )

        # Should fail due to cross-tenant access attempt
        assert result["data"]["createComment"]["__typename"] == "CreateCommentError"
        assert "tenant" in result["data"]["createComment"]["message"].lower()


@pytest.fixture
def multi_tenant_content_data(enterprise_gql_client):
    """Creates test content data for multiple tenants."""
    # This will eventually create real content data
    # For now, return mock data structure
    org1_id = "org1_id"  # Use consistent IDs for testing
    org2_id = "org2_id"

    posts_data = {"org1_post_id": str(uuid.uuid4()), "org2_post_id": str(uuid.uuid4())}

    return org1_id, org2_id, posts_data
