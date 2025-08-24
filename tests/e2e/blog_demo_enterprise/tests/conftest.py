"""
Enterprise Blog Demo Test Configuration
Pytest configuration and fixtures for multi-tenant blog platform testing.
"""
import pytest
import asyncio
import uuid
from typing import Any, Dict, Tuple, List, Optional


class MockEnterpriseGraphQLClient:
    """Mock GraphQL client that simulates multi-tenant organization creation."""
    
    def __init__(self):
        self.organizations = {}
        self.current_tenant_id = None
    
    async def execute(
        self, 
        query: str, 
        variables: Optional[Dict[str, Any]] = None, 
        context: Optional[Dict[str, Any]] = None
    ) -> Dict[str, Any]:
        """Execute a GraphQL query with basic organization creation support."""
        
        # Set tenant context if provided
        if context and "tenant_id" in context:
            self.current_tenant_id = context["tenant_id"]
        
        # Handle create organization mutation
        if "createOrganization" in query and variables:
            return await self._handle_create_organization(variables.get("input", {}))
        
        # Handle create post mutation
        if "createPost" in query and variables:
            return await self._handle_create_post(variables.get("input", {}), context)
        
        # Handle posts query with tenant isolation
        if "posts" in query and "mutation" not in query.lower():
            return await self._handle_posts_query()
        
        # Default fallback
        raise NotImplementedError(f"GraphQL operation not implemented in mock: {query[:50]}...")
    
    async def _handle_create_organization(self, input_data: Dict[str, Any]) -> Dict[str, Any]:
        """Mock implementation of organization creation."""
        
        # Validate required fields
        if not input_data.get("name"):
            return {
                "data": {
                    "createOrganization": {
                        "__typename": "CreateOrganizationError",
                        "message": "Organization name is required",
                        "errorCode": "MISSING_NAME"
                    }
                }
            }
        
        if not input_data.get("identifier"):
            return {
                "data": {
                    "createOrganization": {
                        "__typename": "CreateOrganizationError",
                        "message": "Organization identifier is required",
                        "errorCode": "MISSING_IDENTIFIER"
                    }
                }
            }
        
        # Check for duplicate identifier
        identifier = input_data["identifier"]
        if identifier in self.organizations:
            return {
                "data": {
                    "createOrganization": {
                        "__typename": "CreateOrganizationError",
                        "message": "An organization with this identifier already exists",
                        "errorCode": "DUPLICATE_IDENTIFIER"
                    }
                }
            }
        
        # Create organization
        org_id = str(uuid.uuid4())
        organization = {
            "id": org_id,  # GraphQL uses 'id', database uses 'pk_organization'
            "name": input_data["name"],
            "identifier": input_data["identifier"],
            "subscriptionPlan": input_data.get("subscriptionPlan", "starter"),
            "status": "active",
            "createdAt": "2024-01-01T00:00:00Z"  # Mock timestamp
        }
        
        # Store organization
        self.organizations[identifier] = organization
        
        return {
            "data": {
                "createOrganization": {
                    "__typename": "CreateOrganizationSuccess",
                    "organization": organization,
                    "message": "Organization created successfully"
                }
            }
        }
    
    async def _handle_create_post(self, input_data: Dict[str, Any], context: Optional[Dict[str, Any]]) -> Dict[str, Any]:
        """Mock implementation of post creation with tenant isolation."""
        
        # Validate required fields
        if not input_data.get("title"):
            return {
                "data": {
                    "createPost": {
                        "__typename": "CreatePostError",
                        "message": "Post title is required",
                        "errorCode": "MISSING_TITLE"
                    }
                }
            }
        
        if not input_data.get("content"):
            return {
                "data": {
                    "createPost": {
                        "__typename": "CreatePostError", 
                        "message": "Post content is required",
                        "errorCode": "MISSING_CONTENT"
                    }
                }
            }
        
        # Validate tenant context
        if not context or not context.get("tenant_id"):
            return {
                "data": {
                    "createPost": {
                        "__typename": "CreatePostError",
                        "message": "Tenant context is required",
                        "errorCode": "MISSING_TENANT_CONTEXT"
                    }
                }
            }
        
        # Generate post data
        post_id = str(uuid.uuid4())
        slug = input_data["title"].lower().replace(" ", "-").replace("[^a-z0-9-]", "")
        tenant_id = context["tenant_id"]
        user_id = context.get("user_id", str(uuid.uuid4()))
        
        post = {
            "id": post_id,
            "title": input_data["title"],
            "slug": slug,
            "content": input_data["content"],
            "status": input_data.get("status", "draft"),
            "organizationId": tenant_id,
            "author": {
                "id": user_id,
                "name": "Test Author",
                "organizationId": tenant_id
            },
            "createdAt": "2024-01-01T00:00:00Z"
        }
        
        # Store post for later queries (simple in-memory storage)
        if not hasattr(self, 'posts'):
            self.posts = {}
        if tenant_id not in self.posts:
            self.posts[tenant_id] = []
        self.posts[tenant_id].append(post)
        
        return {
            "data": {
                "createPost": {
                    "__typename": "CreatePostSuccess",
                    "post": post,
                    "message": "Post created successfully"
                }
            }
        }
    
    async def _handle_posts_query(self) -> Dict[str, Any]:
        """Mock implementation of tenant-isolated posts query."""
        
        # Return stored posts for current tenant, or fallback to mock data
        if hasattr(self, 'posts') and self.current_tenant_id in self.posts:
            posts = self.posts[self.current_tenant_id]
        elif self.current_tenant_id == "org1_id":
            # Fallback mock data for org1
            posts = [
                {
                    "id": str(uuid.uuid4()),
                    "title": "Org1 Post 1", 
                    "author": {"name": "Org1 Author", "organizationId": "org1_id"},
                    "organizationId": "org1_id"
                },
                {
                    "id": str(uuid.uuid4()),
                    "title": "Org1 Post 2",
                    "author": {"name": "Org1 Author", "organizationId": "org1_id"},
                    "organizationId": "org1_id"
                }
            ]
        elif self.current_tenant_id == "org2_id":
            # Fallback mock data for org2
            posts = [
                {
                    "id": str(uuid.uuid4()),
                    "title": "Org2 Post 1",
                    "author": {"name": "Org2 Author", "organizationId": "org2_id"},
                    "organizationId": "org2_id"
                }
            ]
        else:
            posts = []
        
        return {
            "data": {
                "posts": posts
            }
        }


@pytest.fixture
def enterprise_gql_client():
    """
    Provides a GraphQL client for enterprise blog demo testing.
    
    Currently returns a mock client that simulates:
    - Organization creation with validation
    - Tenant-isolated queries
    - Multi-tenant data separation
    
    Will be replaced with real FraiseQL client as we build the system.
    """
    return MockEnterpriseGraphQLClient()


@pytest.fixture
def seeded_multi_tenant_data(enterprise_gql_client) -> Tuple[str, str, List, List]:
    """
    Creates test data for two separate organizations.
    
    Returns:
        Tuple containing:
        - org1_id: First organization ID
        - org2_id: Second organization ID  
        - users: List of test users (empty for now)
        - posts: List of test posts (empty for now)
    """
    # This will eventually create real test data
    # For now, return mock IDs that the client can recognize
    org1_id = "org1_id"
    org2_id = "org2_id" 
    
    return org1_id, org2_id, [], []


# Pytest configuration
def pytest_configure(config):
    """Configure pytest for async testing."""
    # Add custom markers
    config.addinivalue_line("markers", "integration: marks tests as integration tests")
    config.addinivalue_line("markers", "multi_tenant: marks tests as multi-tenant specific")
    config.addinivalue_line("markers", "auth: marks tests requiring authentication")


# Event loop configuration removed - using pytest-asyncio defaults