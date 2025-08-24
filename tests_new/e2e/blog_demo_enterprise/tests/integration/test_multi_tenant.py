"""
Multi-tenant integration tests for blog_demo_enterprise.

Tests the core multi-tenancy features including:
- Organization isolation
- Tenant-aware queries
- Cross-tenant data security
"""
import pytest
import uuid
from typing import Dict, Any


class TestMultiTenantOrganizations:
    """Test organization management and tenant isolation."""
    
    @pytest.mark.asyncio
    async def test_create_organization(self, enterprise_gql_client):
        """Test creating a blog hosting organization."""
        mutation = """
            mutation CreateOrganization($input: CreateOrganizationInput!) {
                createOrganization(input: $input) {
                    __typename
                    ... on CreateOrganizationSuccess {
                        organization {
                            id
                            name
                            identifier
                            subscriptionPlan
                            status
                            createdAt
                        }
                        message
                    }
                    ... on CreateOrganizationError {
                        message
                        errorCode
                    }
                }
            }
        """
        
        input_data = {
            "name": "TechBlog Corp",
            "identifier": "techblog",
            "subscriptionPlan": "professional",
            "contactEmail": "admin@techblog.com"
        }
        
        result = await enterprise_gql_client.execute(
            mutation,
            variables={"input": input_data}
        )
        
        assert "errors" not in result
        assert result["data"]["createOrganization"]["__typename"] == "CreateOrganizationSuccess"
        
        org = result["data"]["createOrganization"]["organization"]
        assert org["name"] == "TechBlog Corp"
        assert org["identifier"] == "techblog"
        assert org["subscriptionPlan"] == "professional"
        assert org["status"] == "active"
        assert org["id"] is not None


class TestTenantIsolation:
    """Test that data is properly isolated between tenants."""
    
    @pytest.mark.asyncio
    async def test_tenant_isolated_posts(self, enterprise_gql_client, seeded_multi_tenant_data):
        """Test that posts are isolated by tenant."""
        org1_id, org2_id, users, posts = seeded_multi_tenant_data
        
        # Query posts for org1 - should only see org1 posts
        query = """
            query GetPosts {
                posts {
                    id
                    title
                    author {
                        name
                    }
                }
            }
        """
        
        # Set tenant context to org1
        result = await enterprise_gql_client.execute(
            query,
            context={"tenant_id": org1_id}
        )
        
        assert "errors" not in result
        org1_posts = result["data"]["posts"]
        
        # Should only see posts from org1
        assert len(org1_posts) == 2  # Based on seed data
        for post in org1_posts:
            assert "Org1" in post["title"]  # Verify it's org1 data
        
        # Set tenant context to org2
        result = await enterprise_gql_client.execute(
            query,
            context={"tenant_id": org2_id}
        )
        
        assert "errors" not in result
        org2_posts = result["data"]["posts"]
        
        # Should only see posts from org2
        assert len(org2_posts) == 1  # Based on seed data
        for post in org2_posts:
            assert "Org2" in post["title"]  # Verify it's org2 data


# Fixtures moved to conftest.py