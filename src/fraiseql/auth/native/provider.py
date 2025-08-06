"""Native authentication provider for FraiseQL."""

from typing import Optional

from psycopg_pool import AsyncConnectionPool

from fraiseql.auth.base import AuthProvider, UserContext
from fraiseql.auth.native.models import User
from fraiseql.auth.native.tokens import (
    InvalidTokenError,
    SecurityError,
    TokenExpiredError,
    TokenManager,
)


class NativeAuthProvider(AuthProvider):
    """Native authentication provider using PostgreSQL and JWT tokens.

    This provider integrates the native auth system with FraiseQL's
    authentication framework, handling token validation and user context
    creation.
    """

    def __init__(
        self,
        token_manager: TokenManager,
        db_pool: AsyncConnectionPool,
        schema: str = "public",
    ):
        """Initialize the native auth provider.

        Args:
            token_manager: JWT token manager instance
            db_pool: PostgreSQL connection pool
            schema: Database schema name (for multi-tenant support)
        """
        self.token_manager = token_manager
        self.db_pool = db_pool
        self.schema = schema

    async def validate_token(self, token: str) -> dict[str, any]:
        """Validate an access token and return its payload.

        Args:
            token: JWT access token

        Returns:
            Token payload dictionary

        Raises:
            TokenExpiredError: If token is expired
            InvalidTokenError: If token is invalid
            SecurityError: If security violation is detected
        """
        return self.token_manager.verify_access_token(token)

    async def get_user_from_token(self, token: str) -> UserContext:
        """Get user context from access token.

        Args:
            token: JWT access token

        Returns:
            UserContext with user information

        Raises:
            TokenExpiredError: If token is expired
            InvalidTokenError: If token is invalid or user not found
            SecurityError: If security violation is detected
        """
        # Validate token and extract payload
        try:
            payload = await self.validate_token(token)
        except (TokenExpiredError, InvalidTokenError, SecurityError) as e:
            raise InvalidTokenError(f"Token validation failed: {e}") from e

        # Extract user ID from token
        user_id = payload.get("sub")
        if not user_id:
            raise InvalidTokenError("Token missing user ID")

        # Get user from database
        async with self.db_pool.connection() as conn, conn.cursor() as cursor:
            user = await User.get_by_id(cursor, self.schema, user_id)

        if not user:
            raise InvalidTokenError("User not found")

        if not user.is_active:
            raise InvalidTokenError("User account is disabled")

        # Build UserContext
        return UserContext(
            user_id=str(user.id),
            email=user.email,
            name=user.name,
            roles=user.roles,
            permissions=user.permissions,
            metadata=user.metadata,
        )

    async def refresh_token(self, refresh_token: str) -> tuple[str, str]:
        """Refresh access token using refresh token.

        Args:
            refresh_token: JWT refresh token

        Returns:
            Tuple of (new_access_token, new_refresh_token)

        Raises:
            TokenExpiredError: If refresh token is expired
            InvalidTokenError: If refresh token is invalid
            SecurityError: If token reuse is detected
        """
        async with self.db_pool.connection() as conn, conn.cursor() as cursor:
            # Use TokenManager's rotation method which handles theft detection
            new_tokens = await self.token_manager.rotate_refresh_token(
                refresh_token, cursor, self.schema
            )

            # Commit the token usage tracking
            await conn.commit()

            return (new_tokens["access_token"], new_tokens["refresh_token"])

    async def revoke_token(self, token: str) -> None:
        """Revoke a token and its associated session.

        This method extracts the token family from either an access or refresh
        token and revokes all tokens in that family.

        Args:
            token: JWT token (access or refresh)
        """
        try:
            # Try to parse as refresh token first (has family info)
            payload = self.token_manager.verify_refresh_token(token)
            family_id = payload.get("family")
        except (TokenExpiredError, InvalidTokenError):
            # Try as access token, extract family from session
            try:
                payload = self.token_manager.verify_access_token(token)
                session_id = payload.get("session_id")

                if not session_id:
                    # Can't revoke without family info
                    return

                # Get family from session
                async with self.db_pool.connection() as conn, conn.cursor() as cursor:
                    await cursor.execute(
                        f"SELECT token_family FROM {self.schema}.tb_session WHERE pk_session = %s",
                        (session_id,),
                    )
                    result = await cursor.fetchone()
                    if not result:
                        return
                    family_id = result[0]

            except (TokenExpiredError, InvalidTokenError):
                # Token is invalid, nothing to revoke
                return

        if family_id:
            async with self.db_pool.connection() as conn, conn.cursor() as cursor:
                await self.token_manager.invalidate_token_family(family_id, cursor, self.schema)
                await conn.commit()

    async def get_user_by_id(self, user_id: str) -> Optional[UserContext]:
        """Get user context by user ID.

        This is a convenience method for getting user info without a token.

        Args:
            user_id: User ID

        Returns:
            UserContext if user exists and is active, None otherwise
        """
        async with self.db_pool.connection() as conn, conn.cursor() as cursor:
            user = await User.get_by_id(cursor, self.schema, user_id)

        if not user or not user.is_active:
            return None

        return UserContext(
            user_id=str(user.id),
            email=user.email,
            name=user.name,
            roles=user.roles,
            permissions=user.permissions,
            metadata=user.metadata,
        )

    async def create_tokens_for_user(
        self, user_id: str, session_metadata: Optional[dict] = None
    ) -> dict[str, any]:
        """Create new tokens for a user (for login/registration).

        Args:
            user_id: User ID
            session_metadata: Optional session metadata (user agent, IP, etc.)

        Returns:
            Dictionary with access_token, refresh_token, family_id, expires_at
        """
        # Generate initial tokens
        tokens = self.token_manager.generate_tokens(user_id)

        # Store session in database
        async with self.db_pool.connection() as conn, conn.cursor() as cursor:
            await cursor.execute(
                f"""
                    INSERT INTO {self.schema}.tb_session (
                        fk_user, token_family, user_agent, ip_address
                    ) VALUES (%s, %s, %s, %s)
                    RETURNING pk_session
                    """,
                (
                    user_id,
                    tokens["family_id"],
                    session_metadata.get("user_agent") if session_metadata else None,
                    session_metadata.get("ip_address") if session_metadata else None,
                ),
            )
            session_result = await cursor.fetchone()
            session_id = str(session_result[0]) if session_result else None

            await conn.commit()

        # Generate final tokens with session_id
        if session_id:
            user_claims = {"session_id": session_id}
            tokens = self.token_manager.generate_tokens(
                user_id, family_id=tokens["family_id"], user_claims=user_claims
            )

        return tokens
