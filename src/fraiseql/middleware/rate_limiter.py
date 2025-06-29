"""Rate limiting middleware for FraiseQL.

This module provides rate limiting functionality to prevent API abuse
and ensure fair usage of resources.
"""

import asyncio
import time
from collections import defaultdict, deque
from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Protocol, Set

from fastapi import HTTPException, Request, Response
from redis.asyncio import Redis
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.types import ASGIApp

from fraiseql.audit import get_security_logger
from fraiseql.audit.security_logger import SecurityEvent, SecurityEventSeverity, SecurityEventType


class RateLimitExceeded(HTTPException):
    """Raised when rate limit is exceeded."""

    def __init__(self, retry_after: int, detail: str = "Rate limit exceeded"):
        """Initialize rate limit exception."""
        super().__init__(
            status_code=429,
            detail=detail,
            headers={"Retry-After": str(retry_after)},
        )


@dataclass
class RateLimitInfo:
    """Information about current rate limit status."""

    allowed: bool
    remaining: int
    reset_after: int  # Seconds until reset
    retry_after: int = 0  # Seconds to wait if blocked
    minute_requests: int = 0
    hour_requests: int = 0
    minute_limit: int = 0
    hour_limit: int = 0


@dataclass
class RateLimitConfig:
    """Configuration for rate limiting."""

    # Whether rate limiting is enabled
    enabled: bool = True

    # Request limits
    requests_per_minute: int = 60
    requests_per_hour: int = 1000

    # Burst size (allows short bursts above steady rate)
    burst_size: int = 10

    # Window type: "sliding" or "fixed"
    window_type: str = "sliding"

    # Custom key function to identify clients
    key_func: Optional[Callable[[Request], str]] = None

    # IP whitelist (never rate limited)
    whitelist: List[str] = field(default_factory=list)

    # IP blacklist (always blocked)
    blacklist: List[str] = field(default_factory=list)


class RateLimiter(Protocol):
    """Protocol for rate limiter implementations."""

    async def check_rate_limit(self, key: str) -> RateLimitInfo:
        """Check if request is allowed under rate limit."""
        ...

    async def get_rate_limit_info(self, key: str) -> RateLimitInfo:
        """Get current rate limit status for a key."""
        ...

    async def cleanup_expired(self) -> int:
        """Clean up expired entries."""
        ...


class InMemoryRateLimiter:
    """In-memory rate limiter for development/single instance."""

    def __init__(self, config: RateLimitConfig):
        """Initialize in-memory rate limiter."""
        self.config = config
        self._minute_windows: Dict[str, deque] = defaultdict(deque)
        self._hour_windows: Dict[str, deque] = defaultdict(deque)
        self._lock = asyncio.Lock()

    async def check_rate_limit(self, key: str) -> RateLimitInfo:
        """Check if request is allowed under rate limit."""
        async with self._lock:
            now = time.time()

            # Clean old entries
            self._clean_window(self._minute_windows[key], now - 60)
            self._clean_window(self._hour_windows[key], now - 3600)

            minute_count = len(self._minute_windows[key])
            hour_count = len(self._hour_windows[key])

            # Check blacklist
            if key in self.config.blacklist:
                return RateLimitInfo(
                    allowed=False,
                    remaining=0,
                    reset_after=3600,
                    retry_after=3600,
                    minute_requests=minute_count,
                    hour_requests=hour_count,
                    minute_limit=0,
                    hour_limit=0,
                )

            # Check whitelist
            if key in self.config.whitelist:
                return RateLimitInfo(
                    allowed=True,
                    remaining=999999,
                    reset_after=0,
                    minute_requests=minute_count,
                    hour_requests=hour_count,
                    minute_limit=999999,
                    hour_limit=999999,
                )

            # Check burst allowance
            if minute_count < self.config.burst_size:
                allowed = True
            # Check minute limit
            elif (
                minute_count >= self.config.requests_per_minute
                or hour_count >= self.config.requests_per_hour
            ):
                allowed = False
            else:
                allowed = True

            if allowed:
                # Record request
                self._minute_windows[key].append(now)
                self._hour_windows[key].append(now)

                remaining_minute = max(0, self.config.requests_per_minute - minute_count - 1)
                remaining_hour = max(0, self.config.requests_per_hour - hour_count - 1)
                remaining = min(remaining_minute, remaining_hour)

                # Time until oldest request expires
                reset_after = 0
                if self._minute_windows[key]:
                    reset_after = int(60 - (now - self._minute_windows[key][0]))
            else:
                remaining = 0

                # Calculate retry after
                if minute_count >= self.config.requests_per_minute:
                    retry_after = int(60 - (now - self._minute_windows[key][0]))
                else:
                    retry_after = int(3600 - (now - self._hour_windows[key][0]))

                reset_after = retry_after

                # Log rate limit event
                security_logger = get_security_logger()
                security_logger.log_event(
                    SecurityEvent(
                        event_type=SecurityEventType.RATE_LIMIT_EXCEEDED,
                        severity=SecurityEventSeverity.WARNING,
                        metadata={
                            "key": key,
                            "minute_requests": minute_count,
                            "hour_requests": hour_count,
                        },
                    ),
                )

                return RateLimitInfo(
                    allowed=False,
                    remaining=0,
                    reset_after=reset_after,
                    retry_after=retry_after,
                    minute_requests=minute_count,
                    hour_requests=hour_count,
                    minute_limit=self.config.requests_per_minute,
                    hour_limit=self.config.requests_per_hour,
                )

            return RateLimitInfo(
                allowed=True,
                remaining=remaining,
                reset_after=reset_after,
                minute_requests=minute_count + 1,
                hour_requests=hour_count + 1,
                minute_limit=self.config.requests_per_minute,
                hour_limit=self.config.requests_per_hour,
            )

    async def get_rate_limit_info(self, key: str) -> RateLimitInfo:
        """Get current rate limit status without incrementing."""
        async with self._lock:
            now = time.time()

            # Clean old entries
            self._clean_window(self._minute_windows[key], now - 60)
            self._clean_window(self._hour_windows[key], now - 3600)

            minute_count = len(self._minute_windows[key])
            hour_count = len(self._hour_windows[key])

            remaining_minute = max(0, self.config.requests_per_minute - minute_count)
            remaining_hour = max(0, self.config.requests_per_hour - hour_count)
            remaining = min(remaining_minute, remaining_hour)

            reset_after = 0
            if self._minute_windows[key]:
                reset_after = int(60 - (now - self._minute_windows[key][0]))

            return RateLimitInfo(
                allowed=remaining > 0,
                remaining=remaining,
                reset_after=reset_after,
                minute_requests=minute_count,
                hour_requests=hour_count,
                minute_limit=self.config.requests_per_minute,
                hour_limit=self.config.requests_per_hour,
            )

    async def cleanup_expired(self) -> int:
        """Clean up expired entries."""
        async with self._lock:
            now = time.time()
            cleaned = 0

            # Clean empty windows
            empty_keys = []
            for key, window in self._minute_windows.items():
                self._clean_window(window, now - 60)
                if not window:
                    empty_keys.append(key)

            for key in empty_keys:
                del self._minute_windows[key]
                cleaned += 1

            empty_keys = []
            for key, window in self._hour_windows.items():
                self._clean_window(window, now - 3600)
                if not window:
                    empty_keys.append(key)

            for key in empty_keys:
                del self._hour_windows[key]
                cleaned += 1

            return cleaned

    async def get_limited_keys(self) -> Set[str]:
        """Get all currently rate-limited keys."""
        async with self._lock:
            return set(self._minute_windows.keys()) | set(self._hour_windows.keys())

    def _clean_window(self, window: deque, cutoff: float) -> None:
        """Remove entries older than cutoff time."""
        while window and window[0] < cutoff:
            window.popleft()


class RedisRateLimiter:
    """Redis-backed rate limiter for distributed systems."""

    def __init__(self, redis: Redis, config: RateLimitConfig):
        """Initialize Redis rate limiter."""
        self.redis = redis
        self.config = config
        self.key_prefix = "rate_limit"

    def _minute_key(self, key: str) -> str:
        """Get Redis key for minute window."""
        return f"{self.key_prefix}:minute:{key}"

    def _hour_key(self, key: str) -> str:
        """Get Redis key for hour window."""
        return f"{self.key_prefix}:hour:{key}"

    async def check_rate_limit(self, key: str) -> RateLimitInfo:
        """Check if request is allowed under rate limit."""
        # Check blacklist
        if key in self.config.blacklist:
            return RateLimitInfo(
                allowed=False,
                remaining=0,
                reset_after=3600,
                retry_after=3600,
            )

        # Check whitelist
        if key in self.config.whitelist:
            return RateLimitInfo(
                allowed=True,
                remaining=999999,
                reset_after=0,
            )

        # Use pipeline for atomic operations
        async with self.redis.pipeline(transaction=True) as pipe:
            minute_key = self._minute_key(key)
            hour_key = self._hour_key(key)

            # Increment counters
            pipe.incr(minute_key)
            pipe.incr(hour_key)

            # Set expiry if new
            pipe.expire(minute_key, 60)
            pipe.expire(hour_key, 3600)

            # Get TTLs
            pipe.ttl(minute_key)
            pipe.ttl(hour_key)

            results = await pipe.execute()

            minute_count = results[0]
            hour_count = results[1]
            minute_ttl = results[4]
            hour_ttl = results[5]

        # Check limits
        if minute_count <= self.config.burst_size:
            allowed = True
        elif (
            minute_count > self.config.requests_per_minute
            or hour_count > self.config.requests_per_hour
        ):
            allowed = False
        else:
            allowed = True

        if allowed:
            remaining_minute = max(0, self.config.requests_per_minute - minute_count)
            remaining_hour = max(0, self.config.requests_per_hour - hour_count)
            remaining = min(remaining_minute, remaining_hour)
            reset_after = minute_ttl
        else:
            remaining = 0

            if minute_count > self.config.requests_per_minute:
                retry_after = minute_ttl
            else:
                retry_after = hour_ttl

            reset_after = retry_after

            # Log rate limit event
            security_logger = get_security_logger()
            security_logger.log_event(
                SecurityEvent(
                    event_type=SecurityEventType.RATE_LIMIT_EXCEEDED,
                    severity=SecurityEventSeverity.WARNING,
                    metadata={
                        "key": key,
                        "minute_requests": minute_count,
                        "hour_requests": hour_count,
                    },
                ),
            )

            return RateLimitInfo(
                allowed=False,
                remaining=0,
                reset_after=reset_after,
                retry_after=retry_after,
                minute_requests=minute_count,
                hour_requests=hour_count,
                minute_limit=self.config.requests_per_minute,
                hour_limit=self.config.requests_per_hour,
            )

        return RateLimitInfo(
            allowed=True,
            remaining=remaining,
            reset_after=reset_after,
            minute_requests=minute_count,
            hour_requests=hour_count,
            minute_limit=self.config.requests_per_minute,
            hour_limit=self.config.requests_per_hour,
        )

    async def get_rate_limit_info(self, key: str) -> RateLimitInfo:
        """Get current rate limit status without incrementing."""
        minute_key = self._minute_key(key)
        hour_key = self._hour_key(key)

        # Get current counts
        results = await self.redis.mget(minute_key, hour_key)
        minute_count = int(results[0] or 0)
        hour_count = int(results[1] or 0)

        # Get TTLs
        minute_ttl = await self.redis.ttl(minute_key)
        minute_ttl = max(minute_ttl, 0)

        remaining_minute = max(0, self.config.requests_per_minute - minute_count)
        remaining_hour = max(0, self.config.requests_per_hour - hour_count)
        remaining = min(remaining_minute, remaining_hour)

        return RateLimitInfo(
            allowed=remaining > 0,
            remaining=remaining,
            reset_after=minute_ttl,
            minute_requests=minute_count,
            hour_requests=hour_count,
            minute_limit=self.config.requests_per_minute,
            hour_limit=self.config.requests_per_hour,
        )

    async def cleanup_expired(self) -> int:
        """Redis handles expiry automatically."""
        return 0


class SlidingWindowRateLimiter(InMemoryRateLimiter):
    """Sliding window rate limiter for more accurate rate limiting."""

    # Inherits most functionality from InMemoryRateLimiter
    # The deque-based implementation already provides sliding window behavior


class RateLimiterMiddleware(BaseHTTPMiddleware):
    """FastAPI middleware for rate limiting."""

    def __init__(self, app: ASGIApp, rate_limiter: RateLimiter):
        """Initialize rate limiter middleware."""
        super().__init__(app)
        self.rate_limiter = rate_limiter

    async def dispatch(self, request: Request, call_next) -> Response:
        """Process request with rate limiting."""
        # Skip rate limiting for certain paths
        if request.url.path in ["/health", "/metrics", "/"]:
            return await call_next(request)

        # Get client key
        if hasattr(self.rate_limiter, "config") and self.rate_limiter.config.key_func:
            key = self.rate_limiter.config.key_func(request)
        else:
            # Default to IP address
            key = request.client.host if request.client else "anonymous"

        # Check blacklist first
        if hasattr(self.rate_limiter, "config") and key in self.rate_limiter.config.blacklist:
            raise HTTPException(status_code=403, detail="Forbidden")

        # Check rate limit
        rate_limit_info = await self.rate_limiter.check_rate_limit(key)

        if not rate_limit_info.allowed:
            raise RateLimitExceeded(
                retry_after=rate_limit_info.retry_after,
                detail=f"Rate limit exceeded. Retry after {rate_limit_info.retry_after} seconds.",
            )

        # Process request
        response = await call_next(request)

        # Add rate limit headers
        response.headers["X-RateLimit-Limit"] = str(rate_limit_info.minute_limit)
        response.headers["X-RateLimit-Remaining"] = str(rate_limit_info.remaining)
        response.headers["X-RateLimit-Reset"] = str(int(time.time()) + rate_limit_info.reset_after)

        return response
