"""Tests for security constraints (rate limiting, IP filtering, complexity)."""
# ruff: noqa

import pytest

from fraiseql.enterprise.security import ComplexityAnalyzer, IpFilter, RateLimiter


class TestRateLimiter:
    """Test rate limiting functionality."""

    @pytest.mark.asyncio
    async def test_rate_limiter_allow(self):
        """Test rate limiter allows requests under limit."""
        limiter = RateLimiter(max_requests=10, window_seconds=60)

        # First request should be allowed
        assert await limiter.check("user:1") is True

    @pytest.mark.asyncio
    async def test_rate_limiter_block(self):
        """Test rate limiter blocks requests over limit."""
        limiter = RateLimiter(max_requests=2, window_seconds=60)

        # First 2 requests allowed
        assert await limiter.check("user:1") is True
        assert await limiter.check("user:1") is True

        # 3rd request blocked
        assert await limiter.check("user:1") is False

    @pytest.mark.asyncio
    async def test_rate_limiter_multi_user(self):
        """Test rate limiter tracks users separately."""
        limiter = RateLimiter(max_requests=2, window_seconds=60)

        # User 1: 2 requests (at limit)
        assert await limiter.check("user:1") is True
        assert await limiter.check("user:1") is True
        assert await limiter.check("user:1") is False

        # User 2: still has quota
        assert await limiter.check("user:2") is True
        assert await limiter.check("user:2") is True
        assert await limiter.check("user:2") is False

    @pytest.mark.asyncio
    async def test_rate_limiter_reset(self):
        """Test rate limiter reset functionality."""
        limiter = RateLimiter(max_requests=1, window_seconds=60)

        # Use up quota
        assert await limiter.check("user:1") is True
        assert await limiter.check("user:1") is False

        # Reset quota
        await limiter.reset("user:1")

        # Should work again
        assert await limiter.check("user:1") is True

    @pytest.mark.asyncio
    async def test_rate_limiter_different_keys(self):
        """Test rate limiter with different key types."""
        limiter = RateLimiter(max_requests=1, window_seconds=60)

        # Different key types should be tracked separately
        assert await limiter.check("user:123") is True
        assert await limiter.check("ip:192.168.1.1") is True
        assert await limiter.check("tenant:5") is True

        # Each key has its own limit
        assert await limiter.check("user:123") is False
        assert await limiter.check("ip:192.168.1.1") is False
        assert await limiter.check("tenant:5") is False


class TestIpFilter:
    """Test IP filtering functionality."""

    @pytest.mark.asyncio
    async def test_ip_filter_allowlist(self):
        """Test IP allowlist."""
        filter = IpFilter(allowlist=["192.168.1.0/24"])

        # IP in range should be allowed
        assert await filter.check("192.168.1.100") is True
        assert await filter.check("192.168.1.1") is True
        assert await filter.check("192.168.1.254") is True

        # IP out of range should be blocked
        assert await filter.check("10.0.0.1") is False
        assert await filter.check("192.168.2.1") is False

    @pytest.mark.asyncio
    async def test_ip_filter_blocklist(self):
        """Test IP blocklist."""
        filter = IpFilter(blocklist=["10.0.0.0/8"])

        # IPs not in blocklist should be allowed
        assert await filter.check("192.168.1.100") is True
        assert await filter.check("172.16.0.1") is True

        # IPs in blocklist should be blocked
        assert await filter.check("10.0.0.1") is False
        assert await filter.check("10.255.255.255") is False

    @pytest.mark.asyncio
    async def test_ip_filter_combined(self):
        """Test IP filter with both allowlist and blocklist."""
        filter = IpFilter(
            allowlist=["192.168.0.0/16"],  # Allow 192.168.*.*
            blocklist=["192.168.1.0/24"],  # But block 192.168.1.*
        )

        # Allowed by allowlist, not in blocklist
        assert await filter.check("192.168.2.100") is True
        assert await filter.check("192.168.3.1") is True

        # Blocked by blocklist (takes precedence)
        assert await filter.check("192.168.1.100") is False
        assert await filter.check("192.168.1.1") is False

        # Not in allowlist
        assert await filter.check("10.0.0.1") is False

    @pytest.mark.asyncio
    async def test_ip_filter_empty_allowlist(self):
        """Test IP filter with empty allowlist (allow all)."""
        filter = IpFilter(blocklist=["10.0.1.0/24"])

        # Everything allowed except blocklist
        assert await filter.check("192.168.1.1") is True
        assert await filter.check("172.16.0.1") is True
        assert await filter.check("10.0.2.1") is True

        # Only blocklist items blocked
        assert await filter.check("10.0.1.100") is False

    @pytest.mark.asyncio
    async def test_ip_filter_invalid_ip(self):
        """Test IP filter with invalid IP."""
        filter = IpFilter(allowlist=["192.168.1.0/24"])

        # Invalid IPs should be blocked
        assert await filter.check("invalid") is False
        assert await filter.check("999.999.999.999") is False
        assert await filter.check("") is False

    @pytest.mark.asyncio
    async def test_ip_filter_multiple_ranges(self):
        """Test IP filter with multiple CIDR ranges."""
        filter = IpFilter(allowlist=["192.168.1.0/24", "10.0.0.0/16"])

        # IPs in first range
        assert await filter.check("192.168.1.100") is True

        # IPs in second range
        assert await filter.check("10.0.1.1") is True
        assert await filter.check("10.0.255.255") is True

        # IPs in neither range
        assert await filter.check("172.16.0.1") is False


class TestComplexityAnalyzer:
    """Test query complexity analysis."""

    @pytest.mark.asyncio
    async def test_complexity_simple_query(self):
        """Test complexity analyzer with simple query."""
        analyzer = ComplexityAnalyzer(max_complexity=100)

        # Simple query (low complexity)
        simple = "{ user { id name } }"
        assert await analyzer.check(simple) is True

    @pytest.mark.asyncio
    async def test_complexity_complex_query(self):
        """Test complexity analyzer with complex query."""
        analyzer = ComplexityAnalyzer(max_complexity=50)

        # Complex nested query (high complexity)
        complex = """
        {
            users {
                posts {
                    comments {
                        author {
                            posts {
                                comments {
                                    id
                                }
                            }
                        }
                    }
                }
            }
        }
        """
        assert await analyzer.check(complex) is False

    @pytest.mark.asyncio
    async def test_complexity_threshold(self):
        """Test complexity analyzer with different thresholds."""
        # Very strict analyzer
        strict = ComplexityAnalyzer(max_complexity=20)

        # Relaxed analyzer
        relaxed = ComplexityAnalyzer(max_complexity=200)

        query = "{ users { posts { id } } }"

        # Strict should block, relaxed should allow
        assert await strict.check(query) is False
        assert await relaxed.check(query) is True

    @pytest.mark.asyncio
    async def test_complexity_flat_query(self):
        """Test complexity with flat query (many fields, no nesting)."""
        analyzer = ComplexityAnalyzer(max_complexity=100)

        # Flat query with many fields
        flat = "{ user { id name email phone address city state zip } }"
        assert await analyzer.check(flat) is True

    @pytest.mark.asyncio
    async def test_complexity_deep_nesting(self):
        """Test complexity with deeply nested query."""
        analyzer = ComplexityAnalyzer(max_complexity=50)

        # Deeply nested (depth penalty)
        deep = "{ a { b { c { d { e { f { g { h { i { j } } } } } } } } } }"
        assert await deep.check(deep) is False


class TestIntegration:
    """Integration tests combining multiple constraints."""

    @pytest.mark.asyncio
    async def test_combined_constraints(self):
        """Test using multiple constraints together."""
        # Setup all constraints
        limiter = RateLimiter(max_requests=5, window_seconds=60)
        ip_filter = IpFilter(allowlist=["192.168.0.0/16"])
        complexity = ComplexityAnalyzer(max_complexity=100)

        # Simulate request validation
        user_key = "user:123"
        client_ip = "192.168.1.100"
        query = "{ user { id name } }"

        # All checks should pass
        assert await limiter.check(user_key) is True
        assert await ip_filter.check(client_ip) is True
        assert await complexity.check(query) is True

        # Simulate blocked IP
        blocked_ip = "10.0.0.1"
        assert await ip_filter.check(blocked_ip) is False

        # Simulate rate limit
        for _ in range(4):
            await limiter.check(user_key)
        assert await limiter.check(user_key) is False

        # Simulate complex query
        complex_query = "{ users { posts { comments { author { posts { id } } } } } }"
        assert await complexity.check(complex_query) is False
