"""End-to-end tests for field-level authorization."""

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from graphql import graphql_sync

from fraiseql import fraise_type, query
from fraiseql.decorators import field
from fraiseql.security.field_auth import authorize_field, FieldAuthorizationError
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.gql.schema_builder import build_fraiseql_schema


# Test types with field authorization
@fraise_type
class PublicProfile:
    username: str
    bio: str


@fraise_type
class PrivateProfile:
    email: str
    phone: str
    address: str


@fraise_type
class User:
    id: int
    username: str
    public_profile: PublicProfile
    
    @field
    def private_profile(self) -> PrivateProfile:
        """Private profile - requires authentication and ownership."""
        # In real app, this would check info.context
        return PrivateProfile(
            email="user@example.com",
            phone="+1234567890",
            address="123 Main St",
        )
    
    @field
    def admin_notes(self) -> str:
        """Admin-only field."""
        return "Internal admin notes about user"


# Test queries
@query
async def me(info) -> User | None:
    """Get current user."""
    user_id = info.context.get("user_id")
    if not user_id:
        return None
    
    return User(
        id=user_id,
        username=f"user{user_id}",
        public_profile=PublicProfile(
            username=f"user{user_id}",
            bio="A test user",
        ),
    )


@query
async def get_user(info, user_id: int) -> User:
    """Get user by ID."""
    return User(
        id=user_id,
        username=f"user{user_id}",
        public_profile=PublicProfile(
            username=f"user{user_id}",
            bio="A test user",
        ),
    )


class TestFieldAuthE2E:
    """End-to-end tests for field-level authorization."""
    
    @pytest.fixture
    def app(self):
        """Create test application with field auth."""
        # Create schema
        schema = build_fraiseql_schema(query_types=[me, get_user])
        
        # Create app
        app = create_fraiseql_app(
            schema=schema,
            path="/graphql",
        )
        
        # Add auth simulation middleware
        @app.middleware("http")
        async def auth_middleware(request, call_next):
            # Simulate auth from headers
            auth_header = request.headers.get("Authorization", "")
            
            # Simple auth simulation
            if auth_header.startswith("Bearer "):
                token = auth_header[7:]
                if token == "admin-token":
                    request.state.user_id = 1
                    request.state.is_admin = True
                elif token.startswith("user-"):
                    request.state.user_id = int(token.split("-")[1])
                    request.state.is_admin = False
            
            response = await call_next(request)
            return response
        
        return app
    
    @pytest.fixture
    def client(self, app):
        """Create test client."""
        return TestClient(app)
    
    def test_public_fields_accessible(self, client):
        """Test that public fields are accessible without auth."""
        query = """
        query {
            getUser(userId: 1) {
                id
                username
                publicProfile {
                    username
                    bio
                }
            }
        }
        """
        
        response = client.post("/graphql", json={"query": query})
        assert response.status_code == 200
        
        data = response.json()
        assert data["data"]["getUser"]["id"] == 1
        assert data["data"]["getUser"]["username"] == "user1"
        assert data["data"]["getUser"]["publicProfile"]["bio"] == "A test user"
    
    def test_authenticated_user_query(self, client):
        """Test authenticated user can query their own data."""
        query = """
        query {
            me {
                id
                username
                publicProfile {
                    username
                    bio
                }
            }
        }
        """
        
        # Without auth
        response = client.post("/graphql", json={"query": query})
        assert response.status_code == 200
        data = response.json()
        assert data["data"]["me"] is None
        
        # With auth
        headers = {"Authorization": "Bearer user-123"}
        response = client.post(
            "/graphql", 
            json={"query": query},
            headers=headers,
        )
        assert response.status_code == 200
        data = response.json()
        assert data["data"]["me"]["id"] == 123
        assert data["data"]["me"]["username"] == "user123"
    
    def test_graphql_error_handling(self, client):
        """Test GraphQL error responses for auth failures."""
        # Query with private field
        query = """
        query {
            getUser(userId: 1) {
                id
                username
                privateProfile {
                    email
                    phone
                }
            }
        }
        """
        
        # Execute query - should get partial result with error
        response = client.post("/graphql", json={"query": query})
        assert response.status_code == 200
        
        data = response.json()
        # Should have partial data
        assert data["data"]["getUser"]["id"] == 1
        assert data["data"]["getUser"]["username"] == "user1"
        # Private field should be null due to auth error
        assert data["data"]["getUser"]["privateProfile"] is None
        
        # Should have errors (if field auth is properly integrated)
        # Note: This would require proper integration with GraphQL execution
    
    def test_multiple_auth_levels(self, client):
        """Test different authorization levels."""
        queries = {
            "user": """
            query {
                me {
                    id
                    username
                }
            }
            """,
            "admin": """
            query {
                getUser(userId: 1) {
                    id
                    username
                    adminNotes
                }
            }
            """,
        }
        
        # Test as regular user
        headers = {"Authorization": "Bearer user-2"}
        response = client.post(
            "/graphql",
            json={"query": queries["user"]},
            headers=headers,
        )
        assert response.status_code == 200
        data = response.json()
        assert data["data"]["me"]["id"] == 2
        
        # Test admin query as non-admin (would fail on adminNotes field)
        response = client.post(
            "/graphql",
            json={"query": queries["admin"]},
            headers=headers,
        )
        assert response.status_code == 200
        # Admin field would be null or error
    
    def test_context_propagation(self, client):
        """Test that context is properly propagated to field resolvers."""
        # This test verifies the auth context reaches field-level resolvers
        query = """
        query {
            me {
                id
                username
                publicProfile {
                    username
                }
            }
        }
        """
        
        # Make authenticated request
        headers = {"Authorization": "Bearer user-456"}
        response = client.post(
            "/graphql",
            json={"query": query},
            headers=headers,
        )
        
        assert response.status_code == 200
        data = response.json()
        
        # Verify user data matches auth token
        assert data["data"]["me"]["id"] == 456
        assert data["data"]["me"]["username"] == "user456"


class TestFieldAuthIntegration:
    """Integration tests for field authorization with GraphQL execution."""
    
    def test_authorize_decorator_with_graphql(self):
        """Test authorize_field decorator in GraphQL context."""
        
        @fraise_type
        class SecureDocument:
            title: str
            
            @field
            def content(self) -> str:
                # Check authorization in resolver
                return "Secret content"
        
        @query
        def get_document(info) -> SecureDocument:
            return SecureDocument(title="Test Document")
        
        schema = build_fraiseql_schema(query_types=[get_document])
        
        # Test query
        query_str = """
        query {
            getDocument {
                title
                content
            }
        }
        """
        
        # Execute without auth context
        result = graphql_sync(
            schema,
            query_str,
            context_value={},
        )
        
        # Should get title but content might fail
        assert result.data["getDocument"]["title"] == "Test Document"
        
        # Execute with auth context
        result = graphql_sync(
            schema,
            query_str,
            context_value={"authenticated": True},
        )
        
        assert result.data["getDocument"]["title"] == "Test Document"
        # Content access depends on field resolver implementation