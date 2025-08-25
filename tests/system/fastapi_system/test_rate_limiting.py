"""Tests for rate limiting middleware.

Following TDD principles, these tests are written before the implementation.
"""

from unittest.mock import AsyncMock, MagicMock

import pytest
from fastapi import HTTPException, Request

from fraiseql.middleware.rate_limiter import (
    InMemoryRateLimiter,
    RateLimitConfig,
    RateLimiterMiddleware,
    RedisRateLimiter,
    SlidingWindowRateLimiter,
)


class TestRateLimitConfig:
    """Test rate limit configuration."""

    def test_default_config(self):
        """Test default rate limit configuration."""
        config = RateLimitConfig()

        assert config.enabled is True
        assert config.requests_per_minute == 60
        assert config.requests_per_hour == 1000
        assert config.burst_size == 10
        assert config.window_type == "sliding"
        assert config.key_func is None
        assert config.whitelist == []
        assert config.blacklist == []

    def test_custom_config(self):
        """Test custom rate limit configuration."""

        def custom_key_func(request):
            return request.client.host

        config = RateLimitConfig(
            enabled=True,
            requests_per_minute=30,
            requests_per_hour=500,
            burst_size=5,
            window_type="fixed",
            key_func=custom_key_func,
            whitelist=["192.168.1.1"],
            blacklist=["10.0.0.1"],
        )

        assert config.requests_per_minute == 30
        assert config.requests_per_hour == 500
        assert config.burst_size == 5
        assert config.window_type == "fixed"
        assert config.key_func == custom_key_func


class TestInMemoryRateLimiter:
    """Test in-memory rate limiter implementation."""

    @pytest.fixture
    def limiter(self):
        """Create in-memory rate limiter."""
        config = RateLimitConfig(requests_per_minute=10, requests_per_hour=100, burst_size=3)
        return InMemoryRateLimiter(config)

    @pytest.mark.asyncio
    async def test_allow_request_under_limit(self, limiter):
        """Test allowing requests under the limit."""
        key = "test_user"

        # First few requests should be allowed (within burst)
        for _i in range(3):
            allowed = await limiter.check_rate_limit(key)
            assert allowed.allowed is True
            # Remaining is min of minute/hour limits
            assert allowed.remaining >= 0
            assert allowed.reset_after > 0

    @pytest.mark.asyncio
    async def test_block_request_over_limit(self, limiter):
        """Test blocking requests over the limit."""
        key = "test_user"

        # Exhaust the minute limit (10 requests)
        for _ in range(10):
            await limiter.check_rate_limit(key)

        # Next request should be blocked
        result = await limiter.check_rate_limit(key)
        assert result.allowed is False
        assert result.remaining == 0
        assert result.retry_after > 0

    @pytest.mark.asyncio
    async def test_minute_window_reset(self, limiter):
        """Test that minute window resets properly."""
        key = "test_user"

        # Use up some requests
        for _ in range(5):
            await limiter.check_rate_limit(key)

        # Simulate time passing (would need to mock time in real implementation)
        # For now, just test the structure
        info = await limiter.get_rate_limit_info(key)
        assert info.minute_requests >= 5
        assert info.hour_requests >= 5

    @pytest.mark.asyncio
    async def test_hour_window_limit(self, limiter):
        """Test hour window limit enforcement."""

        # Simulate exhausting hour limit
        # In real implementation, would need to manipulate internal state
        # or mock time to test this properly

    @pytest.mark.asyncio
    async def test_cleanup_old_entries(self, limiter):
        """Test cleanup of old rate limit entries."""
        # Add entries for multiple keys
        for i in range(10):
            await limiter.check_rate_limit(f"user_{i}")

        # Run cleanup
        cleaned = await limiter.cleanup_expired()
        assert isinstance(cleaned, int)

    @pytest.mark.asyncio
    async def test_get_all_limited_keys(self, limiter):
        """Test getting all rate-limited keys."""
        # Create some rate limited entries
        await limiter.check_rate_limit("user1")
        await limiter.check_rate_limit("user2")

        keys = await limiter.get_limited_keys()
        assert "user1" in keys
        assert "user2" in keys


class TestRedisRateLimiter:
    """Test Redis-backed rate limiter."""

    @pytest.fixture
    def mock_redis(self):
        """Create mock Redis client."""
        mock = AsyncMock()
        mock.incr = AsyncMock(return_value=1)
        mock.expire = AsyncMock(return_value=True)
        mock.ttl = AsyncMock(return_value=60)
        mock.get = AsyncMock(return_value=None)
        mock.mget = AsyncMock(return_value=[None, None])

        # Create proper pipeline mock
        pipeline = AsyncMock()
        pipeline.incr = MagicMock()
        pipeline.expire = MagicMock()
        pipeline.ttl = MagicMock()
        pipeline.execute = AsyncMock(return_value=[1, 1, True, True, 60, 3600])
        pipeline.__aenter__ = AsyncMock(return_value=pipeline)
        pipeline.__aexit__ = AsyncMock(return_value=None)

        mock.pipeline = MagicMock(return_value=pipeline)
        mock.scan_iter = AsyncMock(return_value=[])
        return mock

    @pytest.fixture
    def limiter(self, mock_redis):
        """Create Redis rate limiter."""
        config = RateLimitConfig(requests_per_minute=10, requests_per_hour=100)
        return RedisRateLimiter(mock_redis, config)

    @pytest.mark.asyncio
    async def test_check_rate_limit_allowed(self, limiter, mock_redis):
        """Test rate limit check when allowed."""
        # Pipeline returns results for: minute_incr, hour_incr, minute_expire
        # hour_expire, minute_ttl, hour_ttl
        pipeline = mock_redis.pipeline.return_value

        result = await limiter.check_rate_limit("test_user")

        assert result.allowed is True
        assert result.remaining > 0
        assert pipeline.execute.called  # Pipeline was executed

    @pytest.mark.asyncio
    async def test_check_rate_limit_blocked(self, limiter, mock_redis):
        """Test rate limit check when blocked."""
        # Pipeline returns over-limit values
        pipeline = mock_redis.pipeline.return_value
        pipeline.execute = AsyncMock(return_value=[11, 50, True, True, 30, 3600])

        result = await limiter.check_rate_limit("test_user")

        assert result.allowed is False
        assert result.remaining == 0
        assert result.retry_after == 30

    @pytest.mark.asyncio
    async def test_sliding_window_implementation(self, limiter, mock_redis):
        """Test sliding window rate limiting in Redis."""
        # This would test the sliding window algorithm
        # Using sorted sets in Redis for precise rate limiting

    @pytest.mark.asyncio
    async def test_get_rate_limit_info(self, limiter, mock_redis):
        """Test getting rate limit info from Redis."""
        mock_redis.mget.return_value = ["5", "25"]

        info = await limiter.get_rate_limit_info("test_user")

        assert info.minute_requests == 5
        assert info.hour_requests == 25
        assert info.minute_limit == 10
        assert info.hour_limit == 100


class TestSlidingWindowRateLimiter:
    """Test sliding window rate limiter (more accurate)."""

    @pytest.fixture
    def limiter(self):
        """Create sliding window rate limiter."""
        config = RateLimitConfig(requests_per_minute=10, window_type="sliding")
        return SlidingWindowRateLimiter(config)

    @pytest.mark.asyncio
    async def test_sliding_window_accuracy(self, limiter):
        """Test that sliding window is more accurate than fixed window."""

        # Make requests spread over time
        # In sliding window, old requests should expire gradually
        # Not all at once like fixed window

    @pytest.mark.asyncio
    async def test_burst_handling(self, limiter):
        """Test handling of burst requests."""

        # Should allow burst up to configured size
        # Then enforce steady rate


class TestRateLimiterMiddleware:
    """Test rate limiter middleware for FastAPI."""

    @pytest.fixture
    def middleware(self):
        """Create rate limiter middleware."""
        config = RateLimitConfig(
            requests_per_minute=10,
            key_func=lambda req: req.client.host if req.client else "anonymous",
        )
        limiter = InMemoryRateLimiter(config)
        return RateLimiterMiddleware(app=None, rate_limiter=limiter)

    @pytest.mark.asyncio
    async def test_middleware_allows_request(self, middleware):
        """Test middleware allows requests under limit."""
        request = MagicMock(spec=Request)
        request.client.host = "127.0.0.1"
        request.url.path = "/graphql"

        call_next = AsyncMock(return_value=MagicMock())

        response = await middleware.dispatch(request, call_next)

        assert response is not None
        call_next.assert_called_once_with(request)

    @pytest.mark.asyncio
    async def test_middleware_blocks_request(self, middleware):
        """Test middleware blocks requests over limit."""
        request = MagicMock(spec=Request)
        request.client.host = "127.0.0.1"
        request.url.path = "/graphql"

        # Exhaust rate limit
        for _ in range(11):
            try:
                await middleware.dispatch(request, AsyncMock())
            except HTTPException:
                pass

        # Next request should raise 429
        with pytest.raises(HTTPException) as exc_info:
            await middleware.dispatch(request, AsyncMock())

        assert exc_info.value.status_code == 429
        assert "Rate limit exceeded" in exc_info.value.detail

    @pytest.mark.asyncio
    async def test_middleware_whitelist(self, middleware):
        """Test that whitelisted IPs bypass rate limiting."""
        middleware.rate_limiter.config.whitelist = ["192.168.1.1"]

        request = MagicMock(spec=Request)
        request.client.host = "192.168.1.1"
        request.url.path = "/graphql"

        # Should always allow whitelisted IPs
        for _ in range(100):
            response = await middleware.dispatch(request, AsyncMock())
            assert response is not None

    @pytest.mark.asyncio
    async def test_middleware_blacklist(self, middleware):
        """Test that blacklisted IPs are always blocked."""
        middleware.rate_limiter.config.blacklist = ["10.0.0.1"]

        request = MagicMock(spec=Request)
        request.client.host = "10.0.0.1"
        request.url.path = "/graphql"

        with pytest.raises(HTTPException) as exc_info:
            await middleware.dispatch(request, AsyncMock())

        assert exc_info.value.status_code == 403
        assert "Forbidden" in exc_info.value.detail

    @pytest.mark.asyncio
    async def test_middleware_headers(self, middleware):
        """Test that rate limit headers are added to response."""
        request = MagicMock(spec=Request)
        request.client.host = "127.0.0.1"
        request.url.path = "/graphql"

        response = MagicMock()
        response.headers = {}

        call_next = AsyncMock(return_value=response)

        result = await middleware.dispatch(request, call_next)

        # Should add rate limit headers
        assert "X-RateLimit-Limit" in result.headers
        assert "X-RateLimit-Remaining" in result.headers
        assert "X-RateLimit-Reset" in result.headers

    @pytest.mark.asyncio
    async def test_custom_key_function(self):
        """Test custom key function for rate limiting."""

        # Key by user ID instead of IP
        def user_key_func(request):
            return getattr(request.state, "user_id", "anonymous")

        config = RateLimitConfig(key_func=user_key_func)
        limiter = InMemoryRateLimiter(config)
        middleware = RateLimiterMiddleware(app=None, rate_limiter=limiter)

        request = MagicMock(spec=Request)
        request.state.user_id = "user123"
        request.url.path = "/graphql"

        # Should use user ID as key
        result = await middleware.dispatch(request, AsyncMock())
        assert result is not None


class TestRateLimitIntegration:
    """Test rate limiting integration with FraiseQL."""

    @pytest.mark.asyncio
    async def test_graphql_query_rate_limiting(self):
        """Test rate limiting applied to GraphQL queries."""
        # This would test actual integration with GraphQL endpoint

    @pytest.mark.asyncio
    async def test_rate_limit_by_authenticated_user(self):
        """Test rate limiting by authenticated user instead of IP."""
        # Different limits for authenticated vs anonymous users

    @pytest.mark.asyncio
    async def test_rate_limit_info_in_graphql_response(self):
        """Test including rate limit info in GraphQL response extensions."""
        # Useful for clients to know their rate limit status
