"""
Enterprise GraphQL Client Fixture
Provides a real GraphQL client for testing the enterprise blog demo.
"""
import pytest
from typing import Dict, Any, Optional
import asyncio
import uuid

# Mock implementation for now - will be replaced with real FraiseQL client
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
    
    async def _handle_posts_query(self) -> Dict[str, Any]:
        """Mock implementation of tenant-isolated posts query."""
        
        # Mock data for different tenants
        if self.current_tenant_id == "org1_id":
            posts = [
                {
                    "id": str(uuid.uuid4()),
                    "title": "Org1 Post 1", 
                    "author": {"name": "Org1 Author"}
                },
                {
                    "id": str(uuid.uuid4()),
                    "title": "Org1 Post 2",
                    "author": {"name": "Org1 Author"}
                }
            ]
        elif self.current_tenant_id == "org2_id":
            posts = [
                {
                    "id": str(uuid.uuid4()),
                    "title": "Org2 Post 1",
                    "author": {"name": "Org2 Author"}
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
def enterprise_graphql_client():
    """
    Provides a GraphQL client for enterprise blog demo testing.
    
    Currently returns a mock client that simulates:
    - Organization creation with validation
    - Tenant-isolated queries
    - Multi-tenant data separation
    
    Will be replaced with real FraiseQL client as we build the system.
    """
    return MockEnterpriseGraphQLClient()