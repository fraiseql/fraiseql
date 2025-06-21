#!/usr/bin/env python3
"""Standalone test runner for FraiseQL security modules.
This avoids the complex project dependencies and conftest issues.
"""

import asyncio
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent / "src"))

def run_rate_limiting_tests():
    """Test rate limiting functionality."""
    try:
        from fraiseql.security.rate_limiting import (
            GraphQLRateLimiter,
            RateLimit,
            RateLimitStore,
        )

        # Test 1: Rate limit store

        store = RateLimitStore()

        # Test basic functionality
        async def test_store():
            # Test getting non-existent key
            timestamp, count = await store.get("test")
            assert timestamp == 0.0
            assert count == 0

            # Test increment
            timestamp1, count1 = await store.increment("test", 60)
            assert count1 == 1

            # Test increment again
            timestamp2, count2 = await store.increment("test", 60)
            assert count2 == 2

        asyncio.run(test_store())

        # Test 2: Rate limit configuration
        rate_limit = RateLimit(requests=100, window=60)
        assert rate_limit.requests == 100
        assert rate_limit.window == 60

        # Test 3: GraphQL rate limiter
        limiter = GraphQLRateLimiter(store)

        # Test operation type extraction
        query_body = {"query": "query GetUser { user { id } }"}
        op_type, op_name, complexity = limiter._extract_operation_info(query_body)
        assert op_type == "query"

        mutation_body = {"query": "mutation CreateUser { createUser { id } }"}
        op_type, op_name, complexity = limiter._extract_operation_info(mutation_body)
        assert op_type == "mutation"

        # Test complexity estimation
        simple_query = "{ user { id } }"
        complex_query = "{ users { posts { comments { author { name } } } } }"

        simple_complexity = limiter._estimate_complexity(simple_query)
        complex_complexity = limiter._estimate_complexity(complex_query)
        assert complex_complexity > simple_complexity

        return True  # noqa: TRY300

    except Exception:
        return False


def run_csrf_protection_tests():
    """Test CSRF protection functionality."""
    try:
        from fraiseql.security.csrf_protection import (
            CSRFConfig,
            CSRFTokenGenerator,
            GraphQLCSRFValidator,
        )


        # Test 1: Token generation and validation
        generator = CSRFTokenGenerator("test-secret-key", timeout=3600)

        token = generator.generate_token()
        assert isinstance(token, str)
        assert len(token) > 0

        # Validate the token
        is_valid = generator.validate_token(token)
        assert is_valid

        # Test invalid token
        is_invalid = generator.validate_token("invalid-token")
        assert not is_invalid

        # Test 2: Token with session ID
        session_id = "session-123"
        session_token = generator.generate_token(session_id)

        # Validate with correct session
        is_valid_session = generator.validate_token(session_token, session_id)
        assert is_valid_session

        # Validate with wrong session
        is_invalid_session = generator.validate_token(session_token, "wrong-session")
        assert not is_invalid_session

        # Test 3: Configuration
        config = CSRFConfig(
            secret_key="test-key",
            token_timeout=1800,
            require_for_mutations=True,
        )
        assert config.secret_key == "test-key"
        assert config.token_timeout == 1800
        assert config.require_for_mutations is True

        # Test 4: GraphQL CSRF validator
        validator = GraphQLCSRFValidator(config)

        # Test operation type extraction
        query_body = {"query": "query GetUser { user { id } }"}
        op_type = validator._extract_operation_type(query_body)
        assert op_type == "query"

        mutation_body = {"query": "mutation CreateUser { createUser { id } }"}
        op_type = validator._extract_operation_type(mutation_body)
        assert op_type == "mutation"

        # Test protection requirements
        assert validator._requires_csrf_protection("mutation")
        assert not validator._requires_csrf_protection("query")

        return True  # noqa: TRY300

    except Exception:
        return False


def run_security_headers_tests():
    """Test security headers functionality."""
    try:
        from fraiseql.security.security_headers import (
            ContentSecurityPolicy,
            CSPDirective,
            FrameOptions,
            SecurityHeadersConfig,
            create_development_csp,
            create_production_security_config,
            create_strict_csp,
        )


        # Test 1: Content Security Policy
        csp = ContentSecurityPolicy()
        csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")
        csp.add_directive(CSPDirective.SCRIPT_SRC, ["'self'", "'unsafe-inline'"])

        header_value = csp.to_header_value()
        assert "default-src 'self'" in header_value
        assert "script-src 'self' 'unsafe-inline'" in header_value

        # Test header name
        assert csp.get_header_name() == "Content-Security-Policy"

        csp_report_only = ContentSecurityPolicy(report_only=True)
        assert csp_report_only.get_header_name() == "Content-Security-Policy-Report-Only"

        # Test 2: Security headers configuration
        config = SecurityHeadersConfig(
            frame_options=FrameOptions.DENY,
            content_type_options=True,
            hsts=True,
            hsts_max_age=86400,
        )

        assert config.frame_options == FrameOptions.DENY
        assert config.content_type_options is True
        assert config.hsts_max_age == 86400

        # Test 3: Predefined CSP configurations
        strict_csp = create_strict_csp()
        assert CSPDirective.DEFAULT_SRC in strict_csp.directives
        assert strict_csp.directives[CSPDirective.DEFAULT_SRC] == ["'self'"]

        dev_csp = create_development_csp()
        assert "'unsafe-inline'" in dev_csp.directives[CSPDirective.DEFAULT_SRC]

        # Test 4: Production configuration
        prod_config = create_production_security_config("example.com")
        assert prod_config.hsts is True
        assert prod_config.hsts_include_subdomains is True
        assert prod_config.csp is not None

        return True  # noqa: TRY300

    except Exception:
        return False


def run_integration_tests():
    """Test integrated security setup."""
    try:
        from fastapi import FastAPI

        from fraiseql.security import (
            SecurityConfig,
            create_security_config_for_graphql,
            setup_development_security,
            setup_production_security,
            setup_security,
        )


        # Test 1: Security configuration
        config = SecurityConfig(
            secret_key="test-secret-key",
            environment="production",
            domain="api.example.com",
            trusted_origins={"https://app.example.com"},
            api_only=True,
        )

        assert config.secret_key == "test-secret-key"
        assert config.is_production
        assert not config.is_development
        assert config.api_only

        # Test 2: GraphQL-specific configuration
        graphql_config = create_security_config_for_graphql(
            secret_key="graphql-secret",
            environment="development",
            trusted_origins=["https://app.example.com"],
            enable_introspection=True,
        )

        assert graphql_config.secret_key == "graphql-secret"
        assert graphql_config.is_development
        assert graphql_config.api_only
        assert len(graphql_config.custom_rate_limits) > 0

        # Test 3: FastAPI integration (without actually adding middleware)
        FastAPI()

        # Test that setup functions exist and can be called
        # (We can't fully test middleware without running the app)
        assert callable(setup_security)
        assert callable(setup_production_security)
        assert callable(setup_development_security)

        return True  # noqa: TRY300

    except Exception:
        return False


def main():
    """Run all security tests."""
    results = []

    # Run individual module tests
    results.append(run_rate_limiting_tests())
    results.append(run_csrf_protection_tests())
    results.append(run_security_headers_tests())
    results.append(run_integration_tests())

    # Summary

    passed = sum(results)
    total = len(results)


    if passed == total:
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
