# Extracted from: docs/reference/config.md
# Block number: 19
from fraiseql import FraiseQLConfig
from fraiseql.fastapi.config import IntrospectionPolicy

config = FraiseQLConfig(
    # Database
    database_url="postgresql://user:pass@db.example.com:5432/prod",
    database_pool_size=50,
    database_max_overflow=20,
    database_pool_timeout=60,
    # Application
    app_name="Production API",
    app_version="2.0.0",
    environment="production",
    # GraphQL
    introspection_policy=IntrospectionPolicy.DISABLED,
    enable_playground=False,
    max_query_depth=10,
    query_timeout=15,
    # Performance
    enable_query_caching=True,
    cache_ttl=600,
    enable_turbo_router=True,
    jsonb_extraction_enabled=True,
    # Auth
    auth_enabled=True,
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com",
    auth0_api_identifier="https://api.myapp.com",
    # CORS
    cors_enabled=True,
    cors_origins=["https://app.example.com"],
    # Rate Limiting
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=30,
    # Complexity
    complexity_enabled=True,
    complexity_max_score=500,
)
