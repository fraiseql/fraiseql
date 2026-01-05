"""CORS (Cross-Origin Resource Sharing) configuration for Axum server.

Provides production-ready CORS setup with common presets and flexible configuration.
"""

import logging
from typing import Any
from urllib.parse import urlparse

logger = logging.getLogger(__name__)


class InvalidCORSOriginError(ValueError):
    """Raised when a CORS origin is invalid."""


class CORSConfig:
    """Production CORS configuration builder.

    Provides flexible and safe CORS configuration with common presets
    for development and production environments.

    Examples:
        Development (permissive):
            cors_config = CORSConfig.permissive()

        Production (single domain):
            cors_config = CORSConfig.production("example.com")

        Production (multi-domain):
            cors_config = CORSConfig.multi_tenant([
                "app1.example.com",
                "app2.example.com"
            ])

        Custom:
            cors_config = CORSConfig.custom(
                allow_origins=["https://example.com"],
                allow_credentials=True,
                allow_methods=["GET", "POST"],
                max_age=7200
            )
    """

    def __init__(
        self,
        allow_origins: str | list[str] = "*",
        allow_credentials: bool = True,
        allow_methods: list[str] | None = None,
        allow_headers: list[str] | None = None,
        expose_headers: list[str] | None = None,
        max_age: int = 3600,
    ):
        """Initialize CORS configuration.

        Args:
            allow_origins: Origins to allow. "*" for all, or list of URLs.
            allow_credentials: Allow credentials (cookies, authorization headers).
            allow_methods: Allowed HTTP methods. None = standard methods (GET, POST, etc.)
            allow_headers: Allowed request headers. None = standard headers.
            expose_headers: Headers to expose to client.
            max_age: How long (seconds) browser can cache preflight response.

        Raises:
            InvalidCORSOriginError: If any origin is invalid.
        """
        self.allow_origins = self._validate_origins(allow_origins)
        self.allow_credentials = allow_credentials
        self.allow_methods = allow_methods or [
            "GET",
            "HEAD",
            "POST",
            "PUT",
            "DELETE",
            "OPTIONS",
            "PATCH",
        ]
        self.allow_headers = allow_headers or [
            "Content-Type",
            "Authorization",
            "X-Requested-With",
        ]
        self.expose_headers = expose_headers or []
        self.max_age = max_age

        logger.debug(f"Created CORS config: origins={self.allow_origins}")

    @staticmethod
    def _validate_origins(origins: str | list[str]) -> str | list[str]:
        """Validate CORS origins format.

        Args:
            origins: Single origin or list of origins.

        Returns:
            Validated origins.

        Raises:
            InvalidCORSOriginError: If any origin is invalid.
        """
        if origins == "*":
            return "*"

        if isinstance(origins, str):
            origins = [origins]

        validated = []
        for origin in origins:
            if origin == "*":
                # Wildcard is only valid alone
                if len(origins) > 1:
                    raise InvalidCORSOriginError(
                        "Wildcard '*' cannot be combined with other origins",
                    )
                return "*"

            # Validate URL format
            if not origin.startswith(("http://", "https://")):
                raise InvalidCORSOriginError(
                    f"Origin must start with http:// or https://: {origin}",
                )

            # Parse URL to validate format
            try:
                parsed = urlparse(origin)
                if not parsed.hostname:
                    raise InvalidCORSOriginError(f"Invalid origin URL: {origin}")
            except Exception as e:
                raise InvalidCORSOriginError(f"Invalid origin URL: {origin}") from e

            validated.append(origin)

        return validated

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for Axum configuration.

        Returns:
            Dictionary suitable for Axum CORS middleware.
        """
        return {
            "allow_origins": self.allow_origins,
            "allow_credentials": self.allow_credentials,
            "allow_methods": self.allow_methods,
            "allow_headers": self.allow_headers,
            "expose_headers": self.expose_headers,
            "max_age": self.max_age,
        }

    @classmethod
    def permissive(cls) -> "CORSConfig":
        """Create permissive CORS (allow all origins).

        ⚠️ Use only in development!

        Returns:
            CORSConfig allowing all origins.

        Example:
            cors_config = CORSConfig.permissive()
        """
        logger.warning("CORS permissive mode: allowing all origins (development only)")
        return cls(
            allow_origins="*",
            allow_credentials=False,  # Can't use credentials with wildcard
            max_age=3600,
        )

    @classmethod
    def production(
        cls,
        domain: str,
        allow_subdomains: bool = False,
        https_only: bool = True,
    ) -> "CORSConfig":
        """Create CORS for production (single domain).

        Converts domain to full HTTPS URL(s) with validation.

        Args:
            domain: Domain name (e.g., "example.com" or "https://example.com")
            allow_subdomains: Also allow *.domain.com (e.g., app.example.com)
            https_only: Require HTTPS. If False, allow both HTTP and HTTPS.

        Returns:
            CORSConfig for production single-domain setup.

        Example:
            cors_config = CORSConfig.production("example.com")
            # Allows: https://example.com

            cors_config = CORSConfig.production(
                "example.com",
                allow_subdomains=True
            )
            # Allows: https://example.com, https://*.example.com
        """
        # Normalize domain
        if domain.startswith(("http://", "https://")):
            domain = domain.split("//", 1)[1].rstrip("/")
        else:
            domain = domain.rstrip("/")

        # Validate domain format
        if not domain or "." not in domain:
            raise InvalidCORSOriginError(f"Invalid domain: {domain}")

        origins = []

        # Add HTTPS origin
        origins.append(f"https://{domain}")

        # Add HTTP origin if not HTTPS only
        if not https_only:
            origins.append(f"http://{domain}")

        # Add subdomain origins if enabled
        if allow_subdomains:
            origins.append(f"https://*.{domain}")
            if not https_only:
                origins.append(f"http://*.{domain}")

        logger.info(f"Production CORS for {domain}: {origins}")
        return cls(allow_origins=origins)

    @classmethod
    def multi_tenant(
        cls,
        domains: list[str],
        https_only: bool = True,
    ) -> "CORSConfig":
        """Create CORS for multi-tenant production (multiple domains).

        Converts list of domains to full HTTPS URLs with validation.

        Args:
            domains: List of domain names (e.g., ["app1.example.com", "app2.example.com"])
            https_only: Require HTTPS.

        Returns:
            CORSConfig for multi-tenant setup.

        Example:
            cors_config = CORSConfig.multi_tenant([
                "app1.example.com",
                "app2.example.com"
            ])
        """
        origins = []

        for domain in domains:
            # Normalize domain
            normalized_domain = domain
            if normalized_domain.startswith(("http://", "https://")):
                normalized_domain = normalized_domain.split("//", 1)[1].rstrip("/")
            else:
                normalized_domain = normalized_domain.rstrip("/")

            # Validate domain
            if not normalized_domain or "." not in normalized_domain:
                raise InvalidCORSOriginError(f"Invalid domain: {domain}")

            origins.append(f"https://{normalized_domain}")
            if not https_only:
                origins.append(f"http://{normalized_domain}")

        logger.info(f"Multi-tenant CORS for {len(domains)} domains")
        return cls(allow_origins=origins)

    @classmethod
    def localhost(cls, ports: list[int] | None = None) -> "CORSConfig":
        """Create CORS for localhost development.

        Allows localhost on common development ports.

        Args:
            ports: Ports to allow. Default: [3000, 3001, 8000, 8001, 5173]

        Returns:
            CORSConfig for localhost development.

        Example:
            cors_config = CORSConfig.localhost()
            # Allows: http://localhost:3000, http://localhost:3001, etc.

            cors_config = CORSConfig.localhost([3000, 4200])
            # Allows: http://localhost:3000, http://localhost:4200
        """
        ports = ports or [3000, 3001, 8000, 8001, 5173]
        origins = [f"http://localhost:{port}" for port in ports]
        origins.append("http://127.0.0.1:3000")  # Also allow 127.0.0.1

        logger.info(f"Localhost CORS for ports: {ports}")
        return cls(
            allow_origins=origins,
            allow_credentials=True,
            max_age=0,  # Don't cache during development
        )

    @classmethod
    def custom(
        cls,
        allow_origins: str | list[str],
        allow_credentials: bool = True,
        allow_methods: list[str] | None = None,
        allow_headers: list[str] | None = None,
        expose_headers: list[str] | None = None,
        max_age: int = 3600,
    ) -> "CORSConfig":
        """Create custom CORS configuration.

        Args:
            allow_origins: Origins to allow (string or list)
            allow_credentials: Allow credentials
            allow_methods: Allowed HTTP methods
            allow_headers: Allowed request headers
            expose_headers: Headers to expose to client
            max_age: Preflight cache time in seconds

        Returns:
            CORSConfig with custom settings.

        Example:
            cors_config = CORSConfig.custom(
                allow_origins=["https://example.com", "https://app.example.com"],
                allow_credentials=True,
                allow_methods=["GET", "POST"],
                max_age=7200
            )
        """
        return cls(
            allow_origins=allow_origins,
            allow_credentials=allow_credentials,
            allow_methods=allow_methods,
            allow_headers=allow_headers,
            expose_headers=expose_headers,
            max_age=max_age,
        )

    def __repr__(self) -> str:
        """Return string representation."""
        """String representation."""
        origins = (
            f"{self.allow_origins!r}"
            if isinstance(self.allow_origins, str)
            else f"{len(self.allow_origins)} origins"
        )
        return f"CORSConfig(origins={origins}, credentials={self.allow_credentials})"

    def __str__(self) -> str:
        """User-friendly string."""
        if self.allow_origins == "*":
            origins_str = "all origins (permissive)"
        elif isinstance(self.allow_origins, list):
            origins_str = f"{len(self.allow_origins)} specific origins"
        else:
            origins_str = str(self.allow_origins)

        return (
            f"CORS Configuration: {origins_str}, "
            f"credentials={'allowed' if self.allow_credentials else 'not allowed'}"
        )
