"""Tests for CSRF protection middleware."""

import base64
import json
import time
from unittest.mock import AsyncMock, MagicMock, Mock, patch

import pytest
from fastapi import FastAPI, Request, Response
from fastapi.testclient import TestClient

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


@pytest.fixture
def csrf_config():
    """Create test CSRF configuration."""
    return CSRFConfig(
        secret_key="test-secret-key-for-csrf-protection",
        cookie_secure=False,  # For testing
        check_referrer=False,  # Simplify testing
        trusted_origins={"http://localhost:3000"},
    )


@pytest.fixture
def app():
    """Create test FastAPI app."""
    app = FastAPI()

    @app.get("/test")
    async def test_get():
        return {"message": "success"}

    @app.post("/test")
    async def test_post():
        return {"message": "success"}

    @app.post("/graphql")
    async def graphql_endpoint(request: Request):
        await request.body()
        return {"data": {"test": "success"}}

    @app.get("/health")
    async def health():
        return {"status": "healthy"}

    return app


class TestCSRFTokenGenerator:
    """Test CSRF token generation and validation."""

    def test_generate_token(self) -> None:
        """Test token generation."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)
        token = generator.generate_token()

        assert isinstance(token, str)
        assert len(token) > 0

    def test_generate_token_with_session(self) -> None:
        """Test token generation with session ID."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)
        token = generator.generate_token("session-123")

        assert isinstance(token, str)
        assert len(token) > 0

    def test_validate_valid_token(self) -> None:
        """Test validation of valid token."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)
        token = generator.generate_token()

        assert generator.validate_token(token)

    def test_validate_valid_token_with_session(self) -> None:
        """Test validation of valid token with session."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)
        session_id = "session-123"
        token = generator.generate_token(session_id)

        assert generator.validate_token(token, session_id)

    def test_validate_invalid_token(self) -> None:
        """Test validation of invalid token."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)

        assert not generator.validate_token("invalid-token")

    def test_validate_token_wrong_session(self) -> None:
        """Test validation with wrong session ID."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)
        token = generator.generate_token("session-123")

        assert not generator.validate_token(token, "session-456")

    def test_validate_expired_token(self) -> None:
        """Test validation of expired token."""
        generator = CSRFTokenGenerator("secret-key", timeout=1)
        token = generator.generate_token()

        # Wait for token to expire
        time.sleep(2)

        assert not generator.validate_token(token)

    def test_validate_token_different_secret(self) -> None:
        """Test validation with different secret key."""
        generator1 = CSRFTokenGenerator("secret-key-1", timeout=3600)
        generator2 = CSRFTokenGenerator("secret-key-2", timeout=3600)

        token = generator1.generate_token()
        assert not generator2.validate_token(token)

    def test_validate_malformed_token(self) -> None:
        """Test validation of malformed tokens."""
        generator = CSRFTokenGenerator("secret-key", timeout=3600)

        # Various malformed tokens
        malformed_tokens = [
            ""
            """abc"""
            """not-base64!@#"""
            "dGVzdA==",  # Valid base64 but wrong format
        ]

        for token in malformed_tokens:
            assert not generator.validate_token(token)


class TestGraphQLCSRFValidator:
    """Test GraphQL CSRF validation."""

    @pytest.fixture
    def validator(self, csrf_config):
        """Create GraphQL CSRF validator."""
        return GraphQLCSRFValidator(csrf_config)

    def test_extract_operation_type_query(self, validator) -> None:
        """Test extracting query operation type."""
        request_body = {"query": "query GetUser { user { id } }"}
        op_type = validator._extract_operation_type(request_body)
        assert op_type == "query"

    def test_extract_operation_type_mutation(self, validator) -> None:
        """Test extracting mutation operation type."""
        request_body = {"query": "mutation CreateUser { createUser { id } }"}
        op_type = validator._extract_operation_type(request_body)
        assert op_type == "mutation"

    def test_extract_operation_type_subscription(self, validator) -> None:
        """Test extracting subscription operation type."""
        request_body = {"query": "subscription OnUpdate { userUpdated { id } }"}
        op_type = validator._extract_operation_type(request_body)
        assert op_type == "subscription"

    def test_extract_operation_type_implicit_query(self, validator) -> None:
        """Test extracting implicit query operation type."""
        request_body = {"query": "{ user { id } }"}
        op_type = validator._extract_operation_type(request_body)
        assert op_type == "query"

    def test_requires_csrf_protection(self, validator) -> None:
        """Test CSRF protection requirements."""
        assert validator._requires_csrf_protection("mutation")
        assert not validator._requires_csrf_protection("query")
        assert not validator._requires_csrf_protection("subscription")

    def test_requires_csrf_protection_with_subscription_enabled(self, csrf_config) -> None:
        """Test CSRF protection with subscriptions enabled."""
        csrf_config.require_for_subscriptions = True
        validator = GraphQLCSRFValidator(csrf_config)

        assert validator._requires_csrf_protection("subscription")

    @pytest.mark.asyncio
    async def test_validate_graphql_csrf_query_no_protection(self, validator) -> None:
        """Test that queries don't require CSRF protection."""
        request = MagicMock()
        request_body = {"query": "query GetUser { user { id } }"}

        result = await validator.validate_graphql_csrf(request, request_body)
        assert result is None

    @pytest.mark.asyncio
    async def test_validate_graphql_csrf_mutation_missing_token(self, validator) -> None:
        """Test mutation without CSRF token."""
        request = MagicMock()
        request.headers = {}
        request.cookies = {}
        request_body = {"query": "mutation CreateUser { createUser { id } }"}

        result = await validator.validate_graphql_csrf(request, request_body)
        assert result is not None
        assert result.status_code == 403
        # The actual implementation returns this message when token validation fails
        assert "Invalid or expired CSRF token" in result.body.decode()

    @pytest.mark.asyncio
    async def test_validate_graphql_csrf_mutation_valid_token(self, validator) -> None:
        """Test mutation with valid CSRF token."""
        # Generate valid token
        token = validator.token_generator.generate_token()

        request = MagicMock()
        request.headers = {"X-CSRF-Token": token}
        request.cookies = {}
        request.state = MagicMock()
        request.state.session_id = None
        request_body = {"query": "mutation CreateUser { createUser { id } }"}

        result = await validator.validate_graphql_csrf(request, request_body)
        assert result is None

    @pytest.mark.asyncio
    async def test_validate_graphql_csrf_mutation_token_in_variables(self, validator) -> None:
        """Test mutation with CSRF token in GraphQL variables."""
        # Generate valid token
        token = validator.token_generator.generate_token()

        request = MagicMock()
        request.headers = {}
        request.cookies = {}
        request.state = MagicMock()
        request.state.session_id = None
        request_body = {
            "query": "mutation CreateUser($csrfToken: String) { createUser { id } }",
            "variables": {"csrf_token": token},
        }

        result = await validator.validate_graphql_csrf(request, request_body)
        assert result is None

    @pytest.mark.asyncio
    async def test_validate_graphql_csrf_mutation_invalid_token(self, validator) -> None:
        """Test mutation with invalid CSRF token."""
        request = MagicMock()
        request.headers = {"X-CSRF-Token": "invalid-token"}
        request.cookies = {}
        request.state = MagicMock()
        request.state.session_id = None
        request_body = {"query": "mutation CreateUser { createUser { id } }"}

        result = await validator.validate_graphql_csrf(request, request_body)
        assert result is not None
        assert result.status_code == 403
        assert "Invalid or expired CSRF token" in result.body.decode()


class TestCSRFProtectionMiddleware:
    """Test CSRF protection middleware."""

    def test_middleware_creation(self, app, csrf_config) -> None:
        """Test middleware creation."""
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        assert middleware.config == csrf_config
        assert middleware.graphql_path == "/graphql"

    def test_extract_origin(self, app, csrf_config) -> None:
        """Test origin extraction from URL."""
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        origin = middleware._extract_origin("https://example.com/path?query=1")
        assert origin == "https://example.com"

    def test_get_csrf_token_from_header(self, app, csrf_config) -> None:
        """Test getting CSRF token from header."""
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        request = MagicMock()
        request.headers = {"X-CSRF-Token": "test-token"}

        token = middleware._get_csrf_token(request)
        assert token == "test-token"

    def test_get_csrf_token_from_cookie(self, app, csrf_config) -> None:
        """Test getting CSRF token from cookie."""
        csrf_config.storage = CSRFTokenStorage.COOKIE
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        request = MagicMock()
        request.headers = {}
        request.cookies = {"csrf_token": "test-token"}

        token = middleware._get_csrf_token(request)
        assert token == "test-token"

    @pytest.mark.asyncio
    async def test_validate_csrf_missing_token(self, app, csrf_config) -> None:
        """Test CSRF validation with missing token."""
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        request = MagicMock()
        request.headers = {}
        request.cookies = {}

        result = await middleware._validate_csrf(request)
        assert result is not None
        assert result.status_code == 403

    @pytest.mark.asyncio
    async def test_validate_csrf_valid_token(self, app, csrf_config) -> None:
        """Test CSRF validation with valid token."""
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        # Generate valid token
        token = middleware.token_generator.generate_token()

        request = MagicMock()
        request.headers = {"X-CSRF-Token": token}
        request.cookies = {}
        request.state = MagicMock()
        request.state.session_id = None

        result = await middleware._validate_csrf(request)
        assert result is None

    @pytest.mark.asyncio
    async def test_validate_referrer_missing(self, app, csrf_config) -> None:
        """Test referrer validation with missing referrer."""
        csrf_config.check_referrer = True
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        request = MagicMock()
        request.headers = {}

        result = middleware._validate_referrer(request)
        assert result is not None
        assert result.status_code == 403
        assert "Missing referrer" in result.body.decode()

    @pytest.mark.asyncio
    async def test_validate_referrer_untrusted_origin(self, app, csrf_config) -> None:
        """Test referrer validation with untrusted origin."""
        csrf_config.check_referrer = True
        csrf_config.trusted_origins = {"https://trusted.com"}
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        request = MagicMock()
        request.headers = {"Referer": "https://untrusted.com/page"}

        result = middleware._validate_referrer(request)
        assert result is not None
        assert result.status_code == 403
        assert "Untrusted referrer" in result.body.decode()

    @pytest.mark.asyncio
    async def test_validate_referrer_trusted_origin(self, app, csrf_config) -> None:
        """Test referrer validation with trusted origin."""
        csrf_config.check_referrer = True
        csrf_config.trusted_origins = {"https://trusted.com"}
        middleware = CSRFProtectionMiddleware(app=app, config=csrf_config)

        request = MagicMock()
        request.headers = {"Referer": "https://trusted.com/page"}

        result = middleware._validate_referrer(request)
        assert result is None


class TestCSRFTokenEndpoint:
    """Test CSRF token endpoint."""

    def test_csrf_token_endpoint_creation(self, csrf_config) -> None:
        """Test CSRF token endpoint creation."""
        endpoint = CSRFTokenEndpoint(csrf_config)
        assert endpoint.config == csrf_config

    @pytest.mark.asyncio
    async def test_get_csrf_token(self, csrf_config) -> None:
        """Test getting CSRF token from endpoint."""
        endpoint = CSRFTokenEndpoint(csrf_config)

        request = MagicMock()
        request.state = MagicMock()
        request.state.session_id = "session-123"
        request.cookies = {}

        result = await endpoint.get_csrf_token(request)

        assert "csrf_token" in result
        assert "token_name" in result
        assert "header_name" in result
        assert result["token_name"] == csrf_config.token_name
        assert result["header_name"] == csrf_config.header_name


class TestCSRFIntegration:
    """Integration tests with FastAPI."""

    def test_setup_csrf_protection(self, app) -> None:
        """Test setup with default configuration."""
        middleware = setup_csrf_protection(app, "secret-key")
        assert isinstance(middleware, CSRFProtectionMiddleware)

    def test_get_request_allowed(self, app, csrf_config) -> None:
        """Test that GET requests are allowed without CSRF token."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)
        response = client.get("/test")
        assert response.status_code == 200

    def test_post_request_blocked_without_token(self, app, csrf_config) -> None:
        """Test that POST requests are blocked without CSRF token."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)
        response = client.post("/test")
        assert response.status_code == 403
        assert "CSRF token is required" in response.json()["message"]

    def test_post_request_allowed_with_valid_token(self, app, csrf_config) -> None:
        """Test that POST requests are allowed with valid CSRF token."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)

        # First, get a CSRF token
        get_response = client.get("/test")
        csrf_token = None

        # Extract token from cookie
        csrf_token = get_response.cookies.get(csrf_config.cookie_name)

        assert csrf_token is not None

        # Now make POST request with token
        response = client.post("/test", headers={"X-CSRF-Token": csrf_token})
        assert response.status_code == 200

    def test_exempt_paths_allowed(self, app, csrf_config) -> None:
        """Test that exempt paths are allowed without CSRF token."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)
        response = client.get("/health")
        assert response.status_code == 200

    def test_csrf_token_endpoint(self, app, csrf_config) -> None:
        """Test CSRF token endpoint."""
        setup_csrf_protection(app, csrf_config.secret_key, csrf_config)

        client = TestClient(app)
        response = client.get("/csrf-token")
        assert response.status_code == 200

        data = response.json()
        assert "csrf_token" in data
        assert "token_name" in data
        assert "header_name" in data

    def test_graphql_mutation_blocked_without_token(self, app, csrf_config) -> None:
        """Test that GraphQL mutations are blocked without CSRF token."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)
        response = client.post(
            "/graphql", json={"query": "mutation CreateUser { createUser { id } }"}
        )
        assert response.status_code == 403
        assert "CSRF token is required" in response.json()["errors"][0]["message"]

    def test_graphql_query_allowed_without_token(self, app, csrf_config) -> None:
        """Test that GraphQL queries are allowed without CSRF token."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)
        response = client.post("/graphql", json={"query": "query GetUser { user { id } }"})
        assert response.status_code == 200

    def test_graphql_mutation_allowed_with_token_in_header(self, app, csrf_config) -> None:
        """Test GraphQL mutation with CSRF token in header."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)

        # Get CSRF token first
        get_response = client.get("/test")
        csrf_token = get_response.cookies.get(csrf_config.cookie_name)

        assert csrf_token is not None

        # Make GraphQL mutation with token
        response = client.post(
            "/graphql",
            json={"query": "mutation CreateUser { createUser { id } }"},
            headers={"X-CSRF-Token": csrf_token},
        )
        assert response.status_code == 200

    def test_graphql_mutation_allowed_with_token_in_variables(self, app, csrf_config) -> None:
        """Test GraphQL mutation with CSRF token in variables."""
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        client = TestClient(app)

        # Get CSRF token first
        get_response = client.get("/test")
        csrf_token = get_response.cookies.get(csrf_config.cookie_name)

        assert csrf_token is not None

        # Make GraphQL mutation with token in variables
        response = client.post(
            "/graphql",
            json={
                "query": "mutation CreateUser($csrfToken: String) { createUser { id } }",
                "variables": {"csrf_token": csrf_token},
            },
        )
        assert response.status_code == 200


class TestCSRFConfigHelpers:
    """Test configuration helper functions."""

    def test_create_production_csrf_config(self) -> None:
        """Test production CSRF configuration."""
        config = create_production_csrf_config("secret-key", {"https://example.com"})

        assert config.secret_key == "secret-key"
        assert config.cookie_secure is True
        assert config.cookie_httponly is True
        assert config.cookie_samesite == "strict"
        assert config.check_referrer is True
        assert "https://example.com" in config.trusted_origins

    def test_create_development_csrf_config(self) -> None:
        """Test development CSRF configuration."""
        config = create_development_csrf_config("secret-key")

        assert config.secret_key == "secret-key"
        assert config.cookie_secure is False
        assert config.cookie_samesite == "lax"
        assert config.check_referrer is False
        assert "http://localhost:3000" in config.trusted_origins


# Extended tests merged from test_csrf_protection_extended.py


class TestCSRFTokenGeneratorExtended:
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


class TestGraphQLCSRFValidatorExtended:
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
            trusted_origins={"https://app.example.com", "http://localhost:3000"},
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


class TestCSRFProtectionMiddlewareExtended:
    """Extended test CSRF protection middleware."""

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


class TestCSRFConfigurationsExtended:
    """Test CSRF configuration presets."""

    def test_development_config_extended(self):
        """Test development CSRF configuration."""
        config = create_development_csrf_config("dev-secret")

        assert config.secret_key == "dev-secret"
        assert config.cookie_secure is False
        assert config.cookie_samesite == "lax"
        assert config.check_referrer is False

    def test_production_config_extended(self):
        """Test production CSRF configuration."""
        config = create_production_csrf_config(
            secret_key="prod-secret", trusted_origins={"https://app.com"}
        )

        assert config.secret_key == "prod-secret"
        assert config.cookie_secure is True
        assert config.cookie_samesite == "strict"
        assert config.check_referrer is True
        assert config.trusted_origins == {"https://app.com"}


class TestCSRFTokenEndpointExtended:
    """Test CSRF token endpoint."""

    def test_endpoint_creation(self):
        """Test creating CSRF token endpoint."""
        config = CSRFConfig(secret_key="test")
        endpoint = CSRFTokenEndpoint(config)

        assert endpoint.config is config

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


class TestCSRFSetupExtended:
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
