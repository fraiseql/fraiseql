"""Tests for token revocation module."""

import time
from typing import Any
from unittest.mock import MagicMock, patch

import pytest

from fraiseql.auth.token_revocation import (
    InMemoryRevocationStore,
    RevocationConfig,
    TokenRevocationMixin,
    TokenRevocationService,
)


# Tests for RevocationConfig
@pytest.mark.unit
@pytest.mark.security
class TestRevocationConfig:
    """Tests for RevocationConfig dataclass."""

    def test_revocation_config_defaults(self) -> None:
        """RevocationConfig has sensible defaults."""
        config = RevocationConfig()

        assert config.enabled is True
        assert config.check_revocation is True
        assert config.ttl == 86400  # 24 hours
        assert config.cleanup_interval == 3600  # 1 hour

    def test_revocation_config_custom_values(self) -> None:
        """RevocationConfig accepts custom values."""
        config = RevocationConfig(
            enabled=False,
            check_revocation=False,
            ttl=3600,
            cleanup_interval=600,
        )

        assert config.enabled is False
        assert config.check_revocation is False
        assert config.ttl == 3600
        assert config.cleanup_interval == 600


# Tests for InMemoryRevocationStore
@pytest.mark.unit
@pytest.mark.security
class TestInMemoryRevocationStore:
    """Tests for InMemoryRevocationStore class."""

    @pytest.fixture
    def store(self) -> InMemoryRevocationStore:
        """Create fresh in-memory store."""
        return InMemoryRevocationStore()

    @pytest.mark.asyncio
    async def test_revoke_token_adds_to_store(self, store: InMemoryRevocationStore) -> None:
        """revoke_token adds token to revocation store."""
        await store.revoke_token("token_123", "user_456")

        assert "token_123" in store._revoked_tokens
        assert "token_123" in store._user_tokens["user_456"]

    @pytest.mark.asyncio
    async def test_is_revoked_returns_true_for_revoked(
        self, store: InMemoryRevocationStore
    ) -> None:
        """is_revoked returns True for revoked tokens."""
        await store.revoke_token("token_123", "user_456")

        result = await store.is_revoked("token_123")

        assert result is True

    @pytest.mark.asyncio
    async def test_is_revoked_returns_false_for_not_revoked(
        self, store: InMemoryRevocationStore
    ) -> None:
        """is_revoked returns False for tokens not revoked."""
        result = await store.is_revoked("token_not_exists")

        assert result is False

    @pytest.mark.asyncio
    async def test_is_revoked_returns_false_for_expired(
        self, store: InMemoryRevocationStore
    ) -> None:
        """is_revoked returns False when revocation has expired."""
        # Manually add a token with past expiry
        store._revoked_tokens["expired_token"] = time.time() - 100

        result = await store.is_revoked("expired_token")

        assert result is False
        # Should also be cleaned up
        assert "expired_token" not in store._revoked_tokens

    @pytest.mark.asyncio
    async def test_revoke_all_user_tokens(self, store: InMemoryRevocationStore) -> None:
        """revoke_all_user_tokens revokes all tokens for a user."""
        # Add some tokens for the user
        await store.revoke_token("token_1", "user_123")
        await store.revoke_token("token_2", "user_123")
        await store.revoke_token("token_3", "user_123")

        # Update expiry for all
        await store.revoke_all_user_tokens("user_123")

        assert await store.is_revoked("token_1")
        assert await store.is_revoked("token_2")
        assert await store.is_revoked("token_3")

    @pytest.mark.asyncio
    async def test_cleanup_expired_removes_old_tokens(self, store: InMemoryRevocationStore) -> None:
        """cleanup_expired removes expired revocations."""
        # Add some expired tokens
        store._revoked_tokens["expired_1"] = time.time() - 100
        store._revoked_tokens["expired_2"] = time.time() - 50
        store._revoked_tokens["valid"] = time.time() + 3600
        store._user_tokens["user1"] = {"expired_1", "expired_2"}
        store._user_tokens["user2"] = {"valid"}

        cleaned = await store.cleanup_expired()

        assert cleaned == 2
        assert "expired_1" not in store._revoked_tokens
        assert "expired_2" not in store._revoked_tokens
        assert "valid" in store._revoked_tokens

    @pytest.mark.asyncio
    async def test_cleanup_expired_cleans_empty_user_entries(
        self, store: InMemoryRevocationStore
    ) -> None:
        """cleanup_expired removes empty user token sets."""
        store._revoked_tokens["expired_1"] = time.time() - 100
        store._user_tokens["user1"] = {"expired_1"}

        await store.cleanup_expired()

        assert "user1" not in store._user_tokens

    @pytest.mark.asyncio
    async def test_get_revoked_count(self, store: InMemoryRevocationStore) -> None:
        """get_revoked_count returns number of revoked tokens."""
        await store.revoke_token("token_1", "user_1")
        await store.revoke_token("token_2", "user_2")
        await store.revoke_token("token_3", "user_1")

        count = await store.get_revoked_count()

        assert count == 3


# Tests for TokenRevocationService
@pytest.mark.unit
@pytest.mark.security
class TestTokenRevocationService:
    """Tests for TokenRevocationService class."""

    @pytest.fixture
    def store(self) -> InMemoryRevocationStore:
        """Create fresh in-memory store."""
        return InMemoryRevocationStore()

    @pytest.fixture
    def config(self) -> RevocationConfig:
        """Create default config."""
        return RevocationConfig()

    @pytest.fixture
    def service(
        self, store: InMemoryRevocationStore, config: RevocationConfig
    ) -> TokenRevocationService:
        """Create revocation service."""
        return TokenRevocationService(store, config)

    def test_init_with_config(self, store: InMemoryRevocationStore) -> None:
        """TokenRevocationService stores config."""
        config = RevocationConfig(enabled=True, ttl=7200)
        service = TokenRevocationService(store, config)

        assert service.store is store
        assert service.config is config
        assert service.config.ttl == 7200

    def test_init_default_config(self, store: InMemoryRevocationStore) -> None:
        """TokenRevocationService uses default config when not provided."""
        service = TokenRevocationService(store)

        assert service.config.enabled is True
        assert service.config.ttl == 86400

    @pytest.mark.asyncio
    async def test_revoke_token_success(self, service: TokenRevocationService) -> None:
        """revoke_token revokes a token."""
        payload = {"jti": "token_123", "sub": "user_456"}

        with patch("fraiseql.auth.token_revocation.get_security_logger") as mock_logger:
            mock_logger.return_value = MagicMock()
            await service.revoke_token(payload)

        assert await service.store.is_revoked("token_123")

    @pytest.mark.asyncio
    async def test_revoke_token_missing_jti_raises(self, service: TokenRevocationService) -> None:
        """revoke_token raises ValueError when jti is missing."""
        payload: dict[str, Any] = {"sub": "user_456"}

        with pytest.raises(ValueError, match="Token missing JTI"):
            await service.revoke_token(payload)

    @pytest.mark.asyncio
    async def test_revoke_token_missing_sub_raises(self, service: TokenRevocationService) -> None:
        """revoke_token raises ValueError when sub is missing."""
        payload: dict[str, Any] = {"jti": "token_123"}

        with pytest.raises(ValueError, match="Token missing sub"):
            await service.revoke_token(payload)

    @pytest.mark.asyncio
    async def test_revoke_token_when_disabled(self, store: InMemoryRevocationStore) -> None:
        """revoke_token does nothing when disabled."""
        config = RevocationConfig(enabled=False)
        service = TokenRevocationService(store, config)
        payload = {"jti": "token_123", "sub": "user_456"}

        await service.revoke_token(payload)

        assert not await store.is_revoked("token_123")

    @pytest.mark.asyncio
    async def test_is_token_revoked_true(self, service: TokenRevocationService) -> None:
        """is_token_revoked returns True for revoked tokens."""
        await service.store.revoke_token("token_123", "user_456")
        payload = {"jti": "token_123"}

        result = await service.is_token_revoked(payload)

        assert result is True

    @pytest.mark.asyncio
    async def test_is_token_revoked_false(self, service: TokenRevocationService) -> None:
        """is_token_revoked returns False for valid tokens."""
        payload = {"jti": "token_not_revoked"}

        result = await service.is_token_revoked(payload)

        assert result is False

    @pytest.mark.asyncio
    async def test_is_token_revoked_no_jti(self, service: TokenRevocationService) -> None:
        """is_token_revoked returns False when no jti claim."""
        payload: dict[str, Any] = {"sub": "user_456"}

        result = await service.is_token_revoked(payload)

        assert result is False

    @pytest.mark.asyncio
    async def test_is_token_revoked_when_disabled(self, store: InMemoryRevocationStore) -> None:
        """is_token_revoked returns False when disabled."""
        config = RevocationConfig(enabled=False)
        service = TokenRevocationService(store, config)
        await store.revoke_token("token_123", "user_456")
        payload = {"jti": "token_123"}

        result = await service.is_token_revoked(payload)

        assert result is False

    @pytest.mark.asyncio
    async def test_is_token_revoked_when_check_disabled(
        self, store: InMemoryRevocationStore
    ) -> None:
        """is_token_revoked returns False when check_revocation is disabled."""
        config = RevocationConfig(enabled=True, check_revocation=False)
        service = TokenRevocationService(store, config)
        await store.revoke_token("token_123", "user_456")
        payload = {"jti": "token_123"}

        result = await service.is_token_revoked(payload)

        assert result is False

    @pytest.mark.asyncio
    async def test_revoke_all_user_tokens(self, service: TokenRevocationService) -> None:
        """revoke_all_user_tokens revokes all user tokens."""
        await service.store.revoke_token("token_1", "user_123")
        await service.store.revoke_token("token_2", "user_123")

        with patch("fraiseql.auth.token_revocation.get_security_logger") as mock_logger:
            mock_logger.return_value = MagicMock()
            await service.revoke_all_user_tokens("user_123")

        assert await service.store.is_revoked("token_1")
        assert await service.store.is_revoked("token_2")

    @pytest.mark.asyncio
    async def test_revoke_all_user_tokens_when_disabled(
        self, store: InMemoryRevocationStore
    ) -> None:
        """revoke_all_user_tokens does nothing when disabled."""
        config = RevocationConfig(enabled=False)
        service = TokenRevocationService(store, config)

        await service.revoke_all_user_tokens("user_123")

        # Should not raise, just do nothing

    @pytest.mark.asyncio
    async def test_get_stats(self, service: TokenRevocationService) -> None:
        """get_stats returns revocation statistics."""
        await service.store.revoke_token("token_1", "user_1")
        await service.store.revoke_token("token_2", "user_2")

        stats = await service.get_stats()

        assert stats["enabled"] is True
        assert stats["check_revocation"] is True
        assert stats["revoked_tokens"] == 2

    @pytest.mark.asyncio
    async def test_start_creates_cleanup_task(self, store: InMemoryRevocationStore) -> None:
        """start() creates background cleanup task."""
        config = RevocationConfig(enabled=True, cleanup_interval=1)
        service = TokenRevocationService(store, config)

        await service.start()

        assert service._cleanup_task is not None
        assert not service._cleanup_task.done()

        await service.stop()

    @pytest.mark.asyncio
    async def test_start_does_nothing_when_disabled(self, store: InMemoryRevocationStore) -> None:
        """start() does nothing when disabled."""
        config = RevocationConfig(enabled=False)
        service = TokenRevocationService(store, config)

        await service.start()

        assert service._cleanup_task is None

    @pytest.mark.asyncio
    async def test_stop_cancels_cleanup_task(self, store: InMemoryRevocationStore) -> None:
        """stop() cancels the cleanup task."""
        config = RevocationConfig(enabled=True, cleanup_interval=1)
        service = TokenRevocationService(store, config)

        await service.start()
        task = service._cleanup_task

        await service.stop()

        assert task is not None
        assert task.cancelled() or task.done()


# Tests for TokenRevocationMixin
@pytest.mark.unit
@pytest.mark.security
class TestTokenRevocationMixin:
    """Tests for TokenRevocationMixin class."""

    @pytest.fixture
    def mixin_class(self) -> type:
        """Create a class using the mixin."""

        class TestProvider(TokenRevocationMixin):
            def __init__(self) -> None:
                self.revocation_service: TokenRevocationService | None = None
                self._validate_called = False

            async def _original_validate_token(self, token: str) -> dict[str, Any]:
                self._validate_called = True
                return {"jti": "token_123", "sub": "user_456"}

        return TestProvider

    @pytest.mark.asyncio
    async def test_logout_calls_revoke(self, mixin_class: type) -> None:
        """logout() calls revocation service."""
        provider = mixin_class()
        store = InMemoryRevocationStore()
        config = RevocationConfig()
        service = TokenRevocationService(store, config)
        provider.revocation_service = service

        payload = {"jti": "token_123", "sub": "user_456"}

        with patch("fraiseql.auth.token_revocation.get_security_logger") as mock_logger:
            mock_logger.return_value = MagicMock()
            await provider.logout(payload)

        assert await store.is_revoked("token_123")

    @pytest.mark.asyncio
    async def test_logout_does_nothing_without_service(self, mixin_class: type) -> None:
        """logout() does nothing when no service configured."""
        provider = mixin_class()
        provider.revocation_service = None
        payload = {"jti": "token_123", "sub": "user_456"}

        await provider.logout(payload)  # Should not raise

    @pytest.mark.asyncio
    async def test_logout_all_sessions(self, mixin_class: type) -> None:
        """logout_all_sessions() revokes all user tokens."""
        provider = mixin_class()
        store = InMemoryRevocationStore()
        config = RevocationConfig()
        service = TokenRevocationService(store, config)
        provider.revocation_service = service

        await store.revoke_token("token_1", "user_123")
        await store.revoke_token("token_2", "user_123")

        with patch("fraiseql.auth.token_revocation.get_security_logger") as mock_logger:
            mock_logger.return_value = MagicMock()
            await provider.logout_all_sessions("user_123")

        assert await store.is_revoked("token_1")
        assert await store.is_revoked("token_2")

    @pytest.mark.asyncio
    async def test_logout_all_sessions_does_nothing_without_service(
        self, mixin_class: type
    ) -> None:
        """logout_all_sessions() does nothing when no service configured."""
        provider = mixin_class()
        provider.revocation_service = None

        await provider.logout_all_sessions("user_123")  # Should not raise
