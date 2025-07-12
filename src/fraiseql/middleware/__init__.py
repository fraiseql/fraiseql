"""Middleware components for FraiseQL."""

# Import non-Redis classes first
from .rate_limiter import (
    InMemoryRateLimiter,
    RateLimitConfig,
    RateLimiterMiddleware,
    RateLimitExceeded,
    RateLimitInfo,
    SlidingWindowRateLimiter,
)

# Lazy import Redis-dependent classes
try:
    from .rate_limiter import RedisRateLimiter

    _HAS_REDIS = True
except ImportError:
    _HAS_REDIS = False

    class RedisRateLimiter:
        def __init__(self, *args, **kwargs):
            raise ImportError(
                "Redis is required for RedisRateLimiter. "
                "Install it with: pip install fraiseql[redis]",
            )


__all__ = [
    "InMemoryRateLimiter",
    "RateLimitConfig",
    "RateLimitExceeded",
    "RateLimitInfo",
    "RateLimiterMiddleware",
    "RedisRateLimiter",
    "SlidingWindowRateLimiter",
]
