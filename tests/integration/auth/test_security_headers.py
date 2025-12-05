"""Tests for security headers middleware."""

import pytest
from fastapi import FastAPI, Request
from fastapi.testclient import TestClient

from fraiseql.security.security_headers import (
    ContentSecurityPolicy,
    CSPDirective,
    FrameOptions,
    ReferrerPolicy,
    SecurityHeadersConfig,
    SecurityHeadersMiddleware,
    create_api_csp,
    create_development_csp,
    create_development_security_config,
    create_graphql_security_config,
    create_production_security_config,
    create_strict_csp,
    setup_security_headers,
)

pytestmark = pytest.mark.integration


@pytest.fixture
def app() -> None:
    """Create test FastAPI app."""
    app = FastAPI()

    @app.get("/test")
    @pytest.mark.asyncio
    async def test_endpoint() -> None:
        return {"message": "success"}

    @app.post("/test")
    @pytest.mark.asyncio
    async def test_post() -> None:
        return {"message": "success"}

    @app.get("/docs")
    async def docs() -> None:
        return {"docs": "swagger"}

    @app.post("/graphql")
    async def graphql() -> None:
        return {"data": {"test": "success"}}

    return app


class TestContentSecurityPolicy:
    """Test Content Security Policy functionality."""

    def test_create_empty_csp(self) -> None:
        """Test creating empty CSP."""
        csp = ContentSecurityPolicy()
        assert len(csp.directives) == 0
        assert not csp.report_only

    def test_add_directive_single_source(self) -> None:
        """Test adding directive with single source."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")

        assert CSPDirective.DEFAULT_SRC in csp.directives
        assert csp.directives[CSPDirective.DEFAULT_SRC] == ["'self'"]

    def test_add_directive_multiple_sources(self) -> None:
        """Test adding directive with multiple sources."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.SCRIPT_SRC, ["'self'", "'unsafe-inline'"])

        assert CSPDirective.SCRIPT_SRC in csp.directives
        assert csp.directives[CSPDirective.SCRIPT_SRC] == ["'self'", "'unsafe-inline'"]

    def test_add_directive_append_sources(self) -> None:
        """Test appending sources to existing directive."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.SCRIPT_SRC, "'self'")
        csp.add_directive(CSPDirective.SCRIPT_SRC, "'unsafe-inline'")

        assert csp.directives[CSPDirective.SCRIPT_SRC] == ["'self'", "'unsafe-inline'"]

    def test_remove_directive(self) -> None:
        """Test removing directive."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")
        csp.remove_directive(CSPDirective.DEFAULT_SRC)

        assert CSPDirective.DEFAULT_SRC not in csp.directives

    def test_to_header_value_simple(self) -> None:
        """Test converting CSP to header value."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")
        csp.add_directive(CSPDirective.SCRIPT_SRC, ["'self'", "'unsafe-inline'"])

        header_value = csp.to_header_value()
        assert "default-src 'self'" in header_value
        assert "script-src 'self' 'unsafe-inline'" in header_value
        assert ";" in header_value

    def test_to_header_value_no_sources(self) -> None:
        """Test converting CSP with no-source directives."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.UPGRADE_INSECURE_REQUESTS, [])
        csp.add_directive(CSPDirective.BLOCK_ALL_MIXED_CONTENT, [])

        header_value = csp.to_header_value()
        assert "upgrade-insecure-requests" in header_value
        assert "block-all-mixed-content" in header_value
        # These directives shouldn't have sources
        assert "upgrade-insecure-requests ;" not in header_value

    def test_get_header_name_enforce(self) -> None:
        """Test header name for enforcing CSP."""
        csp = ContentSecurityPolicy(report_only=False)
        assert csp.get_header_name() == "Content-Security-Policy"

    def test_get_header_name_report_only(self) -> None:
        """Test header name for report-only CSP."""
        csp = ContentSecurityPolicy(report_only=True)
        assert csp.get_header_name() == "Content-Security-Policy-Report-Only"


class TestSecurityHeadersConfig:
    """Test security headers configuration."""

    def test_default_config(self) -> None:
        """Test default configuration values."""
        config = SecurityHeadersConfig()

        assert config.csp is None
        assert config.frame_options == FrameOptions.SAMEORIGIN
        assert config.content_type_options is True
        assert config.xss_protection is True
        assert config.hsts is True
        assert config.referrer_policy == ReferrerPolicy.STRICT_ORIGIN_WHEN_CROSS_ORIGIN
        assert len(config.exclude_paths) > 0

    def test_custom_config(self) -> None:
        """Test custom configuration."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")

        config = SecurityHeadersConfig(
            csp=csp,
            frame_options=FrameOptions.DENY,
            hsts=False,
            custom_headers={"X-Custom": "value"},
        )

        assert config.csp == csp
        assert config.frame_options == FrameOptions.DENY
        assert config.hsts is False
        assert config.custom_headers["X-Custom"] == "value"


class TestSecurityHeadersMiddleware:
    """Test security headers middleware."""

    def test_middleware_creation(self, app) -> None:
        """Test middleware creation."""
        config = SecurityHeadersConfig()
        middleware = SecurityHeadersMiddleware(app=app, config=config)

        assert middleware.config == config

    def test_exclude_paths(self, app) -> None:
        """Test that excluded paths don't get headers."""
        config = SecurityHeadersConfig(exclude_paths={"/docs"})
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/docs")

        # Should not have security headers
        assert "X-Frame-Options" not in response.headers
        assert "X-Content-Type-Options" not in response.headers

    def test_basic_security_headers(self, app) -> None:
        """Test basic security headers are added."""
        config = SecurityHeadersConfig()
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert response.headers["X-Frame-Options"] == "SAMEORIGIN"
        assert response.headers["X-Content-Type-Options"] == "nosniff"
        assert response.headers["X-XSS-Protection"] == "1; mode=block"
        assert "Strict-Transport-Security" in response.headers
        assert response.headers["Referrer-Policy"] == "strict-origin-when-cross-origin"

    def test_csp_header(self, app) -> None:
        """Test CSP header is added."""
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")

        config = SecurityHeadersConfig(csp=csp)
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert "Content-Security-Policy" in response.headers
        assert "default-src 'self'" in response.headers["Content-Security-Policy"]

    def test_csp_report_only(self, app) -> None:
        """Test CSP report-only header."""
        csp = ContentSecurityPolicy(report_only=True)
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")

        config = SecurityHeadersConfig(csp=csp)
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert "Content-Security-Policy-Report-Only" in response.headers
        assert "Content-Security-Policy" not in response.headers

    def test_frame_options_allow_from(self, app) -> None:
        """Test X-Frame-Options with ALLOW-FROM."""
        config = SecurityHeadersConfig(
            frame_options=FrameOptions.ALLOW_FROM, frame_options_allow_from="https://trusted.com"
        )
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert response.headers["X-Frame-Options"] == "ALLOW-FROM https://trusted.com"

    def test_hsts_with_subdomains_and_preload(self, app) -> None:
        """Test HSTS with subdomains and preload."""
        config = SecurityHeadersConfig(
            hsts=True, hsts_max_age=86400, hsts_include_subdomains=True, hsts_preload=True
        )
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        hsts_header = response.headers["Strict-Transport-Security"]
        assert "max-age=86400" in hsts_header
        assert "includeSubDomains" in hsts_header
        assert "preload" in hsts_header

    def test_permissions_policy(self, app) -> None:
        """Test Permissions-Policy header."""
        config = SecurityHeadersConfig(
            permissions_policy={
                "camera": ["'self'", "https://trusted.com"],
                "microphone": [],
                "geolocation": ["*"],
            }
        )
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        permissions_header = response.headers["Permissions-Policy"]
        assert 'camera=("\'self\'" "https://trusted.com")' in permissions_header
        assert "microphone=()" in permissions_header
        assert 'geolocation=("*")' in permissions_header

    def test_cross_origin_policies(self, app) -> None:
        """Test Cross-Origin policies."""
        config = SecurityHeadersConfig(
            cross_origin_embedder_policy="require-corp",
            cross_origin_opener_policy="same-origin",
            cross_origin_resource_policy="same-site",
        )
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert response.headers["Cross-Origin-Embedder-Policy"] == "require-corp"
        assert response.headers["Cross-Origin-Opener-Policy"] == "same-origin"
        assert response.headers["Cross-Origin-Resource-Policy"] == "same-site"

    def test_custom_headers(self, app) -> None:
        """Test custom headers."""
        config = SecurityHeadersConfig(
            custom_headers={"X-Custom-Header": "custom-value", "X-Another-Header": "another-value"}
        )
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert response.headers["X-Custom-Header"] == "custom-value"
        assert response.headers["X-Another-Header"] == "another-value"

    def test_conditional_headers(self, app) -> None:
        """Test conditional headers based on request."""

        def add_api_headers(request: Request) -> dict:
            if request.url.path.startswith("/api"):
                return {"X-API-Version": "v1"}
            return {}

        config = SecurityHeadersConfig(conditional_headers=[add_api_headers])
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)

        # Regular endpoint shouldn't have API header
        response1 = client.get("/test")
        assert "X-API-Version" not in response1.headers

        # Add API endpoint for testing
        @app.get("/api/test")
        async def api_test() -> None:
            return {"api": "test"}

        # API endpoint should have API header
        response2 = client.get("/api/test")
        assert response2.headers["X-API-Version"] == "v1"

    def test_disabled_headers(self, app) -> None:
        """Test disabling specific headers."""
        config = SecurityHeadersConfig(
            frame_options=None,
            content_type_options=False,
            xss_protection=False,
            hsts=False,
            referrer_policy=None,
        )
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        assert "X-Frame-Options" not in response.headers
        assert "X-Content-Type-Options" not in response.headers
        assert "X-XSS-Protection" not in response.headers
        assert "Strict-Transport-Security" not in response.headers
        assert "Referrer-Policy" not in response.headers


class TestCSPPresets:
    """Test predefined CSP configurations."""

    def test_create_strict_csp(self) -> None:
        """Test strict CSP creation."""
        csp = create_strict_csp()

        assert CSPDirective.DEFAULT_SRC in csp.directives
        assert csp.directives[CSPDirective.DEFAULT_SRC] == ["'self'"]
        assert CSPDirective.OBJECT_SRC in csp.directives
        assert csp.directives[CSPDirective.OBJECT_SRC] == ["'none'"]
        assert CSPDirective.UPGRADE_INSECURE_REQUESTS in csp.directives

    def test_create_development_csp(self) -> None:
        """Test development CSP creation."""
        csp = create_development_csp()

        # Should be more permissive
        assert "'unsafe-inline'" in csp.directives[CSPDirective.DEFAULT_SRC]
        assert "'unsafe-eval'" in csp.directives[CSPDirective.DEFAULT_SRC]
        assert "http://localhost:*" in csp.directives[CSPDirective.CONNECT_SRC]

    def test_create_api_csp(self) -> None:
        """Test API-only CSP creation."""
        csp = create_api_csp()

        # Should be very restrictive
        assert csp.directives[CSPDirective.DEFAULT_SRC] == ["'none'"]
        assert csp.directives[CSPDirective.FRAME_ANCESTORS] == ["'none'"]
        assert csp.directives[CSPDirective.FORM_ACTION] == ["'none'"]


class TestConfigPresets:
    """Test predefined configuration functions."""

    def test_create_production_security_config(self) -> None:
        """Test production configuration."""
        config = create_production_security_config("example.com", api_only=False)

        assert config.csp is not None
        assert config.frame_options == FrameOptions.SAMEORIGIN
        assert config.hsts is True
        assert config.hsts_include_subdomains is True
        assert config.cross_origin_embedder_policy == "require-corp"
        assert len(config.permissions_policy) > 0

    def test_create_production_security_config_api_only(self) -> None:
        """Test production configuration for API-only."""
        config = create_production_security_config("api.example.com", api_only=True)

        assert config.frame_options == FrameOptions.DENY
        assert config.csp.directives[CSPDirective.DEFAULT_SRC] == ["'none'"]

    def test_create_production_security_config_with_preload(self) -> None:
        """Test production configuration with HSTS preload."""
        config = create_production_security_config("example.com", enable_hsts_preload=True)

        assert config.hsts_preload is True

    def test_create_development_security_config(self) -> None:
        """Test development configuration."""
        config = create_development_security_config()

        assert config.csp is not None
        assert config.hsts is False  # No HSTS in development
        assert config.referrer_policy == ReferrerPolicy.NO_REFERRER_WHEN_DOWNGRADE
        assert "/graphql" in config.exclude_paths

    def test_create_graphql_security_config(self) -> None:
        """Test GraphQL-specific configuration."""
        trusted_origins = ["https://app.example.com", "https://admin.example.com"]
        config = create_graphql_security_config(
            trusted_origins=trusted_origins, enable_introspection=True
        )

        assert config.frame_options == FrameOptions.DENY
        assert config.cross_origin_resource_policy == "cross-origin"
        assert "/graphql" in config.exclude_paths

        # Should allow connections from trusted origins
        connect_src = config.csp.directives[CSPDirective.CONNECT_SRC]
        for origin in trusted_origins:
            assert origin in connect_src

    def test_create_graphql_security_config_no_introspection(self) -> None:
        """Test GraphQL config without introspection."""
        config = create_graphql_security_config(
            trusted_origins=["https://app.example.com"], enable_introspection=False
        )

        # Should not have script/style sources for GraphQL UI
        assert CSPDirective.SCRIPT_SRC not in config.csp.directives
        assert "/graphql" not in config.exclude_paths


class TestSetupFunction:
    """Test convenience setup function."""

    def test_setup_security_headers_default(self, app) -> None:
        """Test setup with default configuration."""
        middleware = setup_security_headers(app)
        assert isinstance(middleware, SecurityHeadersMiddleware)

    def test_setup_security_headers_development(self, app) -> None:
        """Test setup for development."""
        middleware = setup_security_headers(app, environment="development")
        assert middleware.config.hsts is False

    def test_setup_security_headers_production(self, app) -> None:
        """Test setup for production."""
        middleware = setup_security_headers(app, environment="production")
        assert middleware.config.hsts is True

    def test_setup_security_headers_custom_config(self, app) -> None:
        """Test setup with custom configuration."""
        custom_config = SecurityHeadersConfig(hsts=False)
        middleware = setup_security_headers(app, config=custom_config)
        assert middleware.config.hsts is False


class TestIntegration:
    """Integration tests."""

    def test_all_headers_together(self, app) -> None:
        """Test that all headers work together."""
        csp = create_strict_csp()
        config = SecurityHeadersConfig(
            csp=csp,
            frame_options=FrameOptions.DENY,
            permissions_policy={"camera": []},
            custom_headers={"X-Custom": "test"},
            cross_origin_embedder_policy="require-corp",
        )

        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.get("/test")

        # Check that multiple headers are present
        expected_headers = [
            "Content-Security-Policy",
            "X-Frame-Options",
            "X-Content-Type-Options",
            "X-XSS-Protection",
            "Strict-Transport-Security",
            "Referrer-Policy",
            "Permissions-Policy",
            "X-Custom",
            "Cross-Origin-Embedder-Policy",
        ]

        for header in expected_headers:
            assert header in response.headers

    def test_graphql_with_security_headers(self, app) -> None:
        """Test GraphQL endpoint with security headers."""
        config = create_graphql_security_config(
            trusted_origins=["https://app.example.com"], enable_introspection=True
        )

        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)

        # GraphQL endpoint should be excluded
        graphql_response = client.post("/graphql")
        assert "X-Frame-Options" not in graphql_response.headers

        # Other endpoints should have headers
        test_response = client.get("/test")
        assert "X-Frame-Options" in test_response.headers

    def test_post_request_headers(self, app) -> None:
        """Test that headers are added to POST responses."""
        config = SecurityHeadersConfig()
        app.add_middleware(SecurityHeadersMiddleware, config=config)

        client = TestClient(app)
        response = client.post("/test")

        assert response.headers["X-Frame-Options"] == "SAMEORIGIN"
        assert "Content-Type" in response.headers  # FastAPI default
        assert "X-Content-Type-Options" in response.headers  # Our security header
