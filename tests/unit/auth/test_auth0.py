"""Tests for auth0 authentication provider module."""

from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from fraiseql.auth.auth0 import Auth0Config, Auth0Provider
from fraiseql.auth.base import (
    AuthenticationError,
    InvalidTokenError,
    TokenExpiredError,
    UserContext,
)


# Test fixtures
@pytest.fixture
def mock_jwks_client() -> MagicMock:
    """Create mock PyJWKClient."""
    mock = MagicMock()
    mock_key = MagicMock()
    mock_key.key = "test_signing_key"
    mock.get_signing_key_from_jwt.return_value = mock_key
    return mock


@pytest.fixture
def auth0_provider(mock_jwks_client: MagicMock) -> Auth0Provider:
    """Create Auth0Provider with mocked JWKS client."""
    with patch("fraiseql.auth.auth0.PyJWKClient", return_value=mock_jwks_client):
        provider = Auth0Provider(
            domain="test.auth0.com",
            api_identifier="https://api.test.com",
        )
    provider.jwks_client = mock_jwks_client
    return provider


# Tests for Auth0Provider initialization
@pytest.mark.unit
@pytest.mark.security
class TestAuth0ProviderInit:
    """Tests for Auth0Provider initialization."""

    def test_init_sets_domain(self) -> None:
        """Auth0Provider stores the domain."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
            )

        assert provider.domain == "myapp.auth0.com"

    def test_init_sets_api_identifier(self) -> None:
        """Auth0Provider stores the api_identifier."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
            )

        assert provider.api_identifier == "https://api.myapp.com"

    def test_init_sets_issuer(self) -> None:
        """Auth0Provider computes issuer URL from domain."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
            )

        assert provider.issuer == "https://myapp.auth0.com/"

    def test_init_sets_jwks_uri(self) -> None:
        """Auth0Provider computes JWKS URI from domain."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
            )

        assert provider.jwks_uri == "https://myapp.auth0.com/.well-known/jwks.json"

    def test_init_default_algorithms(self) -> None:
        """Auth0Provider uses RS256 by default."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
            )

        assert provider.algorithms == ["RS256"]

    def test_init_custom_algorithms(self) -> None:
        """Auth0Provider accepts custom algorithms."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
                algorithms=["RS256", "RS384"],
            )

        assert provider.algorithms == ["RS256", "RS384"]

    def test_init_http_client_is_none(self) -> None:
        """Auth0Provider starts with no HTTP client."""
        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = Auth0Provider(
                domain="myapp.auth0.com",
                api_identifier="https://api.myapp.com",
            )

        assert provider._http_client is None


# Tests for validate_token
@pytest.mark.unit
@pytest.mark.security
class TestValidateToken:
    """Tests for Auth0Provider.validate_token method."""

    @pytest.mark.asyncio
    async def test_validate_token_success(self, auth0_provider: Auth0Provider) -> None:
        """validate_token returns decoded payload for valid token."""
        expected_payload = {
            "sub": "auth0|123",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
            "email": "test@example.com",
        }

        with (
            patch("fraiseql.auth.auth0.jwt.decode", return_value=expected_payload),
            patch("fraiseql.auth.auth0.get_security_logger"),
        ):
            result = await auth0_provider.validate_token("valid_token")

        assert result == expected_payload

    @pytest.mark.asyncio
    async def test_validate_token_expired_raises_token_expired_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """validate_token raises TokenExpiredError for expired tokens."""
        import jwt as pyjwt

        with patch("fraiseql.auth.auth0.jwt.decode") as mock_decode:
            mock_decode.side_effect = pyjwt.ExpiredSignatureError("Token expired")
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                with pytest.raises(TokenExpiredError, match="Token has expired"):
                    await auth0_provider.validate_token("expired_token")

    @pytest.mark.asyncio
    async def test_validate_token_invalid_raises_invalid_token_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """validate_token raises InvalidTokenError for invalid tokens."""
        import jwt as pyjwt

        with patch("fraiseql.auth.auth0.jwt.decode") as mock_decode:
            mock_decode.side_effect = pyjwt.InvalidTokenError("Bad signature")
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                with pytest.raises(InvalidTokenError, match="Invalid token"):
                    await auth0_provider.validate_token("invalid_token")

    @pytest.mark.asyncio
    async def test_validate_token_general_error_raises_authentication_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """validate_token raises AuthenticationError for other errors."""
        with patch("fraiseql.auth.auth0.jwt.decode") as mock_decode:
            mock_decode.side_effect = RuntimeError("Unexpected error")
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                with pytest.raises(AuthenticationError, match="Token validation failed"):
                    await auth0_provider.validate_token("token")


# Tests for get_user_from_token
@pytest.mark.unit
@pytest.mark.security
class TestGetUserFromToken:
    """Tests for Auth0Provider.get_user_from_token method."""

    @pytest.mark.asyncio
    async def test_get_user_from_token_extracts_user_context(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_from_token returns UserContext with user info."""
        payload = {
            "sub": "auth0|123",
            "email": "test@example.com",
            "name": "Test User",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
            "iat": 1234567890,
            "exp": 1234571490,
        }

        with patch.object(
            auth0_provider, "validate_token", new_callable=AsyncMock
        ) as mock_validate:
            mock_validate.return_value = payload
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                result = await auth0_provider.get_user_from_token("token")

        assert isinstance(result, UserContext)
        assert result.user_id == "auth0|123"
        assert result.email == "test@example.com"
        assert result.name == "Test User"

    @pytest.mark.asyncio
    async def test_get_user_from_token_extracts_roles_from_namespace(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_from_token extracts roles from namespaced claims."""
        # Namespace is built as: f"https://{self.api_identifier}/"
        # With api_identifier="https://api.test.com", this becomes:
        # "https://https://api.test.com/" - the code expects the API identifier as-is
        payload = {
            "sub": "auth0|123",
            "email": "test@example.com",
            "https://https://api.test.com/roles": ["admin", "user"],
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
        }

        with patch.object(
            auth0_provider, "validate_token", new_callable=AsyncMock
        ) as mock_validate:
            mock_validate.return_value = payload
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                result = await auth0_provider.get_user_from_token("token")

        assert result.roles == ["admin", "user"]

    @pytest.mark.asyncio
    async def test_get_user_from_token_parses_scope_as_permissions(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_from_token parses scope string into permissions."""
        payload = {
            "sub": "auth0|123",
            "scope": "read:users write:users delete:users",
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
        }

        with patch.object(
            auth0_provider, "validate_token", new_callable=AsyncMock
        ) as mock_validate:
            mock_validate.return_value = payload
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                result = await auth0_provider.get_user_from_token("token")

        assert result.permissions == ["read:users", "write:users", "delete:users"]

    @pytest.mark.asyncio
    async def test_get_user_from_token_uses_permissions_claim_if_present(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_from_token uses permissions claim directly if present."""
        payload = {
            "sub": "auth0|123",
            "permissions": ["read:data", "write:data"],
            "aud": "https://api.test.com",
            "iss": "https://test.auth0.com/",
        }

        with patch.object(
            auth0_provider, "validate_token", new_callable=AsyncMock
        ) as mock_validate:
            mock_validate.return_value = payload
            with patch("fraiseql.auth.auth0.get_security_logger") as mock_logger:
                mock_logger.return_value = MagicMock()
                result = await auth0_provider.get_user_from_token("token")

        assert result.permissions == ["read:data", "write:data"]


# Tests for get_user_profile
@pytest.mark.unit
@pytest.mark.security
class TestGetUserProfile:
    """Tests for Auth0Provider.get_user_profile method."""

    @pytest.mark.asyncio
    async def test_get_user_profile_success(self, auth0_provider: Auth0Provider) -> None:
        """get_user_profile returns profile data from Management API."""
        profile_data = {
            "user_id": "auth0|123",
            "email": "test@example.com",
            "name": "Test User",
            "picture": "https://example.com/avatar.jpg",
        }

        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = profile_data

        mock_client = AsyncMock()
        mock_client.get.return_value = mock_response
        auth0_provider._http_client = mock_client

        result = await auth0_provider.get_user_profile("auth0|123", "access_token")

        assert result == profile_data
        mock_client.get.assert_called_once_with(
            "https://test.auth0.com/api/v2/users/auth0|123",
            headers={"Authorization": "Bearer access_token"},
        )

    @pytest.mark.asyncio
    async def test_get_user_profile_http_error_raises_authentication_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_profile raises AuthenticationError on HTTP errors."""
        import httpx

        mock_client = AsyncMock()
        mock_client.get.side_effect = httpx.HTTPError("Connection failed")
        auth0_provider._http_client = mock_client

        with pytest.raises(AuthenticationError, match="Failed to fetch user profile"):
            await auth0_provider.get_user_profile("auth0|123", "access_token")

    @pytest.mark.asyncio
    async def test_get_user_profile_non_200_raises_authentication_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_profile raises AuthenticationError for non-200 status."""
        mock_response = MagicMock()
        mock_response.status_code = 404
        mock_response.text = "User not found"

        mock_client = AsyncMock()
        mock_client.get.return_value = mock_response
        auth0_provider._http_client = mock_client

        with pytest.raises(AuthenticationError, match="Failed to fetch user profile"):
            await auth0_provider.get_user_profile("auth0|123", "access_token")


# Tests for get_user_roles
@pytest.mark.unit
@pytest.mark.security
class TestGetUserRoles:
    """Tests for Auth0Provider.get_user_roles method."""

    @pytest.mark.asyncio
    async def test_get_user_roles_success(self, auth0_provider: Auth0Provider) -> None:
        """get_user_roles returns roles from Management API."""
        roles_data: list[dict[str, Any]] = [
            {"id": "rol_123", "name": "admin"},
            {"id": "rol_456", "name": "user"},
        ]

        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = roles_data

        mock_client = AsyncMock()
        mock_client.get.return_value = mock_response
        auth0_provider._http_client = mock_client

        result = await auth0_provider.get_user_roles("auth0|123", "access_token")

        assert result == roles_data
        mock_client.get.assert_called_once_with(
            "https://test.auth0.com/api/v2/users/auth0|123/roles",
            headers={"Authorization": "Bearer access_token"},
        )

    @pytest.mark.asyncio
    async def test_get_user_roles_non_200_raises_authentication_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_roles raises AuthenticationError for non-200 status."""
        mock_response = MagicMock()
        mock_response.status_code = 403
        mock_response.text = "Forbidden"

        mock_client = AsyncMock()
        mock_client.get.return_value = mock_response
        auth0_provider._http_client = mock_client

        with pytest.raises(AuthenticationError, match="Failed to fetch user roles"):
            await auth0_provider.get_user_roles("auth0|123", "access_token")


# Tests for get_user_permissions
@pytest.mark.unit
@pytest.mark.security
class TestGetUserPermissions:
    """Tests for Auth0Provider.get_user_permissions method."""

    @pytest.mark.asyncio
    async def test_get_user_permissions_success(self, auth0_provider: Auth0Provider) -> None:
        """get_user_permissions returns permissions from Management API."""
        permissions_data: list[dict[str, Any]] = [
            {"permission_name": "read:users", "resource_server_identifier": "api"},
            {"permission_name": "write:users", "resource_server_identifier": "api"},
        ]

        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.json.return_value = permissions_data

        mock_client = AsyncMock()
        mock_client.get.return_value = mock_response
        auth0_provider._http_client = mock_client

        result = await auth0_provider.get_user_permissions("auth0|123", "access_token")

        assert result == permissions_data
        mock_client.get.assert_called_once_with(
            "https://test.auth0.com/api/v2/users/auth0|123/permissions",
            headers={"Authorization": "Bearer access_token"},
        )

    @pytest.mark.asyncio
    async def test_get_user_permissions_non_200_raises_authentication_error(
        self, auth0_provider: Auth0Provider
    ) -> None:
        """get_user_permissions raises AuthenticationError for non-200 status."""
        mock_response = MagicMock()
        mock_response.status_code = 500
        mock_response.text = "Internal Server Error"

        mock_client = AsyncMock()
        mock_client.get.return_value = mock_response
        auth0_provider._http_client = mock_client

        with pytest.raises(AuthenticationError, match="Failed to fetch user permissions"):
            await auth0_provider.get_user_permissions("auth0|123", "access_token")


# Tests for close
@pytest.mark.unit
@pytest.mark.security
class TestClose:
    """Tests for Auth0Provider.close method."""

    @pytest.mark.asyncio
    async def test_close_closes_http_client(self, auth0_provider: Auth0Provider) -> None:
        """close() closes the HTTP client."""
        mock_client = AsyncMock()
        auth0_provider._http_client = mock_client

        await auth0_provider.close()

        mock_client.aclose.assert_called_once()
        assert auth0_provider._http_client is None

    @pytest.mark.asyncio
    async def test_close_does_nothing_when_no_client(self, auth0_provider: Auth0Provider) -> None:
        """close() is safe to call when no client exists."""
        auth0_provider._http_client = None

        await auth0_provider.close()  # Should not raise

        assert auth0_provider._http_client is None


# Tests for Auth0Config
@pytest.mark.unit
@pytest.mark.security
class TestAuth0Config:
    """Tests for Auth0Config class."""

    def test_auth0_config_init(self) -> None:
        """Auth0Config stores configuration values."""
        config = Auth0Config(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com",
            client_id="client123",
            client_secret="secret456",
            algorithms=["RS256", "RS384"],
        )

        assert config.domain == "myapp.auth0.com"
        assert config.api_identifier == "https://api.myapp.com"
        assert config.client_id == "client123"
        assert config.client_secret == "secret456"
        assert config.algorithms == ["RS256", "RS384"]

    def test_auth0_config_default_algorithms(self) -> None:
        """Auth0Config uses RS256 by default."""
        config = Auth0Config(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com",
        )

        assert config.algorithms == ["RS256"]

    def test_auth0_config_optional_client_credentials(self) -> None:
        """Auth0Config client credentials are optional."""
        config = Auth0Config(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com",
        )

        assert config.client_id is None
        assert config.client_secret is None

    def test_auth0_config_create_provider(self) -> None:
        """Auth0Config.create_provider creates Auth0Provider."""
        config = Auth0Config(
            domain="myapp.auth0.com",
            api_identifier="https://api.myapp.com",
            algorithms=["RS256"],
        )

        with patch("fraiseql.auth.auth0.PyJWKClient"):
            provider = config.create_provider()

        assert isinstance(provider, Auth0Provider)
        assert provider.domain == "myapp.auth0.com"
        assert provider.api_identifier == "https://api.myapp.com"
        assert provider.algorithms == ["RS256"]
