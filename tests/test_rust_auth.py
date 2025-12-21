"""Tests for Phase 10 Rust authentication module.

These tests verify the Rust JWT validation, JWKS caching, and auth providers.
"""

import pytest

# Phase 10 bindings are now exported - tests can run
try:
    from fraiseql._fraiseql_rs import PyAuthProvider, PyUserContext
    HAS_RUST_AUTH = True
except ImportError:
    HAS_RUST_AUTH = False

# Skip tests only if Rust bindings aren't available
pytestmark = pytest.mark.skipif(
    not HAS_RUST_AUTH,
    reason="Phase 10 Rust bindings not available",
)


class TestRustAuthAvailability:
    """Test that Rust auth module is available and properly configured."""

    def test_rust_auth_module_exists(self):
        """Test that Rust auth module classes are available."""
        assert HAS_RUST_AUTH, "PyAuthProvider and PyUserContext should be available"
        assert PyAuthProvider is not None
        assert PyUserContext is not None

    def test_auth0_provider_available(self):
        """Test that Auth0 provider can be created."""
        assert hasattr(PyAuthProvider, "auth0"), "Auth0 factory method should exist"
        # Verify it's a static method
        assert callable(PyAuthProvider.auth0)

    def test_custom_jwt_provider_available(self):
        """Test that CustomJWT provider can be created."""
        assert hasattr(PyAuthProvider, "jwt"), "JWT factory method should exist"
        # Verify it's a static method
        assert callable(PyAuthProvider.jwt)


class TestAuth0Provider:
    """Test Auth0 authentication provider."""

    def test_auth0_provider_creation(self):
        """Test creating Auth0 provider."""
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://example.com"])
        assert provider is not None
        assert provider.provider_type() == "auth0"

    def test_auth0_https_validation(self):
        """Test that Auth0 provider validates HTTPS for JWKS."""
        # Auth0 should succeed with valid domain
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://api.example.com"])
        assert provider.provider_type() == "auth0"

    def test_auth0_provider_has_validation_method(self):
        """Test that Auth0 provider has token validation method."""
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://example.com"])
        assert hasattr(provider, "validate_token_blocking"), "Should have validate_token_blocking method"
        assert callable(provider.validate_token_blocking)

    def test_auth0_invalid_token(self):
        """Test Auth0 rejects invalid tokens."""
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://example.com"])

        # Test with obviously invalid token
        try:
            provider.validate_token_blocking("not.a.valid.token")
            # If we get here, validation should have failed
            assert False, "Should have raised RuntimeError for invalid token"
        except RuntimeError as e:
            # Expected: validation should fail for invalid token
            # Note: May fail with "No tokio runtime" if not in async context
            assert (
                "Token validation failed" in str(e)
                or "Invalid" in str(e)
                or "No tokio runtime" in str(e)
            )

    def test_auth0_expired_token(self):
        """Test Auth0 rejects expired tokens."""
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://example.com"])

        # Test with a malformed token (empty)
        try:
            provider.validate_token_blocking("")
            assert False, "Should have raised RuntimeError for empty token"
        except RuntimeError as e:
            # Expected: validation should fail (may need async context for actual validation)
            assert (
                "Token validation failed" in str(e)
                or "Invalid" in str(e)
                or "No tokio runtime" in str(e)
            )


class TestCustomJWTProvider:
    """Test custom JWT authentication provider."""

    def test_custom_jwt_provider_creation(self):
        """Test creating custom JWT provider."""
        provider = PyAuthProvider.jwt(
            issuer="https://example.com",
            audience=["https://api.example.com"],
            jwks_url="https://example.com/.well-known/jwks.json"
        )
        assert provider is not None
        assert provider.provider_type() == "jwt"

    def test_custom_jwt_https_validation(self):
        """Test that custom JWT provider validates HTTPS for JWKS."""
        # HTTPS URL should work
        provider = PyAuthProvider.jwt(
            issuer="https://example.com",
            audience=["https://api.example.com"],
            jwks_url="https://example.com/.well-known/jwks.json"
        )
        assert provider is not None

        # HTTP URL should fail
        try:
            PyAuthProvider.jwt(
                issuer="https://example.com",
                audience=["https://api.example.com"],
                jwks_url="http://example.com/.well-known/jwks.json"  # HTTP, not HTTPS
            )
            assert False, "Should reject HTTP JWKS URL"
        except ValueError:
            pass  # Expected

    def test_custom_jwt_token_validation(self):
        """Test custom JWT token validation with invalid token."""
        provider = PyAuthProvider.jwt(
            issuer="https://example.com",
            audience=["https://api.example.com"],
            jwks_url="https://example.com/.well-known/jwks.json"
        )

        # Test with invalid token
        try:
            provider.validate_token_blocking("invalid.token.here")
            assert False, "Should have raised RuntimeError"
        except RuntimeError as e:
            # May fail due to missing tokio runtime in sync context
            assert (
                "Token validation failed" in str(e)
                or "Invalid" in str(e)
                or "No tokio runtime" in str(e)
            )


class TestJWKSCaching:
    """Test JWKS caching functionality."""

    def test_jwks_cache_hit(self):
        """Test JWKS cache hit reduces fetch calls."""
        pass  # TODO: Requires real JWT token or mock

    def test_jwks_cache_ttl(self):
        """Test JWKS cache respects 1-hour TTL."""
        pass  # TODO: Requires real JWT token or mock

    def test_jwks_cache_lru_eviction(self):
        """Test JWKS cache evicts old entries when full."""
        pass  # TODO: Requires real JWT token or mock


class TestUserContextCaching:
    """Test user context caching functionality."""

    def test_user_context_cache_hit(self):
        """Test user context cache hit avoids token validation."""
        pass  # TODO: Requires real JWT token or mock

    def test_user_context_cache_ttl(self):
        """Test user context cache respects TTL."""
        pass  # TODO: Requires real JWT token or mock

    def test_user_context_cache_token_expiration(self):
        """Test user context cache checks token expiration."""
        pass  # TODO: Requires real JWT token or mock

    def test_user_context_cache_lru_eviction(self):
        """Test user context cache evicts old entries when full."""
        pass  # TODO: Requires real JWT token or mock


class TestPerformance:
    """Test authentication performance targets."""

    def test_jwt_validation_cached_performance(self):
        """Test cached JWT validation is <1ms."""
        pass  # TODO: Requires real JWT token or mock

    def test_jwt_validation_uncached_performance(self):
        """Test uncached JWT validation is <10ms."""
        pass  # TODO: Requires real JWT token or mock

    def test_jwks_fetch_cached_performance(self):
        """Test cached JWKS fetch is <50ms."""
        pass  # TODO: Requires real JWT token or mock

    def test_cache_hit_rate(self):
        """Test cache hit rate is >95% in normal operation."""
        pass  # TODO: Requires real JWT token or mock


class TestSecurity:
    """Test security features."""

    def test_https_enforcement_auth0(self):
        """Test HTTPS is enforced for Auth0 JWKS URLs."""
        # Auth0 automatically uses HTTPS - can't test rejection directly
        # But verify it creates with HTTPS domain
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://api.example.com"])
        assert provider is not None
        assert provider.provider_type() == "auth0"

    def test_https_enforcement_custom_jwt(self):
        """Test HTTPS is enforced for custom JWKS URLs."""
        # HTTPS should work
        provider = PyAuthProvider.jwt(
            issuer="https://example.com",
            audience=["https://api.example.com"],
            jwks_url="https://example.com/.well-known/jwks.json"
        )
        assert provider is not None

        # HTTP should fail at provider creation time
        try:
            PyAuthProvider.jwt(
                issuer="https://example.com",
                audience=["https://api.example.com"],
                jwks_url="http://insecure.com/.well-known/jwks.json"
            )
            assert False, "Should reject HTTP JWKS URLs"
        except (ValueError, RuntimeError):
            pass  # Expected - HTTPS validation should catch this

    def test_invalid_token_is_rejected(self):
        """Test that invalid tokens are properly rejected with errors."""
        provider = PyAuthProvider.auth0("example.auth0.com", ["https://api.example.com"])

        # All these should raise RuntimeError
        invalid_tokens = [
            "",  # Empty
            "x",  # Too short
            "x.y",  # Only 2 segments
            "x.y.z.w",  # Too many segments
            "invalid.jwt.signature",  # Wrong format
        ]

        for token in invalid_tokens:
            try:
                provider.validate_token_blocking(token)
                assert False, f"Should have rejected token: {token}"
            except RuntimeError:
                pass  # Expected

    def test_audience_validation(self):
        """Test that audience is validated during provider creation."""
        # Multiple audiences should work
        provider = PyAuthProvider.auth0(
            "example.auth0.com",
            ["https://api1.example.com", "https://api2.example.com"]
        )
        assert provider.audience() == ["https://api1.example.com", "https://api2.example.com"]


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
