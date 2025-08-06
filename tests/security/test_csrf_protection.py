"""Tests for CSRF protection middleware."""

import time
from unittest.mock import MagicMock

import pytest
from fastapi import FastAPI, Request
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
