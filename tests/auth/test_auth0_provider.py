"""Tests for Auth0 authentication provider."""

import time
from unittest.mock import AsyncMock, MagicMock, patch

import httpx
import jwt
import pytest
from jwt import PyJWKClient

from fraiseql.auth.auth0 import Auth0Provider
from fraiseql.auth.base import (
    AuthenticationError,
    InvalidTokenError,
    TokenExpiredError,
    UserContext,
)


class TestAuth0Provider:
    """Test Auth0Provider class."""

    @pytest.fixture
    def auth0_provider(self):
        """Create an Auth0Provider instance."""
        return Auth0Provider(
            domain="test.auth0.com",
            api_identifier="https://api.test.com",
            algorithms=["RS256"],
            cache_jwks=True,
        )

    def test_initialization(self):
        """Test Auth0Provider initialization."""
        provider = Auth0Provider(
            domain="example.auth0.com",
            api_identifier="https://api.example.com",
        )
        
        assert provider.domain == "example.auth0.com"
        assert provider.api_identifier == "https://api.example.com"
        assert provider.algorithms == ["RS256"]
        assert provider.issuer == "https://example.auth0.com/"
        assert provider.jwks_uri == "https://example.auth0.com/.well-known/jwks.json"
        assert isinstance(provider.jwks_client, PyJWKClient)

    def test_initialization_with_custom_algorithms(self):
        """Test initialization with custom algorithms."""
        provider = Auth0Provider(
            domain="test.auth0.com",
            api_identifier="api",
            algorithms=["RS256", "HS256"],
            cache_jwks=False,
        )
        
        assert provider.algorithms == ["RS256", "HS256"]

    @pytest.mark.asyncio
    async def test_http_client_lazy_initialization(self, auth0_provider):
        """Test HTTP client is lazily initialized."""
        assert auth0_provider._http_client is None
        
        client = await auth0_provider.http_client
        assert isinstance(client, httpx.AsyncClient)
        assert auth0_provider._http_client is client
        
        # Second call returns same client
        client2 = await auth0_provider.http_client
        assert client2 is client

    @pytest.mark.asyncio
    async def test_close_http_client(self, auth0_provider):
        """Test closing HTTP client."""
        # Create client
        client = await auth0_provider.http_client
        assert auth0_provider._http_client is not None
        
        # Close it
        await auth0_provider.close()
        assert auth0_provider._http_client is None
        
        # Close again should not error
        await auth0_provider.close()

    @pytest.mark.asyncio
    async def test_validate_token_success(self, auth0_provider):
        """Test successful token validation."""
        mock_payload = {
            "sub": "auth0|123",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
            "exp": int(time.time()) + 3600,
            "iat": int(time.time()),
        }
        
        # Mock JWKS client and JWT decode
        with patch.object(auth0_provider.jwks_client, 'get_signing_key_from_jwt') as mock_jwks:
            mock_key = MagicMock()
            mock_key.key = "test-key"
            mock_jwks.return_value = mock_key
            
            with patch('jwt.decode', return_value=mock_payload) as mock_decode:
                result = await auth0_provider.validate_token("test-token")
                
                assert result == mock_payload
                mock_jwks.assert_called_once_with("test-token")
                mock_decode.assert_called_once_with(
                    "test-token",
                    "test-key",
                    algorithms=["RS256"],
                    audience="https://api.test.com",
                    issuer="https://test.auth0.com/",
                )

    @pytest.mark.asyncio
    async def test_validate_token_expired(self, auth0_provider):
        """Test token validation with expired token."""
        with patch.object(auth0_provider.jwks_client, 'get_signing_key_from_jwt'):
            with patch('jwt.decode', side_effect=jwt.ExpiredSignatureError("Token expired")):
                with pytest.raises(TokenExpiredError, match="Token has expired"):
                    await auth0_provider.validate_token("expired-token")

    @pytest.mark.asyncio
    async def test_validate_token_invalid(self, auth0_provider):
        """Test token validation with invalid token."""
        with patch.object(auth0_provider.jwks_client, 'get_signing_key_from_jwt'):
            with patch('jwt.decode', side_effect=jwt.InvalidTokenError("Invalid signature")):
                with pytest.raises(InvalidTokenError, match="Invalid token: Invalid signature"):
                    await auth0_provider.validate_token("invalid-token")

    @pytest.mark.asyncio
    async def test_validate_token_generic_error(self, auth0_provider):
        """Test token validation with generic error."""
        with patch.object(
            auth0_provider.jwks_client, 
            'get_signing_key_from_jwt',
            side_effect=Exception("Network error")
        ):
            with pytest.raises(AuthenticationError, match="Token validation failed: Network error"):
                await auth0_provider.validate_token("test-token")

    @pytest.mark.asyncio
    async def test_get_user_from_token_full_claims(self, auth0_provider):
        """Test getting user from token with full claims."""
        mock_payload = {
            "sub": "auth0|123456",
            "email": "user@example.com",
            "name": "Test User",
            "permissions": ["read:posts", "write:posts"],
            "https://api.test.com/roles": ["admin", "user"],
            "custom_claim": "custom_value",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
            "exp": int(time.time()) + 3600,
            "iat": int(time.time()),
        }
        
        with patch.object(auth0_provider, 'validate_token', return_value=mock_payload):
            user = await auth0_provider.get_user_from_token("test-token")
            
            assert isinstance(user, UserContext)
            assert user.user_id == "auth0|123456"
            assert user.email == "user@example.com"
            assert user.name == "Test User"
            assert user.permissions == ["read:posts", "write:posts"]
            assert user.roles == ["admin", "user"]
            assert user.metadata["custom_claim"] == "custom_value"
            assert "aud" not in user.metadata  # System claims excluded

    @pytest.mark.asyncio
    async def test_get_user_from_token_minimal_claims(self, auth0_provider):
        """Test getting user from token with minimal claims."""
        mock_payload = {
            "sub": "auth0|789",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
            "exp": int(time.time()) + 3600,
            "iat": int(time.time()),
        }
        
        with patch.object(auth0_provider, 'validate_token', return_value=mock_payload):
            user = await auth0_provider.get_user_from_token("test-token")
            
            assert user.user_id == "auth0|789"
            assert user.email is None
            assert user.name is None
            assert user.permissions == []
            assert user.roles == []
            assert user.metadata == {}

    @pytest.mark.asyncio
    async def test_get_user_from_token_with_scope(self, auth0_provider):
        """Test getting user from token with scope instead of permissions."""
        mock_payload = {
            "sub": "auth0|999",
            "scope": "read:posts write:posts delete:posts",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
        }
        
        with patch.object(auth0_provider, 'validate_token', return_value=mock_payload):
            user = await auth0_provider.get_user_from_token("test-token")
            
            assert user.permissions == ["read:posts", "write:posts", "delete:posts"]

    @pytest.mark.asyncio
    async def test_get_user_profile(self, auth0_provider):
        """Test fetching user profile from Management API."""
        mock_response = {
            "user_id": "auth0|123",
            "email": "user@example.com",
            "name": "Test User",
            "picture": "https://example.com/pic.jpg",
            "app_metadata": {"plan": "premium"},
            "user_metadata": {"preferences": {"theme": "dark"}},
        }
        
        # Mock HTTP client
        mock_client = AsyncMock()
        mock_client.get.return_value = AsyncMock(
            json=AsyncMock(return_value=mock_response),
            raise_for_status=AsyncMock(),
        )
        
        with patch.object(auth0_provider, 'http_client', new_callable=AsyncMock) as mock_http:
            mock_http.return_value = mock_client
            
            profile = await auth0_provider.get_user_profile("auth0|123", "access-token")
            
            assert profile == mock_response
            mock_client.get.assert_called_once_with(
                "https://test.auth0.com/api/v2/users/auth0|123",
                headers={"Authorization": "Bearer access-token"},
            )

    @pytest.mark.asyncio
    async def test_get_user_profile_error(self, auth0_provider):
        """Test error handling in get_user_profile."""
        # Mock HTTP client to raise an error
        mock_client = AsyncMock()
        mock_client.get.side_effect = httpx.HTTPError("Network error")
        
        with patch.object(auth0_provider, 'http_client', new_callable=AsyncMock) as mock_http:
            mock_http.return_value = mock_client
            
            with pytest.raises(AuthenticationError, match="Failed to fetch user profile"):
                await auth0_provider.get_user_profile("auth0|123", "access-token")

    @pytest.mark.asyncio
    async def test_refresh_token(self, auth0_provider):
        """Test token refresh functionality."""
        mock_response = {
            "access_token": "new-access-token",
            "id_token": "new-id-token",
            "token_type": "Bearer",
            "expires_in": 86400,
        }
        
        # Mock HTTP client
        mock_client = AsyncMock()
        mock_client.post.return_value = AsyncMock(
            json=AsyncMock(return_value=mock_response),
            raise_for_status=AsyncMock(),
        )
        
        with patch.object(auth0_provider, 'http_client', new_callable=AsyncMock) as mock_http:
            mock_http.return_value = mock_client
            
            result = await auth0_provider.refresh_token("refresh-token-value")
            
            assert result == mock_response
            mock_client.post.assert_called_once()
            call_args = mock_client.post.call_args
            assert call_args[0][0] == "https://test.auth0.com/oauth/token"
            assert call_args[1]["json"]["grant_type"] == "refresh_token"
            assert call_args[1]["json"]["refresh_token"] == "refresh-token-value"

    @pytest.mark.asyncio
    async def test_refresh_token_error(self, auth0_provider):
        """Test error handling in refresh_token."""
        # Mock HTTP client to return error response
        mock_client = AsyncMock()
        mock_response = AsyncMock()
        mock_response.raise_for_status.side_effect = httpx.HTTPStatusError(
            "400 Bad Request", 
            request=MagicMock(), 
            response=MagicMock(status_code=400)
        )
        mock_client.post.return_value = mock_response
        
        with patch.object(auth0_provider, 'http_client', new_callable=AsyncMock) as mock_http:
            mock_http.return_value = mock_client
            
            with pytest.raises(AuthenticationError, match="Token refresh failed"):
                await auth0_provider.refresh_token("invalid-refresh-token")