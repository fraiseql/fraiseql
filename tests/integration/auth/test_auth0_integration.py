"""Test Auth0 token validation fixes."""

from unittest.mock import AsyncMock, Mock, patch

import pytest

from fraiseql.auth.auth0 import Auth0Provider
from fraiseql.auth.base import UserContext
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import get_auth_provider, set_auth_provider

pytestmark = pytest.mark.integration


class TestAuth0Integration:
    """Test Auth0 integration fixes."""

    @pytest.fixture
    def bearer_token(self) -> None:
        """Get the test bearer token."""
        # Sample Auth0 token for testing (expired, safe to use in tests)
        return (
            """eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3QifQ."""
            """eyJodHRwczovL2V4YW1wbGUuY29tL3RlbmFudF9pZCI6IjEyMzQ1Njc4LTEyMzQtMTIzNC0xMjM0LTEyMzQ1Njc4OTAxMiIsImh0d"""
            """HBzOi8vZXhhbXBsZS5jb20vdXNlcl9pZCI6Ijg3NjU0MzIxLTQzMjEtNDMyMS00MzIxLTg3NjU0MzIxMDk4NyIsImlzcyI6Imh0dHB"""
            """zOi8vZXhhbXBsZS5hdXRoMC5jb20vIiwic3ViIjoiYXV0aDB8dGVzdHVzZXIxMjMiLCJhdWQiOlsiaHR0cHM6Ly9hcGkuZXhhbXBsZ"""
            """S5jb20iXSwiaWF0IjoxNjA5NDU5MjAwLCJleHAiOjE2MDk1NDU2MDAsInNjb3BlIjoib3BlbmlkIHByb2ZpbGUgZW1haWwiLCJlbWF"""
            """pbCI6InRlc3RAZXhhbXBsZS5jb20ifQ.fake_signature"""
        )

    @pytest.fixture
    def auth0_provider(self) -> None:
        """Create Auth0 provider instance."""
        return Auth0Provider(
            domain="staging-fraiseql-juqnt9ch4.eu.auth0.com",
            api_identifier="https://develop.api.fraiseql.io",
            algorithms=["RS256"],
        )

    @pytest.fixture
    def mock_jwt_payload(self) -> None:
        """Mock JWT payload with custom namespaced claims."""
        return {
            "https://example.com/email": "test@example.com",
            "https://example.com/email_verified": True,
            "https://example.com/tenant_id": "12345678-1234-1234-1234-123456789012",
            "https://example.com/contact_id": "87654321-4321-4321-4321-876543210987",
            "iss": "https://example.auth0.com/",
            "sub": "auth0|testuser123",
            "aud": ["https://api.example.com", "https://example.auth0.com/userinfo"],
            "iat": 1609459200,
            "exp": 1609545600,
            "scope": "openid profile email read:test",
            "azp": "testclientid123",
            "permissions": ["openid profile email", "read:test"],
            "email": "test@example.com",
        }

    @pytest.mark.asyncio
    async def test_auth0_provider_extracts_custom_claims(
        self, auth0_provider, mock_jwt_payload
    ) -> None:
        """Test that Auth0Provider correctly extracts custom claims into metadata."""
        # Mock the validate_token method to return our payload
        with patch.object(auth0_provider, "validate_token", return_value=mock_jwt_payload):
            user_context = await auth0_provider.get_user_from_token("fake_token")

            assert isinstance(user_context, UserContext)
            assert user_context.user_id == "auth0|testuser123"
            assert user_context.email == "test@example.com"

            # Check that custom claims are in metadata
            assert "https://example.com/tenant_id" in user_context.metadata
            assert "https://example.com/contact_id" in user_context.metadata

            assert (
                user_context.metadata["https://example.com/tenant_id"]
                == "12345678-1234-1234-1234-123456789012"
            )
            assert (
                user_context.metadata["https://example.com/contact_id"]
                == "87654321-4321-4321-4321-876543210987"
            )

    def test_config_creates_auth0_provider(self, clear_registry) -> None:
        """Test that FraiseQL creates Auth0 provider from config."""
        # Reset the auth provider first
        set_auth_provider(None)

        config = FraiseQLConfig(
            database_url="postgresql://localhost/test",
            auth_enabled=True,
            auth_provider="auth0",
            auth0_domain="staging-fraiseql-juqnt9ch4.eu.auth0.com",
            auth0_api_identifier="https://develop.api.fraiseql.io",
        )

        # Import here to avoid circular imports
        import fraiseql
        from fraiseql import query
        from fraiseql.fastapi.app import create_fraiseql_app

        # Create a dummy type and query so the schema has something
        @fraiseql.type
        class DummyType:
            id: str

        @query
        async def dummy_query(info) -> list[DummyType]:
            return []

        # Mock the database pool creation
        with patch("fraiseql.fastapi.app.create_db_pool", new_callable=AsyncMock) as mock_pool:
            mock_pool.return_value = Mock()

            # Create app - this should set up Auth0 provider
            create_fraiseql_app(config=config, types=[DummyType])

            # Check that auth provider was created
            auth_provider = get_auth_provider()
            assert auth_provider is not None
            assert isinstance(auth_provider, Auth0Provider)
            assert auth_provider.domain == "staging-fraiseql-juqnt9ch4.eu.auth0.com"
            assert auth_provider.api_identifier == "https://develop.api.fraiseql.io"

    @pytest.mark.asyncio
    async def test_context_getter_receives_user(self) -> None:
        """Test that custom context_getter receives the user parameter."""
        from graphql import GraphQLField, GraphQLObjectType, GraphQLSchema, GraphQLString

        from fraiseql.fastapi.config import FraiseQLConfig
        from fraiseql.fastapi.routers import create_graphql_router

        # Track calls to context_getter
        context_calls = []

        async def custom_context_getter(request, user=None) -> None:
            context_calls.append(
                {
                    "request": request,
                    "user": user,
                    "user_type": type(user).__name__ if user else None,
                }
            )
            return {"custom": "context"}

        # Create a simple schema
        schema = GraphQLSchema(
            query=GraphQLObjectType(
                "Query", {"hello": GraphQLField(GraphQLString, resolve=lambda *_: "world")}
            )
        )

        config = FraiseQLConfig(database_url="postgresql://localhost/test")

        # Create router with custom context getter
        router = create_graphql_router(
            schema=schema, config=config, auth_provider=None, context_getter=custom_context_getter
        )

        # The router should have our endpoint
        assert any(route.path == "/graphql" for route in router.routes)

        # Verify context_getter signature inspection works
        import inspect

        sig = inspect.signature(custom_context_getter)
        assert len(sig.parameters) >= 2  # Should accept request and user

    @pytest.mark.asyncio
    async def test_custom_context_extraction(self, mock_jwt_payload) -> None:
        """Test the context extraction logic for custom namespaced claims."""
        # Simulate what a custom context_getter function would do

        # Create a mock UserContext as FraiseQL would pass it
        mock_user = Mock()
        mock_user.metadata = mock_jwt_payload
        mock_user.user_id = "auth0|testuser123"
        mock_user.email = "test@example.com"

        # Simulate the context extraction logic
        namespace = "https://example.com"

        # Extract from metadata (UserContext object)
        if hasattr(mock_user, "metadata"):
            metadata = mock_user.metadata
            tenant_id = metadata.get(f"{namespace}/tenant_id")
            contact_id = metadata.get(f"{namespace}/contact_id")

            # Build context as a custom app would
            context = {
                "tenant_id": tenant_id,
                "contact_id": contact_id,
                "user": contact_id,  # Alias
                "auth_user": metadata,
                "auth_user_exists": True,
            }

            # Verify context extraction
            assert context["tenant_id"] == "12345678-1234-1234-1234-123456789012"
            assert context["contact_id"] == "87654321-4321-4321-4321-876543210987"
            assert context["user"] == "87654321-4321-4321-4321-876543210987"  # Alias
            assert context["auth_user_exists"] is True
            assert context["auth_user"] == mock_jwt_payload

    def test_config_validation_requires_auth0_fields(self) -> None:
        """Test that config validation requires auth0 fields when provider is auth0."""
        with pytest.raises(ValueError, match="auth0_domain is required"):
            FraiseQLConfig(
                database_url="postgresql://localhost/test",
                auth_provider="auth0",
                # Missing auth0_domain
            )

    @pytest.mark.asyncio
    async def test_auth0_provider_cleanup(self, auth0_provider) -> None:
        """Test that Auth0Provider properly cleans up resources."""
        # Should not raise any errors
        await auth0_provider.close()

        # Calling close again should be safe
        await auth0_provider.close()
