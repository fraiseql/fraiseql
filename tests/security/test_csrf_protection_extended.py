"""Extended tests for CSRF protection to improve coverage."""

import base64
import json
import time
from unittest.mock import AsyncMock, MagicMock, Mock, patch

import pytest
from fastapi import FastAPI, Response

from fraiseql.security.csrf_protection import (
    CSRFConfig,
    CSRFProtectionMiddleware,
    CSRFTokenEndpoint,
    CSRFTokenGenerator,
    CSRFTokenStorage,
    GraphQLCSRFValidator,
    create_development_csrf_config,
    create_production_csrf_config,
    setup_csrf_protection,
)


class TestCSRFTokenGenerator:
    """Test CSRF token generation and validation."""

    @pytest.fixture
    def generator(self):
        """Create token generator."""
        return CSRFTokenGenerator("test-secret-key", timeout=3600)

    def test_generate_token_without_session(self, generator):
        """Test token generation without session ID."""
        token = generator.generate_token()

        assert isinstance(token, str)
        assert len(token) > 0

        # Should be valid base64
        decoded = base64.urlsafe_b64decode(token.encode()).decode()
        assert ":" in decoded

    def test_generate_token_with_session(self, generator):
        """Test token generation with session ID."""
        session_id = "test-session-123"
        token = generator.generate_token(session_id)

        # Decode and verify session ID is included
        decoded = base64.urlsafe_b64decode(token.encode()).decode()
        assert session_id in decoded

    def test_validate_token_success(self, generator):
        """Test successful token validation."""
        token = generator.generate_token()
        assert generator.validate_token(token) is True

    def test_validate_token_with_session(self, generator):
        """Test token validation with session ID."""
        session_id = "test-session"
        token = generator.generate_token(session_id)

        # Should validate with correct session ID
        assert generator.validate_token(token, session_id) is True

        # Should fail with wrong session ID
        assert generator.validate_token(token, "wrong-session") is False

    def test_validate_token_invalid_format(self, generator):
        """Test validation with invalid token format."""
        # Invalid base64
        assert generator.validate_token("invalid-token") is False

        # Valid base64 but wrong format
        invalid_token = base64.urlsafe_b64encode(b"wrong:format").decode()
        assert generator.validate_token(invalid_token) is False

    def test_validate_token_wrong_signature(self, generator):
        """Test validation with tampered token."""
        # Generate valid token
        token = generator.generate_token()

        # Decode and tamper with it
        decoded = base64.urlsafe_b64decode(token.encode()).decode()
        parts = decoded.split(":")
        parts[-1] = "wrong-signature"
        tampered = ":".join(parts)
        tampered_token = base64.urlsafe_b64encode(tampered.encode()).decode()

        assert generator.validate_token(tampered_token) is False

    def test_validate_token_expired(self, generator):
        """Test validation with expired token."""
        # Create generator with short timeout
        short_generator = CSRFTokenGenerator("test-secret", timeout=1)
        token = short_generator.generate_token()

        # Wait for expiration
        time.sleep(2)

        assert short_generator.validate_token(token) is False

    def test_token_with_bytes_secret(self):
        """Test generator with bytes secret key."""
        generator = CSRFTokenGenerator(b"bytes-secret-key")
        token = generator.generate_token()
        assert generator.validate_token(token) is True


class TestGraphQLCSRFValidator:
    """Test GraphQL-specific CSRF validation."""

    @pytest.fixture
    def validator(self):
        """Create GraphQL CSRF validator."""
        config = CSRFConfig(
            secret_key="test-secret", require_for_mutations=True, require_for_subscriptions=False
        )
        return GraphQLCSRFValidator(config)

    def test_extract_operation_type_mutation(self, validator):
        """Test extracting mutation operation type."""
        bodies = [
            {"query": "mutation CreateUser { ... }"},
            {"query": "MUTATION UpdatePost { ... }"},
            {"query": "  mutation  DeleteItem { ... }"},
        ]

        for body in bodies:
            assert validator._extract_operation_type(body) == "mutation"

    def test_extract_operation_type_query(self, validator):
        """Test extracting query operation type."""
        bodies = [
            {"query": "query GetUser { ... }"},
            {"query": "QUERY ListPosts { ... }"},
            {"query": "{ user { id } }"},  # Anonymous query
            {"query": "  { items { name } }"},
        ]

        for body in bodies:
            assert validator._extract_operation_type(body) == "query"

    def test_extract_operation_type_subscription(self, validator):
        """Test extracting subscription operation type."""
        bodies = [
            {"query": "subscription OnMessage { ... }"},
            {"query": "SUBSCRIPTION Updates { ... }"},
        ]

        for body in bodies:
            assert validator._extract_operation_type(body) == "subscription"

    def test_extract_operation_type_invalid(self, validator):
        """Test extracting operation type from invalid query."""
        bodies = [
            {"query": ""},
            {"query": "invalid graphql"},
            {},
            {"notQuery": "mutation Test { ... }"},
        ]

        for body in bodies:
            result = validator._extract_operation_type(body)
            assert result is None or result == "query"

    def test_requires_csrf_protection(self, validator):
        """Test checking if operation requires CSRF protection."""
        assert validator._requires_csrf_protection("mutation") is True
        assert validator._requires_csrf_protection("subscription") is False
        assert validator._requires_csrf_protection("query") is False

    def test_requires_csrf_protection_custom_config(self):
        """Test CSRF requirements with custom config."""
        config = CSRFConfig(
            secret_key="test", require_for_mutations=False, require_for_subscriptions=True
        )
        validator = GraphQLCSRFValidator(config)

        assert validator._requires_csrf_protection("mutation") is False
        assert validator._requires_csrf_protection("subscription") is True

    @pytest.mark.asyncio
    async def test_validate_request_success(self, validator):
        """Test successful request validation."""
        # Mock request with valid token
        request = AsyncMock()
        request.headers = {"x-csrf-token": "valid-token"}
        request.cookies = {}

        # Mock token validation
        with patch.object(validator.token_generator, "validate_token", return_value=True):
            result = await validator.validate_request(request)
            assert result is True

    @pytest.mark.asyncio
    async def test_validate_request_mutation_no_token(self, validator):
        """Test mutation request without CSRF token."""
        request = AsyncMock()
        request.headers = {}
        request.cookies = {}

        result = await validator.validate_request(request, {"query": "mutation CreateUser { ... }"})
        assert result is False

    @pytest.mark.asyncio
    async def test_validate_request_header_token(self, validator):
        """Test validation with token in header."""
        request = AsyncMock()
        request.headers = {"x-csrf-token": "header-token"}
        request.cookies = {}
        request.state = AsyncMock()
        request.state.session_id = None

        with patch.object(
            validator.token_generator, "validate_token", return_value=True
        ) as mock_validate:
            result = await validator.validate_request(request, {"query": "mutation Test { ... }"})
            assert result is True
            # The validator may extract token from body, so check if it was called
            assert mock_validate.called

    @pytest.mark.asyncio
    async def test_validate_request_cookie_token(self, validator):
        """Test validation with token in cookie."""
        validator.config.storage = CSRFTokenStorage.COOKIE

        request = AsyncMock()
        request.headers = {}
        request.cookies = {"csrf_token": "cookie-token"}
        request.state = AsyncMock()
        request.state.session_id = None

        with patch.object(
            validator.token_generator, "validate_token", return_value=True
        ) as mock_validate:
            result = await validator.validate_request(request, {"query": "mutation Test { ... }"})
            assert result is True
            # The validator may extract token from body, so check if it was called
            assert mock_validate.called

    @pytest.mark.asyncio
    async def test_check_referrer_header(self, validator):
        """Test referrer header checking through middleware."""
        # Create middleware instance for referrer checking
        config = CSRFConfig(
            secret_key="test",
            check_referrer=True,
            trusted_origins={"https://app.example.com", "http://localhost:3000"}
        )
        app = AsyncMock()
        middleware = CSRFProtectionMiddleware(app, config)

        # Valid referrer
        request = Mock()
        request.headers = {"Referer": "https://app.example.com/page"}
        result = middleware._validate_referrer(request)
        assert result is None  # None means valid

        # Invalid referrer
        request2 = Mock()
        request2.headers = {"Referer": "https://evil.com/attack"}
        result = middleware._validate_referrer(request2)
        assert result is not None  # Should return error response
        assert result.status_code == 403

        # No referrer (should fail when check is enabled)
        request3 = Mock()
        request3.headers = {}
        result = middleware._validate_referrer(request3)
        assert result is not None  # Should return error response
        assert result.status_code == 403


class TestCSRFProtectionMiddleware:
    """Test CSRF protection middleware."""

    @pytest.fixture
    def app(self):
        """Create test app with middleware."""
        app = FastAPI()
        return app

    @pytest.fixture
    def middleware(self, app):
        """Create middleware instance."""
        config = CSRFConfig(
            secret_key="test-secret", cookie_secure=False, exempt_paths={"/health", "/metrics"}
        )
        return CSRFProtectionMiddleware(app, config)

    @pytest.mark.asyncio
    async def test_middleware_allows_safe_methods(self, middleware):
        """Test middleware allows GET/HEAD/OPTIONS without CSRF."""
        request = MagicMock()
        request.method = "GET"
        request.url.path = "/api/data"

        response = Response()

        async def call_next(req):
            return response

        result = await middleware.dispatch(request, call_next)
        assert result is response

    @pytest.mark.asyncio
    async def test_middleware_exempt_paths(self, middleware):
        """Test middleware skips exempt paths."""
        request = MagicMock()
        request.method = "POST"
        request.url.path = "/health"

        response = Response()

        async def call_next(req):
            return response

        result = await middleware.dispatch(request, call_next)
        assert result is response

    @pytest.mark.asyncio
    async def test_middleware_graphql_validation(self, middleware):
        """Test middleware validates GraphQL requests."""
        request = AsyncMock()
        request.method = "POST"
        request.url.path = "/graphql"
        request.headers = {}
        request.cookies = {}

        # Mock body with mutation
        async def get_body():
            return json.dumps({"query": "mutation Test { ... }"}).encode()

        request.body = get_body

        response = Response()

        async def call_next(req):
            return response

        # Should reject without CSRF token
        result = await middleware.dispatch(request, call_next)
        assert result.status_code == 403

    @pytest.mark.asyncio
    async def test_middleware_generates_token_for_get(self, middleware):
        """Test middleware generates CSRF token for GET requests."""
        request = MagicMock()
        request.method = "GET"
        request.url.path = "/page"
        request.cookies = {}

        response = Response()

        async def call_next(req):
            return response

        with patch.object(middleware.token_generator, "generate_token", return_value="new-token"):
            result = await middleware.dispatch(request, call_next)

            # Should set cookie
            assert "csrf_token=new-token" in result.headers.get("set-cookie", "")


class TestCSRFConfigurations:
    """Test CSRF configuration presets."""

    def test_development_config(self):
        """Test development CSRF configuration."""
        config = create_development_csrf_config("dev-secret")

        assert config.secret_key == "dev-secret"
        assert config.cookie_secure is False
        assert config.cookie_samesite == "lax"
        assert config.check_referrer is False

    def test_production_config(self):
        """Test production CSRF configuration."""
        config = create_production_csrf_config(
            secret_key="prod-secret", trusted_origins={"https://app.com"}
        )

        assert config.secret_key == "prod-secret"
        assert config.cookie_secure is True
        assert config.cookie_samesite == "strict"
        assert config.check_referrer is True
        assert config.trusted_origins == {"https://app.com"}


class TestCSRFTokenEndpoint:
    """Test CSRF token endpoint."""

    def test_endpoint_creation(self):
        """Test creating CSRF token endpoint."""
        config = CSRFConfig(secret_key="test")
        endpoint = CSRFTokenEndpoint(config)

        assert endpoint.config is config
        # CSRFTokenEndpoint doesn't have a path attribute, it's set when registered with the app

    @pytest.mark.asyncio
    async def test_endpoint_generates_token(self):
        """Test endpoint generates new token."""
        config = CSRFConfig(secret_key="test", cookie_secure=False)
        endpoint = CSRFTokenEndpoint(config)

        request = AsyncMock()
        request.state = AsyncMock()
        request.state.session_id = None
        request.cookies = {}

        response = await endpoint.get_csrf_token(request)

        # Should return token in response
        assert "csrf_token" in response
        assert isinstance(response["csrf_token"], str)
        assert response["token_name"] == config.token_name
        assert response["header_name"] == config.header_name


class TestCSRFSetup:
    """Test CSRF setup function."""

    def test_setup_csrf_protection(self):
        """Test setting up CSRF protection on app."""
        app = FastAPI()
        secret_key = "test"

        middleware = setup_csrf_protection(app, secret_key)

        # Should return middleware instance
        assert isinstance(middleware, CSRFProtectionMiddleware)

        # Should add token endpoint
        routes = [r.path for r in app.routes]
        assert "/csrf-token" in routes
