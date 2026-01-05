"""Configuration for Axum-based FraiseQL server.

Provides AxumFraiseQLConfig which is a drop-in replacement for FraiseQLConfig,
extended with Axum-specific settings.
"""

import os
from typing import Any

from pydantic import BaseModel, Field, field_validator


class AxumFraiseQLConfig(BaseModel):
    """Configuration for Axum-based FraiseQL server.

    Combines FastAPI configuration options with Axum HTTP server settings.
    This is a drop-in replacement for FraiseQLConfig with additional Axum-specific fields.

    Attributes:
        database_url: PostgreSQL connection URL (required)
        database_pool_size: Number of connections in pool (default: 10)
        database_pool_timeout: Timeout for acquiring connection in seconds (default: 30)
        database_max_overflow: Max overflow connections (default: 20)

        environment: Deployment environment (default: "development")
        production_mode: Enable production optimizations (default: False)

        enable_introspection: Enable GraphQL introspection (default: True)
        enable_playground: Enable GraphQL playground (default: True)
        playground_tool: Playground tool name (default: "graphiql")
        max_query_depth: Max GraphQL query depth (default: 10)

        auth_enabled: Enable authentication (default: False)
        jwt_secret: JWT secret key (optional)
        jwt_algorithm: JWT algorithm (default: "HS256")

        enable_query_caching: Cache query results (default: False)
        cache_ttl: Cache TTL in seconds (default: 300)

        hide_error_details: Hide error details in responses (default: False)

        axum_host: HTTP server bind address (default: "127.0.0.1")
        axum_port: HTTP server port (default: 8000)
        axum_workers: Number of worker threads (default: auto-detect from CPU count)
        axum_metrics_token: Token for /metrics endpoint (default: "")

        cors_origins: Allowed CORS origins (optional)
        cors_allow_credentials: Allow CORS credentials (default: True)
        cors_allow_methods: Allowed CORS methods (optional)
        cors_allow_headers: Allowed CORS headers (optional)

        enable_compression: Enable response compression (default: True)
        compression_algorithm: Compression algorithm "brotli" or "zstd" (default: "brotli")
        compression_min_bytes: Minimum bytes to compress (default: 256)
    """

    # Database configuration (from FastAPI)
    database_url: str = Field(..., description="PostgreSQL connection URL")
    database_pool_size: int = Field(10, ge=1, le=100, description="Connection pool size")
    database_pool_timeout: int = Field(
        30, ge=1, le=300, description="Connection timeout in seconds"
    )
    database_max_overflow: int = Field(20, ge=0, le=200, description="Max overflow connections")

    # Environment
    environment: str = Field("development", pattern="^(development|staging|production)$")
    production_mode: bool = Field(False, description="Enable production optimizations")

    # GraphQL features
    enable_introspection: bool = Field(True, description="Enable GraphQL introspection")
    enable_playground: bool = Field(True, description="Enable GraphQL playground")
    playground_tool: str = Field("graphiql", pattern="^(graphiql|apollo)$")
    max_query_depth: int = Field(10, ge=1, le=50, description="Max query depth")

    # Security
    auth_enabled: bool = Field(False, description="Enable authentication")
    jwt_secret: str | None = Field(None, description="JWT secret key")
    jwt_algorithm: str = Field("HS256", pattern="^(HS256|HS512|RS256)$")

    # Performance
    enable_query_caching: bool = Field(False, description="Enable query result caching")
    cache_ttl: int = Field(300, ge=1, le=3600, description="Cache TTL in seconds")

    # Error handling
    hide_error_details: bool = Field(False, description="Hide error details in responses")

    # Axum HTTP server configuration
    axum_host: str = Field("127.0.0.1", description="HTTP server bind address")
    axum_port: int = Field(8000, ge=1, le=65535, description="HTTP server port")
    axum_workers: int | None = Field(
        None, description="Number of worker threads (auto-detect if None)"
    )
    axum_metrics_token: str = Field("", description="Token for /metrics endpoint access")

    # CORS configuration
    cors_origins: list[str] | None = Field(None, description="Allowed CORS origins")
    cors_allow_credentials: bool = Field(True, description="Allow CORS credentials")
    cors_allow_methods: list[str] | None = Field(None, description="Allowed CORS methods")
    cors_allow_headers: list[str] | None = Field(None, description="Allowed CORS headers")

    # Response compression
    enable_compression: bool = Field(True, description="Enable response compression")
    compression_algorithm: str = Field("brotli", pattern="^(brotli|zstd)$")
    compression_min_bytes: int = Field(256, ge=0, le=10000, description="Minimum bytes to compress")

    class Config:
        """Pydantic configuration."""

        case_sensitive = False
        str_strip_whitespace = True
        validate_default = True

    @field_validator("axum_workers", mode="before")
    @classmethod
    def validate_workers(cls, v: Any) -> int | None:
        """Auto-detect workers from CPU count if None."""
        if v is None:
            return None
        if isinstance(v, int) and v > 0:
            return v
        raise ValueError("axum_workers must be positive integer or None")

    @field_validator("database_url")
    @classmethod
    def validate_database_url(cls, v: str) -> str:
        """Validate database URL format."""
        if not v.startswith(("postgresql://", "postgres://")):
            raise ValueError("Database URL must start with postgresql:// or postgres://")
        return v

    @field_validator("cors_origins")
    @classmethod
    def validate_cors_origins(cls, v: list[str] | None) -> list[str] | None:
        """Validate CORS origins are valid URLs or patterns."""
        if v is None:
            return None
        for origin in v:
            if not origin.startswith(("http://", "https://", "*")):
                raise ValueError(
                    f"CORS origin must start with http://, https://, or be *: {origin}"
                )
        return v

    @property
    def effective_workers(self) -> int:
        """Get effective number of workers, auto-detecting if not configured."""
        if self.axum_workers is not None:
            return self.axum_workers
        return os.cpu_count() or 4

    @property
    def server_url(self) -> str:
        """Get the full server URL."""
        return f"http://{self.axum_host}:{self.axum_port}"

    @classmethod
    def from_env(cls) -> "AxumFraiseQLConfig":
        """Create config from environment variables.

        Environment variables (with defaults):
            FRAISEQL_DATABASE_URL: PostgreSQL URL (required)
            FRAISEQL_ENV: Environment (default: "development")
            FRAISEQL_HOST: Bind address (default: "127.0.0.1")
            FRAISEQL_PORT: Bind port (default: "8000")
            FRAISEQL_WORKERS: Number of workers (default: auto-detect)
            FRAISEQL_AUTH_ENABLED: Enable auth (default: "false")
            FRAISEQL_JWT_SECRET: JWT secret key
            FRAISEQL_PRODUCTION: Enable production mode (default: "false")

        Returns:
            AxumFraiseQLConfig instance

        Raises:
            ValueError: If required environment variables are missing
        """
        database_url = os.getenv("FRAISEQL_DATABASE_URL")
        if not database_url:
            raise ValueError("FRAISEQL_DATABASE_URL environment variable is required")

        return cls(
            database_url=database_url,
            environment=os.getenv("FRAISEQL_ENV", "development"),
            axum_host=os.getenv("FRAISEQL_HOST", "127.0.0.1"),
            axum_port=int(os.getenv("FRAISEQL_PORT", "8000")),
            axum_workers=int(w) if (w := os.getenv("FRAISEQL_WORKERS")) else None,
            auth_enabled=os.getenv("FRAISEQL_AUTH_ENABLED", "false").lower() == "true",
            jwt_secret=os.getenv("FRAISEQL_JWT_SECRET"),
            production_mode=os.getenv("FRAISEQL_PRODUCTION", "false").lower() == "true",
        )

    def to_dict(self) -> dict[str, Any]:
        """Convert config to dictionary.

        Useful for logging and inspection.
        """
        return self.model_dump(exclude_none=True)

    def __str__(self) -> str:
        """String representation of config."""
        return (
            f"AxumFraiseQLConfig("
            f"host={self.axum_host}, "
            f"port={self.axum_port}, "
            f"env={self.environment}, "
            f"workers={self.effective_workers})"
        )
