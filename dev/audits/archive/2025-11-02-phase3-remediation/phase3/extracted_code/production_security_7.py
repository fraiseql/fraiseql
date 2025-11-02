# Extracted from: docs/production/security.md
# Block number: 7
import redis.asyncio as redis

from fraiseql.security import RateLimit, RateLimitRule, setup_rate_limiting

# Redis client
redis_client = redis.from_url("redis://localhost:6379/0")

# Rate limit rules
rate_limits = [
    # GraphQL endpoint
    RateLimitRule(
        path_pattern="/graphql",
        rate_limit=RateLimit(requests=100, window=60),  # 100/min
        message="GraphQL rate limit exceeded",
    ),
    # Authentication endpoints
    RateLimitRule(
        path_pattern="/auth/login",
        rate_limit=RateLimit(requests=5, window=300),  # 5 per 5 min
        message="Too many login attempts",
    ),
    RateLimitRule(
        path_pattern="/auth/register",
        rate_limit=RateLimit(requests=3, window=3600),  # 3 per hour
        message="Too many registration attempts",
    ),
    # Mutations
    RateLimitRule(
        path_pattern="/graphql",
        rate_limit=RateLimit(requests=20, window=60),  # 20/min for mutations
        http_methods=["POST"],
        message="Mutation rate limit exceeded",
    ),
]

# Setup rate limiting
setup_rate_limiting(app=app, redis_client=redis_client, custom_rules=rate_limits)
