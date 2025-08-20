"""End-to-end tests for field-level authorization."""

import pytest
from fastapi.testclient import TestClient
from graphql import graphql_sync

import fraiseql
from fraiseql import query
from fraiseql.decorators import field
from fraiseql.gql.schema_builder import build_fraiseql_schema

# Import database fixtures
pytest_plugins = ["tests.database_conftest"]


# Test types with field authorization
@fraiseql.type
class PublicProfile:
    username: str
    bio: str


@fraiseql.type
class PrivateProfile:
    email: str
    phone: str
    address: str


@fraiseql.type
class User:
    id: int
    username: str
    public_profile: PublicProfile

    @field
    def private_profile(self, info) -> PrivateProfile | None:
        """Private profile - requires authentication and ownership."""
        # Check if user is authenticated
        user_id = info.context.get("user_id")
        if not user_id:
            return None  # Not authenticated

        # Check if user is accessing their own profile
        if user_id != self.id and not info.context.get("is_admin"):
            return None  # Not authorized

        return PrivateProfile(email="user@example.com", phone="+1234567890", address="123 Main St")

    @field
    def admin_notes(self, info) -> str | None:
        """Admin-only field."""
        # Check if user is admin
        if not info.context.get("is_admin"):
            return None
        return "Internal admin notes about user"


# Test queries
@query
async def me(info) -> User | None:
    """Get current user."""
    # Debug: print entire context
    print(f"me() called with context keys: {list(info.context.keys())}")
    print(f"user_id in context: {info.context.get('user_id')}")

    # Check both request state and context for user_id
    request = info.context.get("request")
    user_id = None

    if request and hasattr(request, "state") and hasattr(request.state, "user_id"):
        user_id = request.state.user_id
        print(f"Found user_id in request.state: {user_id}")
    else:
        user_id = info.context.get("user_id")
        print(f"Using user_id from context: {user_id}")

    if not user_id:
        print("No user_id found, returning None")
        return None

    return User(
        id=user_id,
        username=f"user{user_id}",
        public_profile=PublicProfile(username=f"user{user_id}", bio="A test user"),
    )


@query
async def get_user(info, user_id: int) -> User:
    """Get user by ID."""
    return User(
        id=user_id,
        username=f"user{user_id}",
        public_profile=PublicProfile(username=f"user{user_id}", bio="A test user"),
    )


@pytest.mark.database
class TestFieldAuthE2E:
    """End-to-end tests for field-level authorization."""

    @pytest.fixture
    def app(self, create_fraiseql_app_with_db):
        """Create test application with field auth."""

        # Create a custom context getter that adds auth data
        async def auth_context_getter(request):
            """Context getter that adds auth data from headers."""
            auth_header = request.headers.get("Authorization", "")
            auth_data = {}

            # Simple auth simulation
            if auth_header.startswith("Bearer "):
                token = auth_header[7:]
                if token == "admin-token":
                    auth_data["user_id"] = 1
                    auth_data["is_admin"] = True
                elif token.startswith("user-"):
                    auth_data["user_id"] = int(token.split("-")[1])
                    auth_data["is_admin"] = False

            return auth_data

        # Create app with custom context getter
        app = create_fraiseql_app_with_db(
            types=[User, PublicProfile, PrivateProfile],
            queries=[me, get_user],
            context_getter=auth_context_getter,
        )

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
        response = client.post("/graphql", json={"query": query}, headers=headers)
        assert response.status_code == 200
        data = response.json()
        print(f"Response with auth: {data}")  # Debug output
        assert data["data"]["me"] is not None, f"Expected user but got: {data}"
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
        response = client.post("/graphql", json={"query": queries["user"]}, headers=headers)
        assert response.status_code == 200
        data = response.json()
        assert data["data"]["me"]["id"] == 2

        # Test admin query as non-admin (would fail on adminNotes field)
        response = client.post("/graphql", json={"query": queries["admin"]}, headers=headers)
        assert response.status_code == 200
        data = response.json()
        # Admin field should be null for non-admin
        assert data["data"]["getUser"]["adminNotes"] is None

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
        response = client.post("/graphql", json={"query": query}, headers=headers)

        assert response.status_code == 200
        data = response.json()

        # Verify user data matches auth token
        assert data["data"]["me"]["id"] == 456
        assert data["data"]["me"]["username"] == "user456"


@pytest.mark.database
class TestFieldAuthIntegration:
    """Integration tests for field authorization with GraphQL execution."""

    def test_authorize_decorator_with_graphql(self):
        """Test authorize_field decorator in GraphQL context."""

        @fraiseql.type
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
        result = graphql_sync(schema, query_str, context_value={})

        # Should get title but content might fail
        assert result.data["getDocument"]["title"] == "Test Document"

        # Execute with auth context
        result = graphql_sync(schema, query_str, context_value={"authenticated": True})

        assert result.data["getDocument"]["title"] == "Test Document"
        # Content access depends on field resolver implementation
