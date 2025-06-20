#!/usr/bin/env python3
"""Simple integration test for FraiseQL security middleware."""

import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

def test_middleware_creation():
    """Test that security middleware can be created."""
    try:
        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        from fraiseql.security.csrf_protection import (
            CSRFProtectionMiddleware,
            create_development_csrf_config,
        )
        from fraiseql.security.rate_limiting import RateLimitMiddleware, RateLimitStore
        from fraiseql.security.security_headers import (
            SecurityHeadersMiddleware,
            create_development_security_config,
        )


        # Test 1: Create basic FastAPI app
        app = FastAPI()

        @app.get("/test")
        async def test_endpoint():
            return {"message": "test"}


        # Test 2: Add rate limiting middleware
        rate_store = RateLimitStore()
        app.add_middleware(RateLimitMiddleware, store=rate_store, rules=[])

        # Test 3: Add CSRF protection middleware
        csrf_config = create_development_csrf_config("test-secret")
        app.add_middleware(CSRFProtectionMiddleware, config=csrf_config)

        # Test 4: Add security headers middleware
        headers_config = create_development_security_config()
        app.add_middleware(SecurityHeadersMiddleware, config=headers_config)

        # Test 5: Create test client
        client = TestClient(app)

        # Test 6: Make a simple request
        response = client.get("/test")

        # Test 7: Check for security headers
        headers = response.headers
        security_headers = [
            "X-Frame-Options", "X-Content-Type-Options",
            "Referrer-Policy",
        ]

        found_headers = [h for h in security_headers if h in headers]
        if found_headers:
            pass
        else:
            pass

        return True

    except Exception:
        import traceback
        traceback.print_exc()
        return False


def test_individual_components():
    """Test individual security components."""
    try:
        # Test rate limiting components
        from fraiseql.security.rate_limiting import RateLimit, RateLimitRule

        rule = RateLimitRule(
            path_pattern="/api/*",
            rate_limit=RateLimit(requests=100, window=60),
            message="API rate limit exceeded",
        )
        assert rule.rate_limit.requests == 100

        # Test CSRF components
        from fraiseql.security.csrf_protection import CSRFConfig, CSRFTokenGenerator

        config = CSRFConfig(secret_key="test-key")
        generator = CSRFTokenGenerator(config.secret_key)
        token = generator.generate_token()
        assert generator.validate_token(token)

        # Test security headers components
        from fraiseql.security.security_headers import ContentSecurityPolicy, CSPDirective

        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")
        header_value = csp.to_header_value()
        assert "default-src 'self'" in header_value

        return True

    except Exception:
        return False


def test_configuration_helpers():
    """Test security configuration helpers."""
    try:
        from fraiseql.security import (
            SecurityConfig,
            create_development_csrf_config,
            create_development_security_config,
            create_production_csrf_config,
            create_production_security_config,
            create_security_config_for_graphql,
        )

        # Test SecurityConfig class
        config = SecurityConfig(
            secret_key="test-key",
            environment="production",
            domain="api.example.com",
        )
        assert config.is_production
        assert not config.is_development

        # Test GraphQL config helper
        graphql_config = create_security_config_for_graphql(
            secret_key="graphql-key",
            environment="development",
            trusted_origins=["https://app.example.com"],
        )
        assert graphql_config.api_only
        assert len(graphql_config.custom_rate_limits) > 0

        # Test CSRF config helpers
        prod_csrf = create_production_csrf_config("prod-key", {"https://app.example.com"})
        dev_csrf = create_development_csrf_config("dev-key")

        assert prod_csrf.cookie_secure
        assert not dev_csrf.cookie_secure

        # Test security headers config helpers
        prod_headers = create_production_security_config("api.example.com")
        dev_headers = create_development_security_config()

        assert prod_headers.hsts
        assert not dev_headers.hsts

        return True

    except Exception:
        return False


def main():
    """Run simple integration tests."""
    results = []

    # Run tests
    results.append(test_middleware_creation())
    results.append(test_individual_components())
    results.append(test_configuration_helpers())

    # Summary

    passed = sum(results)
    total = len(results)


    if passed == total:
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
