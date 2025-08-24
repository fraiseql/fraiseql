"""Core configuration for Blog Demo Application.

Following PrintOptim Backend configuration patterns for enterprise applications.
"""

import os
from typing import Literal, Optional
from dataclasses import dataclass

from fraiseql import FraiseQLConfig


@dataclass
class BlogConfig:
    """Main configuration for Blog Demo Application."""

    # Database Configuration
    database_url: str
    database_pool_size: int = 10
    database_max_overflow: int = 20
    database_pool_timeout: int = 30

    # Environment Settings
    environment: Literal["development", "test", "production"] = "development"
    debug: bool = False

    # API Configuration
    api_host: str = "0.0.0.0"
    api_port: int = 8000
    api_prefix: str = "/api/v1"

    # GraphQL Configuration
    graphql_path: str = "/graphql"
    graphql_playground: bool = True
    graphql_introspection: bool = True

    # Authentication (following PrintOptim patterns)
    auth_secret_key: Optional[str] = None
    auth_algorithm: str = "HS256"
    auth_access_token_expire_minutes: int = 30

    # Caching Configuration
    enable_query_cache: bool = True
    cache_ttl_seconds: int = 300

    # Logging Configuration
    log_level: str = "INFO"
    log_format: str = "json"

    # Security Configuration
    cors_origins: list[str] = None
    max_query_depth: int = 10
    max_query_complexity: int = 1000

    # Blog-Specific Configuration
    max_post_content_length: int = 50000
    max_comment_content_length: int = 2000
    default_posts_per_page: int = 10
    allow_anonymous_comments: bool = True
    auto_approve_comments: bool = False

    @classmethod
    def from_env(cls) -> "BlogConfig":
        """Create configuration from environment variables."""
        return cls(
            # Database
            database_url=os.getenv(
                "DATABASE_URL",
                "postgresql://postgres:postgres@localhost:5432/blog_demo"
            ),
            database_pool_size=int(os.getenv("DB_POOL_SIZE", "10")),
            database_max_overflow=int(os.getenv("DB_MAX_OVERFLOW", "20")),
            database_pool_timeout=int(os.getenv("DB_POOL_TIMEOUT", "30")),

            # Environment
            environment=os.getenv("ENVIRONMENT", "development"),
            debug=os.getenv("DEBUG", "false").lower() == "true",

            # API
            api_host=os.getenv("API_HOST", "0.0.0.0"),
            api_port=int(os.getenv("API_PORT", "8000")),
            api_prefix=os.getenv("API_PREFIX", "/api/v1"),

            # GraphQL
            graphql_path=os.getenv("GRAPHQL_PATH", "/graphql"),
            graphql_playground=os.getenv("GRAPHQL_PLAYGROUND", "true").lower() == "true",
            graphql_introspection=os.getenv("GRAPHQL_INTROSPECTION", "true").lower() == "true",

            # Authentication
            auth_secret_key=os.getenv("AUTH_SECRET_KEY"),
            auth_algorithm=os.getenv("AUTH_ALGORITHM", "HS256"),
            auth_access_token_expire_minutes=int(os.getenv("AUTH_TOKEN_EXPIRE_MINUTES", "30")),

            # Caching
            enable_query_cache=os.getenv("ENABLE_QUERY_CACHE", "true").lower() == "true",
            cache_ttl_seconds=int(os.getenv("CACHE_TTL_SECONDS", "300")),

            # Logging
            log_level=os.getenv("LOG_LEVEL", "INFO"),
            log_format=os.getenv("LOG_FORMAT", "json"),

            # Security
            cors_origins=os.getenv("CORS_ORIGINS", "").split(",") if os.getenv("CORS_ORIGINS") else ["*"],
            max_query_depth=int(os.getenv("MAX_QUERY_DEPTH", "10")),
            max_query_complexity=int(os.getenv("MAX_QUERY_COMPLEXITY", "1000")),

            # Blog-Specific
            max_post_content_length=int(os.getenv("MAX_POST_CONTENT_LENGTH", "50000")),
            max_comment_content_length=int(os.getenv("MAX_COMMENT_CONTENT_LENGTH", "2000")),
            default_posts_per_page=int(os.getenv("DEFAULT_POSTS_PER_PAGE", "10")),
            allow_anonymous_comments=os.getenv("ALLOW_ANONYMOUS_COMMENTS", "true").lower() == "true",
            auto_approve_comments=os.getenv("AUTO_APPROVE_COMMENTS", "false").lower() == "true",
        )

    def to_fraiseql_config(self) -> FraiseQLConfig:
        """Convert to FraiseQL configuration."""
        return FraiseQLConfig(
            database_url=self.database_url,
            debug=self.debug,
            introspection=self.graphql_introspection,
            playground=self.graphql_playground,
            default_mutation_schema="app",  # Following PrintOptim patterns
            cors_origins=self.cors_origins,
            max_query_depth=self.max_query_depth,
            max_query_complexity=self.max_query_complexity,
        )


# Global configuration instance
config = BlogConfig.from_env()
