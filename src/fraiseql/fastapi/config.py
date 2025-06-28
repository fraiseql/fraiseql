"""Configuration for FraiseQL FastAPI integration."""

from typing import Literal

from pydantic import PostgresDsn, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class FraiseQLConfig(BaseSettings):
    """Configuration for FraiseQL application.

    This class defines all configuration options for a FraiseQL-powered FastAPI
    application. Configuration values can be set through environment variables,
    .env files, or directly in code.

    Environment variables should be prefixed with the app name and use uppercase
    (e.g., FRAISEQL_DATABASE_URL).

    Attributes:
        database_url: PostgreSQL connection URL with JSONB support required.
        database_pool_size: Maximum number of database connections in the pool.
        database_max_overflow: Maximum overflow connections allowed beyond pool_size.
        database_pool_timeout: Seconds to wait before timing out when acquiring connection.
        database_echo: Enable SQL query logging (use only in development).
        app_name: Application name displayed in API documentation.
        app_version: Application version string.
        environment: Current environment (development/production/testing).
        enable_introspection: Allow GraphQL schema introspection queries.
        enable_playground: Enable GraphQL playground IDE.
        playground_tool: Which GraphQL IDE to use (graphiql or apollo-sandbox).
        max_query_depth: Maximum allowed query depth to prevent abuse.
        query_timeout: Maximum query execution time in seconds.
        auto_camel_case: Automatically convert snake_case fields to camelCase.
        enable_auth: Enable authentication and authorization.
        auth_provider: Authentication provider to use.
        auth0_domain: Auth0 tenant domain (required if using Auth0).
        auth0_api_identifier: Auth0 API identifier (required if using Auth0).
        auth0_cache_ttl: Cache TTL for Auth0 JWKS in seconds.
        cors_allow_origins: List of allowed CORS origins.
        cors_allow_credentials: Allow credentials in CORS requests.
        cors_allow_methods: Allowed HTTP methods for CORS.
        cors_allow_headers: Allowed headers for CORS requests.
        enable_metrics: Enable Prometheus metrics endpoint.
        metrics_path: URL path for metrics endpoint.
        enable_health_check: Enable health check endpoints.
        health_check_path: URL path for health check.
        enable_rate_limiting: Enable rate limiting.
        rate_limit_requests: Maximum requests per period.
        rate_limit_period: Rate limit period in seconds.
        log_level: Application log level.
        log_format: Log format (json or text).
        enable_request_logging: Log all incoming requests.
        enable_response_logging: Log all outgoing responses.
        request_id_header: Header name for request correlation ID.

    Example:
        ```python
        from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

        config = FraiseQLConfig(
            database_url="postgresql://user:pass@localhost/mydb",
            environment="production",
            enable_auth=True,
            auth_provider="auth0",
            auth0_domain="myapp.auth0.com",
            auth0_api_identifier="https://api.myapp.com"
        )

        app = create_fraiseql_app(types=[User, Post], config=config)
        ```
    """

    # Database settings
    database_url: PostgresDsn
    database_pool_size: int = 20
    database_max_overflow: int = 10
    database_pool_timeout: int = 30
    database_echo: bool = False

    # Application settings
    app_name: str = "FraiseQL API"
    app_version: str = "1.0.0"
    environment: Literal["development", "production", "testing"] = "development"

    # GraphQL settings
    enable_introspection: bool = True
    enable_playground: bool = True
    playground_tool: Literal["graphiql", "apollo-sandbox"] = "graphiql"  # Which GraphQL IDE to use
    max_query_depth: int | None = None
    query_timeout: int = 30  # seconds
    auto_camel_case: bool = True  # Auto-convert snake_case to camelCase in GraphQL

    # Auth settings
    auth_enabled: bool = True
    auth_provider: Literal["auth0", "custom", "none"] = "none"

    # Auth0 specific settings
    auth0_domain: str | None = None
    auth0_api_identifier: str | None = None
    auth0_algorithms: list[str] = ["RS256"]

    # Development auth settings
    dev_auth_username: str = "admin"
    dev_auth_password: str | None = None

    # Performance settings
    enable_query_caching: bool = True
    cache_ttl: int = 300  # seconds
    enable_turbo_router: bool = True  # Enable TurboRouter for registered queries
    turbo_router_cache_size: int = 1000  # Max number of queries to cache

    # CORS settings
    cors_enabled: bool = True
    cors_origins: list[str] = ["*"]
    cors_methods: list[str] = ["GET", "POST"]
    cors_headers: list[str] = ["*"]

    @field_validator("enable_introspection")
    @classmethod
    def introspection_for_dev_only(cls, v: bool, info) -> bool:
        """Disable introspection in production unless explicitly enabled."""
        if info.data.get("environment") == "production" and v is True:
            return False
        return v

    @field_validator("enable_playground")
    @classmethod
    def playground_for_dev_only(cls, v: bool, info) -> bool:
        """Disable playground in production unless explicitly enabled."""
        if info.data.get("environment") == "production" and v is True:
            return False
        return v

    @field_validator("auth0_domain")
    @classmethod
    def validate_auth0_config(cls, v: str | None, info) -> str | None:
        """Validate Auth0 configuration when Auth0 is selected."""
        if info.data.get("auth_provider") == "auth0" and not v:
            msg = "auth0_domain is required when using Auth0 provider"
            raise ValueError(msg)
        return v

    model_config = SettingsConfigDict(
        env_prefix="FRAISEQL_",
        case_sensitive=False,
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",  # Ignore extra environment variables
    )
