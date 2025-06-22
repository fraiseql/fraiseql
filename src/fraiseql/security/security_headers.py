"""Security headers middleware for FraiseQL applications."""

import logging
from collections.abc import Callable
from dataclasses import dataclass, field
from enum import Enum

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware

logger = logging.getLogger(__name__)


class CSPDirective(Enum):
    """Content Security Policy directives."""

    DEFAULT_SRC = "default-src"
    SCRIPT_SRC = "script-src"
    STYLE_SRC = "style-src"
    IMG_SRC = "img-src"
    CONNECT_SRC = "connect-src"
    FONT_SRC = "font-src"
    OBJECT_SRC = "object-src"
    MEDIA_SRC = "media-src"
    FRAME_SRC = "frame-src"
    CHILD_SRC = "child-src"
    FORM_ACTION = "form-action"
    FRAME_ANCESTORS = "frame-ancestors"
    BASE_URI = "base-uri"
    MANIFEST_SRC = "manifest-src"
    WORKER_SRC = "worker-src"
    UPGRADE_INSECURE_REQUESTS = "upgrade-insecure-requests"
    BLOCK_ALL_MIXED_CONTENT = "block-all-mixed-content"
    REQUIRE_SRI_FOR = "require-sri-for"
    REPORT_URI = "report-uri"
    REPORT_TO = "report-to"


class ReferrerPolicy(Enum):
    """Referrer policy values."""

    NO_REFERRER = "no-referrer"
    NO_REFERRER_WHEN_DOWNGRADE = "no-referrer-when-downgrade"
    ORIGIN = "origin"
    ORIGIN_WHEN_CROSS_ORIGIN = "origin-when-cross-origin"
    SAME_ORIGIN = "same-origin"
    STRICT_ORIGIN = "strict-origin"
    STRICT_ORIGIN_WHEN_CROSS_ORIGIN = "strict-origin-when-cross-origin"
    UNSAFE_URL = "unsafe-url"


class FrameOptions(Enum):
    """X-Frame-Options values."""

    DENY = "DENY"
    SAMEORIGIN = "SAMEORIGIN"
    ALLOW_FROM = "ALLOW-FROM"


@dataclass
class ContentSecurityPolicy:
    """Content Security Policy configuration."""

    directives: dict[CSPDirective, list[str]] = field(default_factory=dict)
    report_only: bool = False

    def add_directive(self, directive: CSPDirective, sources: str | list[str]) -> None:
        """Add a CSP directive with sources."""
        if isinstance(sources, str):
            sources = [sources]

        if directive not in self.directives:
            self.directives[directive] = []

        self.directives[directive].extend(sources)

    def remove_directive(self, directive: CSPDirective) -> None:
        """Remove a CSP directive."""
        self.directives.pop(directive, None)

    def to_header_value(self) -> str:
        """Convert CSP to header value."""
        parts = []

        for directive, sources in self.directives.items():
            if directive in [
                CSPDirective.UPGRADE_INSECURE_REQUESTS,
                CSPDirective.BLOCK_ALL_MIXED_CONTENT,
            ]:
                # These directives don't have sources
                parts.append(directive.value)
            else:
                sources_str = " ".join(sources)
                parts.append(f"{directive.value} {sources_str}")

        return "; ".join(parts)

    def get_header_name(self) -> str:
        """Get the appropriate header name."""
        return (
            "Content-Security-Policy-Report-Only" if self.report_only else "Content-Security-Policy"
        )


@dataclass
class SecurityHeadersConfig:
    """Security headers configuration."""

    # Content Security Policy
    csp: ContentSecurityPolicy | None = None

    # X-Frame-Options
    frame_options: FrameOptions | None = FrameOptions.SAMEORIGIN
    frame_options_allow_from: str | None = None

    # X-Content-Type-Options
    content_type_options: bool = True

    # X-XSS-Protection (deprecated but still useful for older browsers)
    xss_protection: bool = True
    xss_protection_mode: str = "1; mode=block"

    # Strict-Transport-Security
    hsts: bool = True
    hsts_max_age: int = 31536000  # 1 year
    hsts_include_subdomains: bool = True
    hsts_preload: bool = False

    # Referrer-Policy
    referrer_policy: ReferrerPolicy | None = ReferrerPolicy.STRICT_ORIGIN_WHEN_CROSS_ORIGIN

    # Permissions-Policy (formerly Feature-Policy)
    permissions_policy: dict[str, list[str]] = field(default_factory=dict)

    # X-Permitted-Cross-Domain-Policies
    cross_domain_policies: str = "none"

    # Cross-Origin policies
    cross_origin_embedder_policy: str | None = None  # "require-corp" or "unsafe-none"
    cross_origin_opener_policy: str | None = (
        None  # "same-origin", "same-origin-allow-popups", "unsafe-none"
    )
    cross_origin_resource_policy: str | None = None  # "same-site", "same-origin", "cross-origin"

    # Custom headers
    custom_headers: dict[str, str] = field(default_factory=dict)

    # Paths to exclude from security headers
    exclude_paths: set[str] = field(default_factory=lambda: {"/docs", "/redoc", "/openapi.json"})

    # Conditional headers based on request
    conditional_headers: list[Callable[[Request], dict[str, str]]] = field(default_factory=list)


class SecurityHeadersMiddleware(BaseHTTPMiddleware):
    """FastAPI middleware for adding security headers."""

    def __init__(self, app, config: SecurityHeadersConfig) -> None:
        super().__init__(app)
        self.config = config

    async def dispatch(self, request: Request, call_next):
        """Add security headers to responses."""
        response = await call_next(request)

        # Skip excluded paths
        if request.url.path in self.config.exclude_paths:
            return response

        # Add security headers
        await self._add_security_headers(request, response)

        return response

    async def _add_security_headers(self, request: Request, response: Response) -> None:
        """Add all configured security headers."""
        headers = {}

        # Content Security Policy
        if self.config.csp:
            headers[self.config.csp.get_header_name()] = self.config.csp.to_header_value()

        # X-Frame-Options
        if self.config.frame_options:
            frame_options_value = self.config.frame_options.value
            if (
                self.config.frame_options == FrameOptions.ALLOW_FROM
                and self.config.frame_options_allow_from
            ):
                frame_options_value += f" {self.config.frame_options_allow_from}"
            headers["X-Frame-Options"] = frame_options_value

        # X-Content-Type-Options
        if self.config.content_type_options:
            headers["X-Content-Type-Options"] = "nosniff"

        # X-XSS-Protection
        if self.config.xss_protection:
            headers["X-XSS-Protection"] = self.config.xss_protection_mode

        # Strict-Transport-Security
        if self.config.hsts:
            hsts_value = f"max-age={self.config.hsts_max_age}"
            if self.config.hsts_include_subdomains:
                hsts_value += "; includeSubDomains"
            if self.config.hsts_preload:
                hsts_value += "; preload"
            headers["Strict-Transport-Security"] = hsts_value

        # Referrer-Policy
        if self.config.referrer_policy:
            headers["Referrer-Policy"] = self.config.referrer_policy.value

        # Permissions-Policy
        if self.config.permissions_policy:
            permissions_parts = []
            for feature, allowlist in self.config.permissions_policy.items():
                if allowlist:
                    allowlist_str = " ".join(f'"{origin}"' for origin in allowlist)
                    permissions_parts.append(f"{feature}=({allowlist_str})")
                else:
                    permissions_parts.append(f"{feature}=()")

            if permissions_parts:
                headers["Permissions-Policy"] = ", ".join(permissions_parts)

        # X-Permitted-Cross-Domain-Policies
        if self.config.cross_domain_policies:
            headers["X-Permitted-Cross-Domain-Policies"] = self.config.cross_domain_policies

        # Cross-Origin policies
        if self.config.cross_origin_embedder_policy:
            headers["Cross-Origin-Embedder-Policy"] = self.config.cross_origin_embedder_policy

        if self.config.cross_origin_opener_policy:
            headers["Cross-Origin-Opener-Policy"] = self.config.cross_origin_opener_policy

        if self.config.cross_origin_resource_policy:
            headers["Cross-Origin-Resource-Policy"] = self.config.cross_origin_resource_policy

        # Custom headers
        headers.update(self.config.custom_headers)

        # Conditional headers
        for condition_func in self.config.conditional_headers:
            conditional_headers = condition_func(request)
            headers.update(conditional_headers)

        # Apply all headers to response
        for name, value in headers.items():
            response.headers[name] = value


# Predefined CSP configurations


def create_strict_csp() -> ContentSecurityPolicy:
    """Create a strict Content Security Policy."""
    csp = ContentSecurityPolicy()

    # Default to self only
    csp.add_directive(CSPDirective.DEFAULT_SRC, "'self'")

    # Scripts: self and unsafe-inline for development
    csp.add_directive(CSPDirective.SCRIPT_SRC, ["'self'", "'unsafe-inline'"])

    # Styles: self and unsafe-inline for CSS frameworks
    csp.add_directive(CSPDirective.STYLE_SRC, ["'self'", "'unsafe-inline'"])

    # Images: self and data URLs
    csp.add_directive(CSPDirective.IMG_SRC, ["'self'", "data:"])

    # Connections: self for API calls
    csp.add_directive(CSPDirective.CONNECT_SRC, "'self'")

    # Fonts: self and common CDNs
    csp.add_directive(CSPDirective.FONT_SRC, ["'self'", "https://fonts.gstatic.com"])

    # No plugins
    csp.add_directive(CSPDirective.OBJECT_SRC, "'none'")

    # No frames by default
    csp.add_directive(CSPDirective.FRAME_SRC, "'none'")

    # Form actions: self only
    csp.add_directive(CSPDirective.FORM_ACTION, "'self'")

    # Frame ancestors: none (no embedding)
    csp.add_directive(CSPDirective.FRAME_ANCESTORS, "'none'")

    # Base URI: self only
    csp.add_directive(CSPDirective.BASE_URI, "'self'")

    # Upgrade insecure requests
    csp.add_directive(CSPDirective.UPGRADE_INSECURE_REQUESTS, [])

    return csp


def create_development_csp() -> ContentSecurityPolicy:
    """Create a development-friendly Content Security Policy."""
    csp = ContentSecurityPolicy()

    # More permissive for development
    csp.add_directive(CSPDirective.DEFAULT_SRC, ["'self'", "'unsafe-inline'", "'unsafe-eval'"])

    # Allow common development servers
    csp.add_directive(
        CSPDirective.CONNECT_SRC,
        [
            "'self'",
            "http://localhost:*",
            "ws://localhost:*",
            "wss://localhost:*",
        ],
    )

    # Allow images from anywhere for development
    csp.add_directive(CSPDirective.IMG_SRC, ["'self'", "data:", "http:", "https:"])

    # Allow fonts from CDNs
    csp.add_directive(
        CSPDirective.FONT_SRC,
        [
            "'self'",
            "https://fonts.gstatic.com",
            "https://cdn.jsdelivr.net",
        ],
    )

    # No plugins
    csp.add_directive(CSPDirective.OBJECT_SRC, "'none'")

    return csp


def create_api_csp() -> ContentSecurityPolicy:
    """Create a CSP for API-only applications."""
    csp = ContentSecurityPolicy()

    # Very restrictive for APIs
    csp.add_directive(CSPDirective.DEFAULT_SRC, "'none'")
    csp.add_directive(CSPDirective.FRAME_ANCESTORS, "'none'")
    csp.add_directive(CSPDirective.FORM_ACTION, "'none'")
    csp.add_directive(CSPDirective.BASE_URI, "'none'")

    return csp


# Predefined configurations


def create_production_security_config(
    domain: str,
    api_only: bool = False,
    enable_hsts_preload: bool = False,
) -> SecurityHeadersConfig:
    """Create a production security headers configuration."""
    config = SecurityHeadersConfig()

    # Strict CSP for production
    if api_only:
        config.csp = create_api_csp()
    else:
        config.csp = create_strict_csp()
        # Add domain-specific sources
        config.csp.add_directive(CSPDirective.CONNECT_SRC, f"https://{domain}")

    # Frame options
    config.frame_options = FrameOptions.DENY if api_only else FrameOptions.SAMEORIGIN

    # HSTS with preload if requested
    config.hsts = True
    config.hsts_max_age = 31536000  # 1 year
    config.hsts_include_subdomains = True
    config.hsts_preload = enable_hsts_preload

    # Strict referrer policy
    config.referrer_policy = ReferrerPolicy.STRICT_ORIGIN_WHEN_CROSS_ORIGIN

    # Disable dangerous features
    config.permissions_policy = {
        "camera": [],
        "microphone": [],
        "geolocation": [],
        "payment": [],
        "usb": [],
        "magnetometer": [],
        "gyroscope": [],
        "accelerometer": [],
    }

    # Cross-origin policies
    config.cross_origin_embedder_policy = "require-corp"
    config.cross_origin_opener_policy = "same-origin"
    config.cross_origin_resource_policy = "same-site"

    return config


def create_development_security_config() -> SecurityHeadersConfig:
    """Create a development-friendly security headers configuration."""
    config = SecurityHeadersConfig()

    # Development-friendly CSP
    config.csp = create_development_csp()

    # Less strict frame options
    config.frame_options = FrameOptions.SAMEORIGIN

    # No HSTS in development (allows HTTP)
    config.hsts = False

    # Permissive referrer policy
    config.referrer_policy = ReferrerPolicy.NO_REFERRER_WHEN_DOWNGRADE

    # Minimal permissions restrictions
    config.permissions_policy = {}

    # No cross-origin restrictions
    config.cross_origin_embedder_policy = None
    config.cross_origin_opener_policy = None
    config.cross_origin_resource_policy = None

    # Include docs in excluded paths
    config.exclude_paths = {"/docs", "/redoc", "/openapi.json", "/graphql", "/playground"}

    return config


def create_graphql_security_config(
    trusted_origins: list[str],
    enable_introspection: bool = False,
) -> SecurityHeadersConfig:
    """Create security headers configuration for GraphQL APIs."""
    config = SecurityHeadersConfig()

    # API-focused CSP
    config.csp = create_api_csp()

    # Allow connections from trusted origins
    config.csp.add_directive(CSPDirective.CONNECT_SRC, ["'self'", *trusted_origins])

    # If GraphQL Playground/GraphiQL is enabled
    if enable_introspection:
        config.csp.add_directive(
            CSPDirective.SCRIPT_SRC,
            [
                "'self'",
                "'unsafe-inline'",
                "'unsafe-eval'",  # Required for GraphiQL
                "https://unpkg.com",  # For GraphiQL CDN
                "https://embeddable-sandbox.cdn.apollographql.com",  # For Apollo Sandbox
            ],
        )
        config.csp.add_directive(
            CSPDirective.STYLE_SRC,
            [
                "'self'",
                "'unsafe-inline'",
                "https://unpkg.com",  # For GraphiQL styles
            ],
        )
        config.csp.add_directive(CSPDirective.IMG_SRC, ["'self'", "data:"])
        config.csp.add_directive(
            CSPDirective.CONNECT_SRC,
            [
                "'self'",
                "https://embeddable-sandbox.cdn.apollographql.com",  # For Apollo Sandbox API calls
            ],
        )
        config.exclude_paths.add("/graphql")
        config.exclude_paths.add("/playground")

    # Frame protection
    config.frame_options = FrameOptions.DENY

    # CORS-related headers
    config.cross_origin_resource_policy = "cross-origin"

    return config


# Convenience function


def setup_security_headers(
    app,
    config: SecurityHeadersConfig | None = None,
    environment: str = "production",
) -> SecurityHeadersMiddleware:
    """Set up security headers middleware with sensible defaults."""
    if config is None:
        if environment == "development":
            config = create_development_security_config()
        else:
            # Need domain for production config
            config = create_production_security_config("example.com")

    middleware = SecurityHeadersMiddleware(app=app, config=config)
    app.add_middleware(SecurityHeadersMiddleware, config=config)

    return middleware


# CSP Reporting (for monitoring CSP violations)


def create_csp_report_handler(webhook_url: str | None = None):
    """Create a CSP violation report handler."""

    async def csp_report_endpoint(request: Request):
        """Handle CSP violation reports."""
        try:
            report = await request.json()

            # Log violation
            violation = report.get("csp-report", {})
            logger.warning(
                "CSP Violation: %s blocked %s on %s",
                violation.get("violated-directive"),
                violation.get("blocked-uri"),
                violation.get("document-uri"),
            )

            # Send to webhook if configured
            if webhook_url:
                import httpx

                async with httpx.AsyncClient() as client:
                    await client.post(webhook_url, json=report)

            return {"status": "received"}  # noqa: TRY300

        except Exception:
            logger.exception("Error handling CSP report")
            return {"status": "error"}

    return csp_report_endpoint


def add_csp_reporting(app, report_uri: str, webhook_url: str | None = None):
    """Add CSP violation reporting to the application."""
    # Add report endpoint
    report_handler = create_csp_report_handler(webhook_url)
    app.post(report_uri)(report_handler)

    return report_handler
