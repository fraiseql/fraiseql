"""Configuration for FraiseQL FastAPI integration."""

from typing import Literal

from pydantic import PostgresDsn, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class FraiseQLConfig(BaseSettings):
    """Configuration for FraiseQL application."""

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
    max_query_depth: int | None = None
    query_timeout: int = 30  # seconds
    auto_camel_case: bool = False  # Auto-convert snake_case to camelCase in GraphQL

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
    enable_query_compilation: bool = False
    compiled_queries_path: str | None = None

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
            raise ValueError("auth0_domain is required when using Auth0 provider")
        return v

    model_config = SettingsConfigDict(
        env_prefix="FRAISEQL_",
        case_sensitive=False,
        env_file=".env",
        env_file_encoding="utf-8",
    )
