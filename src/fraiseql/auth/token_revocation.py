"""Token revocation mechanism for FraiseQL.

This module provides functionality to revoke JWT tokens before they expire,
supporting both in-memory and Redis-backed storage for revocation lists.
"""

import asyncio
import logging
import time
from collections import defaultdict
from dataclasses import dataclass
from typing import Any, Optional, Protocol

from redis.asyncio import Redis

from fraiseql.audit import get_security_logger
from fraiseql.audit.security_logger import SecurityEvent, SecurityEventSeverity, SecurityEventType

from .base import InvalidTokenError

logger = logging.getLogger(__name__)


class RevocationStore(Protocol):
    """Protocol for token revocation stores."""

    async def revoke_token(self, token_id: str, user_id: str) -> None:
        """Revoke a specific token."""
        ...

    async def is_revoked(self, token_id: str) -> bool:
        """Check if a token is revoked."""
        ...

    async def revoke_all_user_tokens(self, user_id: str) -> None:
        """Revoke all tokens for a user."""
        ...

    async def cleanup_expired(self) -> int:
        """Clean up expired revocations. Returns number cleaned."""
        ...

    async def get_revoked_count(self) -> int:
        """Get count of revoked tokens."""
        ...


class InMemoryRevocationStore:
    """In-memory token revocation store for development/testing."""

    def __init__(self) -> None:
        """Initialize in-memory store."""
        # Map token_id to expiry timestamp
        self._revoked_tokens: dict[str, float] = {}
        # Map user_id to set of token_ids
        self._user_tokens: dict[str, set[str]] = defaultdict(set)
        self._lock = asyncio.Lock()

    async def revoke_token(self, token_id: str, user_id: str) -> None:
        """Revoke a specific token."""
        async with self._lock:
            # Store with expiry time (could be from token exp claim)
            # For now, use a default TTL of 24 hours
            expiry_time = time.time() + 86400
            self._revoked_tokens[token_id] = expiry_time
            self._user_tokens[user_id].add(token_id)

            logger.info(f"Revoked token {token_id} for user {user_id}")

    async def is_revoked(self, token_id: str) -> bool:
        """Check if a token is revoked."""
        async with self._lock:
            if token_id not in self._revoked_tokens:
                return False

            # Check if still valid
            expiry = self._revoked_tokens[token_id]
            if time.time() > expiry:
                # Expired, remove it
                del self._revoked_tokens[token_id]
                return False

            return True

    async def revoke_all_user_tokens(self, user_id: str) -> None:
        """Revoke all tokens for a user."""
        async with self._lock:
            # Get all tokens for this user
            user_tokens = self._user_tokens.get(user_id, set())

            # Mark them all as revoked
            expiry_time = time.time() + 86400
            for token_id in user_tokens:
                self._revoked_tokens[token_id] = expiry_time

            logger.info(f"Revoked {len(user_tokens)} tokens for user {user_id}")

    async def cleanup_expired(self) -> int:
        """Clean up expired revocations."""
        async with self._lock:
            current_time = time.time()
            expired = [
                token_id
                for token_id, expiry in self._revoked_tokens.items()
                if current_time > expiry
            ]

            for token_id in expired:
                del self._revoked_tokens[token_id]
                # Clean from user mappings
                for user_tokens in self._user_tokens.values():
                    user_tokens.discard(token_id)

            # Clean empty user entries
            empty_users = [user_id for user_id, tokens in self._user_tokens.items() if not tokens]
            for user_id in empty_users:
                del self._user_tokens[user_id]

            return len(expired)

    async def get_revoked_count(self) -> int:
        """Get count of revoked tokens."""
        async with self._lock:
            return len(self._revoked_tokens)


class RedisRevocationStore:
    """Redis-backed token revocation store for production."""

    def __init__(self, redis_client: Redis, ttl: int = 86400) -> None:
        """Initialize Redis revocation store.

        Args:
            redis_client: Redis async client
            ttl: Time-to-live for revoked tokens in seconds
        """
        self.redis = redis_client
        self.ttl = ttl
        self.key_prefix = "revoked"

    def _token_key(self, token_id: str) -> str:
        """Get Redis key for a token."""
        return f"{self.key_prefix}:token:{token_id}"

    def _user_key(self, user_id: str) -> str:
        """Get Redis key for user's tokens."""
        return f"{self.key_prefix}:user:{user_id}"

    async def revoke_token(self, token_id: str, user_id: str) -> None:
        """Revoke a specific token."""
        # Store token with TTL
        await self.redis.setex(self._token_key(token_id), self.ttl, "1")

        # Add to user's token set
        await self.redis.sadd(self._user_key(user_id), token_id)

        logger.info(f"Revoked token {token_id} for user {user_id}")

    async def is_revoked(self, token_id: str) -> bool:
        """Check if a token is revoked."""
        result = await self.redis.exists(self._token_key(token_id))
        return result > 0

    async def revoke_all_user_tokens(self, user_id: str) -> None:
        """Revoke all tokens for a user."""
        user_key = self._user_key(user_id)

        # Get all tokens for this user
        token_ids = await self.redis.smembers(user_key)

        if token_ids:
            # Revoke each token
            for token_id in token_ids:
                await self.redis.setex(self._token_key(token_id), self.ttl, "1")

            logger.info(f"Revoked {len(token_ids)} tokens for user {user_id}")

        # Delete the user set
        await self.redis.delete(user_key)

    async def cleanup_expired(self) -> int:
        """Clean up expired revocations (Redis handles this automatically)."""
        # Redis handles TTL automatically
        return 0

    async def get_revoked_count(self) -> int:
        """Get approximate count of revoked tokens."""
        # This is approximate as it counts all keys with the prefix
        count = 0
        async for _ in self.redis.scan_iter(match=f"{self.key_prefix}:token:*"):
            count += 1
        return count


@dataclass
class RevocationConfig:
    """Configuration for token revocation."""

    enabled: bool = True
    check_revocation: bool = True
    ttl: int = 86400  # 24 hours
    cleanup_interval: int = 3600  # 1 hour
    store_type: str = "memory"  # "memory" or "redis"


class TokenRevocationService:
    """Main service for handling token revocation."""

    def __init__(
        self,
        store: RevocationStore,
        config: Optional[RevocationConfig] = None,
    ) -> None:
        """Initialize revocation service.

        Args:
            store: Revocation store backend
            config: Revocation configuration
        """
        self.store = store
        self.config = config or RevocationConfig()
        self._cleanup_task: Optional[asyncio.Task] = None

    async def start(self) -> None:
        """Start the revocation service."""
        if self.config.enabled and self.config.cleanup_interval > 0:
            self._cleanup_task = asyncio.create_task(self._cleanup_loop())
            logger.info("Token revocation service started")

    async def stop(self) -> None:
        """Stop the revocation service."""
        if self._cleanup_task:
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass
            logger.info("Token revocation service stopped")

    async def revoke_token(self, token_payload: dict[str, Any]) -> None:
        """Revoke a token.

        Args:
            token_payload: Decoded JWT payload (must contain 'jti' and 'sub')
        """
        if not self.config.enabled:
            return

        token_id = token_payload.get("jti")
        user_id = token_payload.get("sub")

        if not token_id:
            raise ValueError("Token missing JTI (JWT ID) claim")
        if not user_id:
            raise ValueError("Token missing sub (subject) claim")

        await self.store.revoke_token(token_id, user_id)

        # Log security event
        security_logger = get_security_logger()
        security_logger.log_event(
            SecurityEvent(
                event_type=SecurityEventType.AUTH_LOGOUT,
                severity=SecurityEventSeverity.INFO,
                user_id=user_id,
                metadata={"token_id": token_id},
            ),
        )

    async def is_token_revoked(self, token_payload: dict[str, Any]) -> bool:
        """Check if a token is revoked.

        Args:
            token_payload: Decoded JWT payload (must contain 'jti')

        Returns:
            True if token is revoked
        """
        if not self.config.enabled or not self.config.check_revocation:
            return False

        token_id = token_payload.get("jti")
        if not token_id:
            # No JTI, can't check revocation
            return False

        return await self.store.is_revoked(token_id)

    async def revoke_all_user_tokens(self, user_id: str) -> None:
        """Revoke all tokens for a user.

        Args:
            user_id: User identifier
        """
        if not self.config.enabled:
            return

        await self.store.revoke_all_user_tokens(user_id)

        # Log security event
        security_logger = get_security_logger()
        security_logger.log_event(
            SecurityEvent(
                event_type=SecurityEventType.AUTH_LOGOUT,
                severity=SecurityEventSeverity.INFO,
                user_id=user_id,
                metadata={"action": "logout_all_sessions"},
            ),
        )

    async def get_stats(self) -> dict[str, Any]:
        """Get revocation statistics."""
        return {
            "enabled": self.config.enabled,
            "check_revocation": self.config.check_revocation,
            "revoked_tokens": await self.store.get_revoked_count(),
        }

    async def _cleanup_loop(self) -> None:
        """Periodic cleanup of expired revocations."""
        while True:
            try:
                await asyncio.sleep(self.config.cleanup_interval)
                await self._run_cleanup_once()
            except asyncio.CancelledError:
                break
            except Exception:
                logger.exception("Error in revocation cleanup")

    async def _run_cleanup_once(self) -> None:
        """Run cleanup once."""
        cleaned = await self.store.cleanup_expired()
        if cleaned > 0:
            logger.info(f"Cleaned {cleaned} expired token revocations")


class TokenRevocationMixin:
    """Mixin for auth providers to add revocation support."""

    revocation_service: Optional[TokenRevocationService] = None

    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate token with revocation check.

        This wraps the original validate_token method to add revocation checking.
        """
        # First, validate the token normally
        payload = await self._original_validate_token(token)

        # Then check if it's revoked
        if self.revocation_service and await self.revocation_service.is_token_revoked(payload):
            raise InvalidTokenError("Token has been revoked")

        return payload

    async def _original_validate_token(self, token: str) -> dict[str, Any]:
        """Original token validation (to be overridden by auth provider)."""
        raise NotImplementedError

    async def logout(self, token_payload: dict[str, Any]) -> None:
        """Logout by revoking the token."""
        if self.revocation_service:
            await self.revocation_service.revoke_token(token_payload)

    async def logout_all_sessions(self, user_id: str) -> None:
        """Logout all sessions by revoking all user tokens."""
        if self.revocation_service:
            await self.revocation_service.revoke_all_user_tokens(user_id)
