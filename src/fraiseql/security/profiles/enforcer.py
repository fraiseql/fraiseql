"""Security profile enforcer that bridges profiles to middleware.

This module provides the ProfileEnforcer class that takes a SecurityProfileConfig
and generates the appropriate middleware configurations and stacks for enforcing
security policies in a FastAPI application.
"""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Callable

from fastapi import Request

from .definitions import SecurityProfileConfig

if TYPE_CHECKING:
    from fraiseql.middleware.body_size_limiter import BodySizeConfig
    from fraiseql.middleware.rate_limiter import RateLimitConfig


@dataclass
class QueryValidatorConfig:
    """Configuration for GraphQL query validation.

    Attributes:
        max_depth: Maximum allowed query depth
        max_complexity: Maximum allowed query complexity
        introspection_enabled: Whether GraphQL introspection is allowed
        field_validation_enabled: Whether field-level validation is enabled
    """

    max_depth: int
    max_complexity: int
    introspection_enabled: bool
    field_validation_enabled: bool = True

    def to_dict(self) -> dict:
        """Convert configuration to dictionary for serialization."""
        return {
            "max_depth": self.max_depth,
            "max_complexity": self.max_complexity,
            "introspection_enabled": self.introspection_enabled,
            "field_validation_enabled": self.field_validation_enabled,
        }


class ProfileEnforcer:
    """Enforces security profiles by bridging to middleware configurations.

    This class takes a SecurityProfileConfig and provides methods to generate
    the appropriate middleware configurations and apply them to a FastAPI app.
    """

    def __init__(self, profile_config: SecurityProfileConfig) -> None:
        """Initialize the profile enforcer.

        Args:
            profile_config: The security profile configuration to enforce
        """
        self.profile_config = profile_config

    def get_body_size_config(self) -> "BodySizeConfig":
        """Get body size limiter configuration from profile."""
        from fraiseql.middleware.body_size_limiter import BodySizeConfig

        return BodySizeConfig(
            max_body_size=self.profile_config.max_body_size,
            exempt_paths=[],  # Can be customized per deployment
            exempt_methods={"GET", "HEAD", "OPTIONS"},  # Standard exempt methods
        )

    def get_rate_limit_config(self) -> "RateLimitConfig":
        """Get rate limiter configuration from profile."""
        from fraiseql.middleware.rate_limiter import RateLimitConfig

        return RateLimitConfig(
            enabled=self.profile_config.rate_limit_enabled,
            requests_per_minute=self.profile_config.rate_limit_requests_per_minute,
            requests_per_hour=self.profile_config.rate_limit_requests_per_minute
            * 60,  # Rough estimate
            burst_size=min(
                10, self.profile_config.rate_limit_requests_per_minute // 6
            ),  # 1/6 of limit
            window_type="sliding",
            key_func=self._default_key_func,
            whitelist=[],  # Can be customized per deployment
            blacklist=[],  # Can be customized per deployment
        )

    def get_query_validator_config(self) -> QueryValidatorConfig:
        """Get GraphQL query validator configuration from profile."""
        return QueryValidatorConfig(
            max_depth=self.profile_config.max_query_depth,
            max_complexity=self.profile_config.max_query_complexity,
            introspection_enabled=self.profile_config.introspection_policy.value
            in {"enabled", "authenticated"},
            field_validation_enabled=True,  # Always enabled for security
        )

    def get_middleware_stack(self) -> list[tuple[Callable, dict[str, Any]]]:
        """Get the complete middleware stack for the security profile.

        Returns:
            List of (middleware_class, config_dict) tuples for FastAPI app.add_middleware()
        """
        middleware_stack = []

        # Body size limiter (always included for security)
        from fraiseql.middleware.body_size_limiter import BodySizeLimiterMiddleware

        body_config = self.get_body_size_config()
        middleware_stack.append((BodySizeLimiterMiddleware, {"config": body_config}))

        # Rate limiter (only if enabled)
        if self.profile_config.rate_limit_enabled:
            from fraiseql.middleware.rate_limiter import InMemoryRateLimiter, RateLimiterMiddleware

            rate_config = self.get_rate_limit_config()
            rate_limiter = InMemoryRateLimiter(rate_config)
            middleware_stack.append((RateLimiterMiddleware, {"rate_limiter": rate_limiter}))

        # Additional security headers (always included)
        from fraiseql.security.security_headers import SecurityHeadersMiddleware

        middleware_stack.append((SecurityHeadersMiddleware, {}))

        # CSRF protection (always included)
        from fraiseql.security.csrf_protection import CSRFProtectionMiddleware

        middleware_stack.append((CSRFProtectionMiddleware, {}))

        return middleware_stack

    def validate_request_context(self, request: Request) -> dict[str, Any]:
        """Validate request context against security profile.

        Args:
            request: The incoming FastAPI request

        Returns:
            Dictionary with validation results and security context

        Raises:
            HTTPException: If request violates security policies
        """
        from fastapi import HTTPException

        context = {
            "profile": self.profile_config.profile.value,
            "tls_required": self.profile_config.tls_required,
            "auth_required": self.profile_config.auth_required,
            "validation_passed": True,
            "warnings": [],
        }

        # TLS validation
        if self.profile_config.tls_required and request.url.scheme != "https":
            raise HTTPException(
                status_code=426,  # Upgrade Required
                detail="HTTPS required for this security profile",
            )

        # Authentication requirement check (basic - actual auth handled elsewhere)
        if self.profile_config.auth_required:
            # This would be expanded with actual auth validation
            # For now, just flag that auth is required
            context["auth_required"] = True

        # Body size pre-check (additional to middleware)
        content_length = request.headers.get("content-length")
        if content_length:
            try:
                size = int(content_length)
                if size > self.profile_config.max_body_size:
                    raise HTTPException(
                        status_code=413,
                        detail=(
                            f"Request body too large: {size} > {self.profile_config.max_body_size}"
                        ),
                    )
            except ValueError:
                context["warnings"].append("Invalid Content-Length header")

        return context

    def apply_to_app(self, app: Any) -> None:
        """Apply the security profile to a FastAPI application.

        Args:
            app: The FastAPI application instance
        """
        middleware_stack = self.get_middleware_stack()

        for middleware_class, config in middleware_stack:
            app.add_middleware(middleware_class, **config)

        # Store profile config on app for runtime access
        app.state.security_profile = self.profile_config
        app.state.profile_enforcer = self

    def _default_key_func(self, request: Request) -> str:
        """Default function to generate rate limiting keys from requests."""
        # Use client IP as default key
        client_host = request.client.host if request.client else "anonymous"

        # Include user ID if authenticated (would be set by auth middleware)
        if hasattr(request.state, "user_id") and getattr(request.state, "user_id", None):
            user_id = request.state.user_id
            return f"{client_host}:{user_id}"

        return client_host
