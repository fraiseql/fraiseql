"""Tests for token revocation mechanism.

Following TDD principles, these tests are written before implementation.
"""

import time
from datetime import UTC, datetime, timedelta
from unittest.mock import AsyncMock

import pytest

from fraiseql.auth.base import InvalidTokenError
from fraiseql.auth.token_revocation import (
    InMemoryRevocationStore,
    RedisRevocationStore,
    RevocationConfig,
    TokenRevocationMixin,
    TokenRevocationService,
)


class TestInMemoryRevocationStore:
    """Test in-memory token revocation store."""

    @pytest.mark.asyncio
    async def test_revoke_token(self):
        """Test revoking a token."""
        store = InMemoryRevocationStore()
        token_id = "token123"
        user_id = "user456"

        await store.revoke_token(token_id, user_id)

        is_revoked = await store.is_revoked(token_id)
        assert is_revoked is True

    @pytest.mark.asyncio
    async def test_token_not_revoked(self):
        """Test checking non-revoked token."""
        store = InMemoryRevocationStore()

        is_revoked = await store.is_revoked("nonexistent")
        assert is_revoked is False

    @pytest.mark.asyncio
    async def test_revoke_all_user_tokens(self):
        """Test revoking all tokens for a user."""
        store = InMemoryRevocationStore()
        user_id = "user789"

        # Revoke multiple tokens for the user
        await store.revoke_token("token1", user_id)
        await store.revoke_token("token2", user_id)
        await store.revoke_token("token3", "other_user")

        await store.revoke_all_user_tokens(user_id)

        # Check that user's tokens are revoked
        assert await store.is_revoked("token1") is True
        assert await store.is_revoked("token2") is True
        # Other user's token should not be affected
        assert await store.is_revoked("token3") is True  # Still revoked from before

    @pytest.mark.asyncio
    async def test_cleanup_expired(self):
        """Test cleanup of expired revocations."""
        store = InMemoryRevocationStore()

        # Add tokens with different expiry times
        now = time.time()
        store._revoked_tokens["expired1"] = now - 3600  # Expired 1 hour ago
        store._revoked_tokens["expired2"] = now - 7200  # Expired 2 hours ago
        store._revoked_tokens["valid"] = now + 3600  # Expires in 1 hour

        # Also add user mapping
        store._user_tokens["user1"] = {"expired1", "valid"}

        cleaned = await store.cleanup_expired()

        assert cleaned == 2
        assert "expired1" not in store._revoked_tokens
        assert "expired2" not in store._revoked_tokens
        assert "valid" in store._revoked_tokens

        # User mapping should be cleaned too
        assert "expired1" not in store._user_tokens["user1"]
        assert "valid" in store._user_tokens["user1"]

    @pytest.mark.asyncio
    async def test_get_revoked_count(self):
        """Test getting count of revoked tokens."""
        store = InMemoryRevocationStore()

        await store.revoke_token("token1", "user1")
        await store.revoke_token("token2", "user2")

        count = await store.get_revoked_count()
        assert count == 2


class TestRedisRevocationStore:
    """Test Redis-backed token revocation store."""

    @pytest.fixture
    def mock_redis(self):
        """Create mock Redis client."""
        mock = AsyncMock()
        mock.setex = AsyncMock(return_value=True)
        mock.exists = AsyncMock(return_value=0)
        mock.sadd = AsyncMock(return_value=1)
        mock.smembers = AsyncMock(return_value=set())
        mock.delete = AsyncMock(return_value=1)
        mock.scan_iter = AsyncMock(return_value=[])
        mock.ttl = AsyncMock(return_value=-2)  # Key doesn't exist
        mock.dbsize = AsyncMock(return_value=0)
        return mock

    @pytest.mark.asyncio
    async def test_revoke_token(self, mock_redis):
        """Test revoking a token in Redis."""
        store = RedisRevocationStore(mock_redis, ttl=3600)
        token_id = "token123"
        user_id = "user456"

        await store.revoke_token(token_id, user_id)

        # Verify Redis calls
        mock_redis.setex.assert_called_once()
        call_args = mock_redis.setex.call_args
        assert call_args[0][0] == "revoked:token:token123"
        assert call_args[0][1] == 3600
        assert call_args[0][2] == "1"

        # Verify user set update
        mock_redis.sadd.assert_called_once_with("revoked:user:user456", "token123")

    @pytest.mark.asyncio
    async def test_is_revoked_true(self, mock_redis):
        """Test checking revoked token in Redis."""
        mock_redis.exists.return_value = 1
        store = RedisRevocationStore(mock_redis)

        is_revoked = await store.is_revoked("token123")

        assert is_revoked is True
        mock_redis.exists.assert_called_once_with("revoked:token:token123")

    @pytest.mark.asyncio
    async def test_is_revoked_false(self, mock_redis):
        """Test checking non-revoked token in Redis."""
        mock_redis.exists.return_value = 0
        store = RedisRevocationStore(mock_redis)

        is_revoked = await store.is_revoked("token123")

        assert is_revoked is False

    @pytest.mark.asyncio
    async def test_revoke_all_user_tokens(self, mock_redis):
        """Test revoking all tokens for a user in Redis."""
        mock_redis.smembers.return_value = {"token1", "token2", "token3"}
        store = RedisRevocationStore(mock_redis, ttl=3600)

        await store.revoke_all_user_tokens("user123")

        # Should get user's tokens
        mock_redis.smembers.assert_called_once_with("revoked:user:user123")

        # Should revoke each token
        assert mock_redis.setex.call_count == 3

        # Should delete the user set
        mock_redis.delete.assert_called_once_with("revoked:user:user123")

    @pytest.mark.asyncio
    async def test_cleanup_not_supported(self, mock_redis):
        """Test that Redis cleanup returns 0 (Redis handles TTL)."""
        store = RedisRevocationStore(mock_redis)

        cleaned = await store.cleanup_expired()

        assert cleaned == 0  # Redis handles expiry automatically


class TestTokenRevocationService:
    """Test the main token revocation service."""

    @pytest.fixture
    def mock_store(self):
        """Create mock revocation store."""
        return AsyncMock()

    @pytest.fixture
    def revocation_config(self):
        """Create revocation configuration."""
        return RevocationConfig(enabled=True, check_revocation=True, ttl=3600, cleanup_interval=300)

    @pytest.mark.asyncio
    async def test_revoke_token(self, mock_store, revocation_config):
        """Test revoking a token through service."""
        service = TokenRevocationService(store=mock_store, config=revocation_config)

        # Mock token payload
        token_payload = {
            "jti": "token123",
            "sub": "user456",
            "exp": int((datetime.now(UTC) + timedelta(hours=1)).timestamp()),
        }

        await service.revoke_token(token_payload)

        mock_store.revoke_token.assert_called_once_with("token123", "user456")

    @pytest.mark.asyncio
    async def test_revoke_token_missing_jti(self, mock_store, revocation_config):
        """Test revoking token without JTI fails."""
        service = TokenRevocationService(store=mock_store, config=revocation_config)

        token_payload = {"sub": "user456"}  # Missing jti

        with pytest.raises(ValueError, match="Token missing JTI"):
            await service.revoke_token(token_payload)

    @pytest.mark.asyncio
    async def test_is_token_revoked(self, mock_store, revocation_config):
        """Test checking if token is revoked."""
        mock_store.is_revoked.return_value = True
        service = TokenRevocationService(store=mock_store, config=revocation_config)

        token_payload = {"jti": "token123"}

        is_revoked = await service.is_token_revoked(token_payload)

        assert is_revoked is True
        mock_store.is_revoked.assert_called_once_with("token123")

    @pytest.mark.asyncio
    async def test_is_token_revoked_disabled(self, mock_store):
        """Test revocation check when disabled."""
        config = RevocationConfig(enabled=True, check_revocation=False)
        service = TokenRevocationService(store=mock_store, config=config)

        token_payload = {"jti": "token123"}

        is_revoked = await service.is_token_revoked(token_payload)

        assert is_revoked is False
        mock_store.is_revoked.assert_not_called()

    @pytest.mark.asyncio
    async def test_revoke_all_user_tokens(self, mock_store, revocation_config):
        """Test revoking all tokens for a user."""
        service = TokenRevocationService(store=mock_store, config=revocation_config)

        await service.revoke_all_user_tokens("user123")

        mock_store.revoke_all_user_tokens.assert_called_once_with("user123")

    @pytest.mark.asyncio
    async def test_cleanup_task(self, mock_store, revocation_config):
        """Test periodic cleanup task."""
        mock_store.cleanup_expired.return_value = 5
        service = TokenRevocationService(store=mock_store, config=revocation_config)

        # Run cleanup once
        await service._run_cleanup_once()

        mock_store.cleanup_expired.assert_called_once()

    @pytest.mark.asyncio
    async def test_get_stats(self, mock_store, revocation_config):
        """Test getting revocation statistics."""
        mock_store.get_revoked_count.return_value = 42
        service = TokenRevocationService(store=mock_store, config=revocation_config)

        stats = await service.get_stats()

        assert stats["revoked_tokens"] == 42
        assert stats["enabled"] is True
        assert stats["check_revocation"] is True


class TestTokenRevocationMixin:
    """Test the mixin for auth providers."""

    class MockAuthProvider(TokenRevocationMixin):
        """Mock auth provider with revocation support."""

        def __init__(self, revocation_service):
            self.revocation_service = revocation_service
            self.original_validate_called = False

        async def _original_validate_token(self, token: str) -> dict:
            """Mock original token validation."""
            self.original_validate_called = True
            return {
                "jti": "token123",
                "sub": "user456",
                "exp": int((datetime.now(UTC) + timedelta(hours=1)).timestamp()),
            }

    @pytest.mark.asyncio
    async def test_validate_with_revocation_check(self):
        """Test token validation with revocation check."""
        mock_service = AsyncMock()
        mock_service.is_token_revoked.return_value = False

        provider = self.MockAuthProvider(mock_service)

        payload = await provider.validate_token("dummy_token")

        assert provider.original_validate_called is True
        assert payload["jti"] == "token123"
        mock_service.is_token_revoked.assert_called_once()

    @pytest.mark.asyncio
    async def test_validate_revoked_token(self):
        """Test validation fails for revoked token."""
        mock_service = AsyncMock()
        mock_service.is_token_revoked.return_value = True

        provider = self.MockAuthProvider(mock_service)

        with pytest.raises(InvalidTokenError, match="Token has been revoked"):
            await provider.validate_token("dummy_token")

    @pytest.mark.asyncio
    async def test_logout(self):
        """Test logout revokes token."""
        mock_service = AsyncMock()
        provider = self.MockAuthProvider(mock_service)

        token_payload = {"jti": "token123", "sub": "user456"}

        await provider.logout(token_payload)

        mock_service.revoke_token.assert_called_once_with(token_payload)

    @pytest.mark.asyncio
    async def test_logout_all_sessions(self):
        """Test logout all sessions revokes all user tokens."""
        mock_service = AsyncMock()
        provider = self.MockAuthProvider(mock_service)

        await provider.logout_all_sessions("user456")

        mock_service.revoke_all_user_tokens.assert_called_once_with("user456")
