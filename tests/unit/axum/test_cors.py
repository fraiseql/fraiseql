"""Unit tests for CORS configuration."""

import pytest

from fraiseql.axum.cors import CORSConfig, InvalidCORSOriginError


class TestCORSConfigCreation:
    """Test creating CORS configurations."""

    def test_default_config(self) -> None:
        """Test creating default CORS config."""
        cors = CORSConfig()

        assert cors.allow_origins == "*"
        assert cors.allow_credentials is True
        assert "GET" in cors.allow_methods
        assert "POST" in cors.allow_methods

    def test_custom_config(self) -> None:
        """Test creating custom CORS config."""
        cors = CORSConfig(
            allow_origins=["https://example.com"],
            allow_credentials=False,
            allow_methods=["GET", "POST"],
            max_age=7200,
        )

        assert cors.allow_origins == ["https://example.com"]
        assert cors.allow_credentials is False
        assert cors.allow_methods == ["GET", "POST"]
        assert cors.max_age == 7200


class TestCORSValidation:
    """Test CORS origin validation."""

    def test_wildcard_origin(self) -> None:
        """Test wildcard origin."""
        cors = CORSConfig(allow_origins="*")
        assert cors.allow_origins == "*"

    def test_valid_single_origin(self) -> None:
        """Test valid single origin."""
        cors = CORSConfig(allow_origins="https://example.com")
        assert cors.allow_origins == ["https://example.com"]

    def test_valid_multiple_origins(self) -> None:
        """Test valid multiple origins."""
        origins = ["https://example.com", "https://app.example.com"]
        cors = CORSConfig(allow_origins=origins)
        assert cors.allow_origins == origins

    def test_invalid_origin_no_scheme(self) -> None:
        """Test that origins without http/https are rejected."""
        with pytest.raises(InvalidCORSOriginError):
            CORSConfig(allow_origins="example.com")

    def test_invalid_origin_bad_scheme(self) -> None:
        """Test that non-http schemes are rejected."""
        with pytest.raises(InvalidCORSOriginError):
            CORSConfig(allow_origins="ftp://example.com")

    def test_wildcard_with_other_origins_rejected(self) -> None:
        """Test that wildcard cannot be combined with other origins."""
        with pytest.raises(InvalidCORSOriginError):
            CORSConfig(allow_origins=["*", "https://example.com"])

    def test_http_origin_allowed(self) -> None:
        """Test that HTTP origins are allowed (for localhost dev)."""
        cors = CORSConfig(allow_origins="http://localhost:3000")
        assert cors.allow_origins == ["http://localhost:3000"]


class TestCORSPermissive:
    """Test permissive CORS preset."""

    def test_permissive_allows_all_origins(self) -> None:
        """Test that permissive allows all origins."""
        cors = CORSConfig.permissive()
        assert cors.allow_origins == "*"

    def test_permissive_disables_credentials(self) -> None:
        """Test that permissive disables credentials (required with wildcard)."""
        cors = CORSConfig.permissive()
        assert cors.allow_credentials is False


class TestCORSProduction:
    """Test production CORS preset."""

    def test_production_single_domain(self) -> None:
        """Test production config for single domain."""
        cors = CORSConfig.production("example.com")
        assert "https://example.com" in cors.allow_origins
        assert len(cors.allow_origins) == 1

    def test_production_https_only_by_default(self) -> None:
        """Test that production uses HTTPS only by default."""
        cors = CORSConfig.production("example.com")
        # Should only have HTTPS, not HTTP
        assert all(origin.startswith("https://") for origin in cors.allow_origins)

    def test_production_with_http(self) -> None:
        """Test production config with HTTP allowed."""
        cors = CORSConfig.production("example.com", https_only=False)
        assert "http://example.com" in cors.allow_origins
        assert "https://example.com" in cors.allow_origins

    def test_production_with_subdomains(self) -> None:
        """Test production config with subdomains."""
        cors = CORSConfig.production("example.com", allow_subdomains=True)
        assert "https://example.com" in cors.allow_origins
        assert "https://*.example.com" in cors.allow_origins

    def test_production_normalizes_domain(self) -> None:
        """Test that production normalizes domain format."""
        # Input with https://
        cors1 = CORSConfig.production("https://example.com")
        # Input with trailing slash
        cors2 = CORSConfig.production("example.com/")
        # Both should produce same result
        assert cors1.allow_origins == cors2.allow_origins

    def test_production_invalid_domain(self) -> None:
        """Test that invalid domains are rejected."""
        with pytest.raises(InvalidCORSOriginError):
            CORSConfig.production("invalid")

    def test_production_empty_domain(self) -> None:
        """Test that empty domains are rejected."""
        with pytest.raises(InvalidCORSOriginError):
            CORSConfig.production("")


class TestCORSMultiTenant:
    """Test multi-tenant CORS preset."""

    def test_multi_tenant_multiple_domains(self) -> None:
        """Test multi-tenant config with multiple domains."""
        domains = ["app1.example.com", "app2.example.com"]
        cors = CORSConfig.multi_tenant(domains)

        assert "https://app1.example.com" in cors.allow_origins
        assert "https://app2.example.com" in cors.allow_origins
        assert len(cors.allow_origins) == 2

    def test_multi_tenant_with_http(self) -> None:
        """Test multi-tenant with HTTP allowed."""
        domains = ["app1.example.com", "app2.example.com"]
        cors = CORSConfig.multi_tenant(domains, https_only=False)

        assert "http://app1.example.com" in cors.allow_origins
        assert "https://app2.example.com" in cors.allow_origins
        assert len(cors.allow_origins) == 4

    def test_multi_tenant_invalid_domain(self) -> None:
        """Test that invalid domains are rejected."""
        with pytest.raises(InvalidCORSOriginError):
            CORSConfig.multi_tenant(["invalid"])


class TestCORSLocalhost:
    """Test localhost CORS preset."""

    def test_localhost_default_ports(self) -> None:
        """Test localhost config with default ports."""
        cors = CORSConfig.localhost()

        assert "http://localhost:3000" in cors.allow_origins
        assert "http://localhost:3001" in cors.allow_origins
        assert "http://localhost:8000" in cors.allow_origins
        assert cors.allow_credentials is True

    def test_localhost_custom_ports(self) -> None:
        """Test localhost config with custom ports."""
        cors = CORSConfig.localhost([4200, 4300])

        assert "http://localhost:4200" in cors.allow_origins
        assert "http://localhost:4300" in cors.allow_origins
        # Should not have default ports
        assert "http://localhost:3000" not in cors.allow_origins

    def test_localhost_no_cache(self) -> None:
        """Test that localhost doesn't cache preflight."""
        cors = CORSConfig.localhost()
        assert cors.max_age == 0


class TestCORSCustom:
    """Test custom CORS configuration."""

    def test_custom_with_all_options(self) -> None:
        """Test custom config with all options."""
        cors = CORSConfig.custom(
            allow_origins=["https://example.com"],
            allow_credentials=False,
            allow_methods=["GET", "POST", "PUT"],
            allow_headers=["Authorization", "X-Custom"],
            expose_headers=["X-Total-Count"],
            max_age=7200,
        )

        assert cors.allow_origins == ["https://example.com"]
        assert cors.allow_credentials is False
        assert cors.allow_methods == ["GET", "POST", "PUT"]
        assert cors.allow_headers == ["Authorization", "X-Custom"]
        assert cors.expose_headers == ["X-Total-Count"]
        assert cors.max_age == 7200


class TestCORSSerialization:
    """Test CORS serialization."""

    def test_to_dict(self) -> None:
        """Test converting CORS config to dictionary."""
        cors = CORSConfig(
            allow_origins="https://example.com",
            allow_credentials=False,
        )

        config_dict = cors.to_dict()

        assert isinstance(config_dict, dict)
        assert "allow_origins" in config_dict
        assert "allow_credentials" in config_dict
        assert config_dict["allow_credentials"] is False

    def test_to_dict_includes_methods(self) -> None:
        """Test that to_dict includes all fields."""
        cors = CORSConfig()
        config_dict = cors.to_dict()

        assert "allow_methods" in config_dict
        assert "allow_headers" in config_dict
        assert "expose_headers" in config_dict
        assert "max_age" in config_dict


class TestCORSStringRepresentation:
    """Test string representations."""

    def test_repr(self) -> None:
        """Test __repr__ method."""
        cors = CORSConfig(allow_origins="*")
        repr_str = repr(cors)

        assert "CORSConfig" in repr_str
        assert "*" in repr_str or "permissive" in repr_str

    def test_repr_multiple_origins(self) -> None:
        """Test __repr__ with multiple origins."""
        cors = CORSConfig(allow_origins=["https://a.com", "https://b.com"])
        repr_str = repr(cors)

        assert "CORSConfig" in repr_str
        assert "2" in repr_str  # Number of origins

    def test_str_permissive(self) -> None:
        """Test __str__ for permissive config."""
        cors = CORSConfig.permissive()
        str_repr = str(cors)

        assert "all origins" in str_repr or "*" in str_repr

    def test_str_specific_origins(self) -> None:
        """Test __str__ for specific origins."""
        cors = CORSConfig(allow_origins=["https://example.com"])
        str_repr = str(cors)

        assert "specific origins" in str_repr


class TestCORSIntegration:
    """Integration tests for CORS."""

    def test_development_setup(self) -> None:
        """Test typical development CORS setup."""
        cors = CORSConfig.localhost([3000, 4200])

        assert cors.allow_credentials is True
        assert cors.max_age == 0
        assert len(cors.allow_origins) > 0
        assert all("localhost" in origin or "127.0.0.1" in origin for origin in cors.allow_origins)

    def test_production_setup(self) -> None:
        """Test typical production CORS setup."""
        cors = CORSConfig.production("example.com", allow_subdomains=True)

        assert cors.allow_credentials is True
        assert all(origin.startswith("https://") for origin in cors.allow_origins)
        assert len(cors.allow_origins) == 2  # example.com + *.example.com

    def test_production_multi_env_setup(self) -> None:
        """Test production setup across multiple environments."""
        cors = CORSConfig.multi_tenant(
            [
                "staging.example.com",
                "app.example.com",
            ]
        )

        assert len(cors.allow_origins) == 2
        assert all(origin.startswith("https://") for origin in cors.allow_origins)
