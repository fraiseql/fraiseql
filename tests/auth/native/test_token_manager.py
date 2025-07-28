"""Tests for TokenManager in native authentication."""

import time
import uuid
from datetime import UTC, datetime, timedelta

import jwt
import pytest
from psycopg.types.json import Json
from tests.utils.schema_utils import get_current_schema


class TestTokenManager:
    """Test the TokenManager functionality."""

    def test_create_token_family(self):
        """Test creating a new token family ID."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(secret_key="test-secret-key")

        family_id = manager.create_token_family("user123")

        # Should be a valid UUID
        assert isinstance(family_id, str)
        uuid.UUID(family_id)  # Will raise if not valid UUID

    def test_generate_access_token(self):
        """Test generating an access token."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(secret_key="test-secret-key")
        user_id = "user123"

        tokens = manager.generate_tokens(user_id)

        # Verify access token
        assert "access_token" in tokens
        assert "expires_at" in tokens

        # Decode and verify claims
        decoded = jwt.decode(tokens["access_token"], "test-secret-key", algorithms=["HS256"])

        assert decoded["sub"] == user_id
        assert decoded["type"] == "access"
        assert "exp" in decoded
        assert "iat" in decoded
        assert "jti" in decoded

        # Verify expiration (15 minutes)
        exp_time = datetime.fromtimestamp(decoded["exp"], UTC)
        iat_time = datetime.fromtimestamp(decoded["iat"], UTC)
        assert (exp_time - iat_time).total_seconds() == 900  # 15 minutes

    def test_generate_refresh_token(self):
        """Test generating a refresh token with family tracking."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(secret_key="test-secret-key")
        user_id = "user123"

        tokens = manager.generate_tokens(user_id)

        # Verify refresh token
        assert "refresh_token" in tokens
        assert "family_id" in tokens

        # Decode and verify claims
        decoded = jwt.decode(tokens["refresh_token"], "test-secret-key", algorithms=["HS256"])

        assert decoded["sub"] == user_id
        assert decoded["type"] == "refresh"
        assert decoded["family"] == tokens["family_id"]
        assert "exp" in decoded
        assert "jti" in decoded

        # Verify expiration (30 days)
        exp_time = datetime.fromtimestamp(decoded["exp"], UTC)
        iat_time = datetime.fromtimestamp(decoded["iat"], UTC)
        assert (exp_time - iat_time).total_seconds() == 2592000  # 30 days

    def test_verify_valid_access_token(self):
        """Test verifying a valid access token."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(secret_key="test-secret-key")
        user_id = "user123"

        tokens = manager.generate_tokens(user_id)

        # Verify the access token
        payload = manager.verify_access_token(tokens["access_token"])

        assert payload["sub"] == user_id
        assert payload["type"] == "access"

    def test_verify_expired_access_token(self):
        """Test verifying an expired access token."""
        from fraiseql.auth.native.tokens import TokenExpiredError, TokenManager

        manager = TokenManager(secret_key="test-secret-key")

        # Create an expired token
        expired_payload = {
            "sub": "user123",
            "type": "access",
            "exp": datetime.now(UTC) - timedelta(minutes=1),
            "iat": datetime.now(UTC) - timedelta(minutes=16),
            "jti": str(uuid.uuid4()),
        }

        expired_token = jwt.encode(expired_payload, "test-secret-key", algorithm="HS256")

        # Should raise TokenExpiredError
        with pytest.raises(TokenExpiredError):
            manager.verify_access_token(expired_token)

    def test_verify_invalid_token(self):
        """Test verifying an invalid token."""
        from fraiseql.auth.native.tokens import InvalidTokenError, TokenManager

        manager = TokenManager(secret_key="test-secret-key")

        # Invalid token
        with pytest.raises(InvalidTokenError):
            manager.verify_access_token("invalid.token.here")

        # Token with wrong signature
        wrong_key_token = jwt.encode(
            {"sub": "user123", "type": "access"}, "wrong-key", algorithm="HS256"
        )

        with pytest.raises(InvalidTokenError):
            manager.verify_access_token(wrong_key_token)

    def test_verify_wrong_token_type(self):
        """Test verifying a token with wrong type."""
        from fraiseql.auth.native.tokens import InvalidTokenError, TokenManager

        manager = TokenManager(secret_key="test-secret-key")
        tokens = manager.generate_tokens("user123")

        # Try to verify refresh token as access token
        with pytest.raises(InvalidTokenError, match="Invalid token type"):
            manager.verify_access_token(tokens["refresh_token"])

    @pytest.mark.database
    async def test_rotate_refresh_token_success(self, db_with_native_auth):
        """Test successful refresh token rotation."""
        from fraiseql.auth.native.tokens import TokenManager

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            manager = TokenManager(secret_key="test-secret-key")
            user_id = "user123"

            # Generate initial tokens
            initial_tokens = manager.generate_tokens(user_id)

            # Wait a moment to ensure different timestamps
            time.sleep(0.1)

            # Rotate the refresh token
            new_tokens = await manager.rotate_refresh_token(
                initial_tokens["refresh_token"], cursor, schema
            )

            # Verify new tokens were generated
            assert new_tokens["access_token"] != initial_tokens["access_token"]
            assert new_tokens["refresh_token"] != initial_tokens["refresh_token"]
            assert new_tokens["family_id"] == initial_tokens["family_id"]  # Same family

            # Verify old refresh token was marked as used
            await cursor.execute(f"""
                SELECT token_jti FROM {schema}.tb_used_refresh_token
            """)
            used_tokens = await cursor.fetchall()
            assert len(used_tokens) == 1

    @pytest.mark.database
    async def test_token_theft_detection(self, db_with_native_auth):
        """Test that token reuse is detected as theft."""
        from fraiseql.auth.native.tokens import SecurityError, TokenManager

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            manager = TokenManager(secret_key="test-secret-key")
            user_id = "user123"

            # Generate initial tokens
            initial_tokens = manager.generate_tokens(user_id)

            # First rotation (legitimate)
            new_tokens = await manager.rotate_refresh_token(
                initial_tokens["refresh_token"], cursor, schema
            )
            await db_with_native_auth.commit()

            # Try to use the old token again (theft simulation)
            with pytest.raises(SecurityError, match="Token reuse detected"):
                await manager.rotate_refresh_token(initial_tokens["refresh_token"], cursor, schema)

    @pytest.mark.database
    async def test_invalidate_token_family(self, db_with_native_auth):
        """Test invalidating an entire token family."""
        from fraiseql.auth.native.models import User
        from fraiseql.auth.native.tokens import TokenManager

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create a user
            user = User(
                email="token_test@example.com", password="Password123!", name="Token Test User"
            )
            await user.save(cursor, schema)

            manager = TokenManager(secret_key="test-secret-key")

            # Generate tokens and create session
            tokens = manager.generate_tokens(user.id)

            # Create session record
            await cursor.execute(
                f"""
                INSERT INTO {schema}.tb_session
                (fk_user, token_family, user_agent, ip_address)
                VALUES (%s, %s, %s, %s)
            """,
                (
                    user.id,
                    tokens["family_id"],
                    Json({"browser": "Chrome", "os": "Linux"}),
                    "127.0.0.1",
                ),
            )
            await db_with_native_auth.commit()

            # Invalidate the token family
            await manager.invalidate_token_family(tokens["family_id"], cursor, schema)
            await db_with_native_auth.commit()

            # Check that all sessions in family are revoked
            await cursor.execute(
                f"""
                SELECT revoked_at FROM {schema}.tb_session
                WHERE token_family = %s
            """,
                (tokens["family_id"],),
            )

            session = await cursor.fetchone()
            assert session[0] is not None  # revoked_at should be set

    def test_extract_user_id_from_token(self):
        """Test extracting user ID from token without full verification."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(secret_key="test-secret-key")
        user_id = "user123"

        tokens = manager.generate_tokens(user_id)

        # Extract user ID
        extracted_id = manager.extract_user_id(tokens["access_token"])
        assert extracted_id == user_id

        # Should work even with expired token
        expired_payload = {
            "sub": "user456",
            "type": "access",
            "exp": datetime.now(UTC) - timedelta(minutes=1),
            "iat": datetime.now(UTC) - timedelta(minutes=16),
            "jti": str(uuid.uuid4()),
        }
        expired_token = jwt.encode(expired_payload, "test-secret-key", algorithm="HS256")

        extracted_id = manager.extract_user_id(expired_token)
        assert extracted_id == "user456"

    def test_custom_token_expiration(self):
        """Test creating tokens with custom expiration times."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(
            secret_key="test-secret-key",
            access_token_ttl=timedelta(minutes=5),
            refresh_token_ttl=timedelta(days=7),
        )

        tokens = manager.generate_tokens("user123")

        # Verify custom access token expiration
        access_decoded = jwt.decode(tokens["access_token"], "test-secret-key", algorithms=["HS256"])
        exp_time = datetime.fromtimestamp(access_decoded["exp"], UTC)
        iat_time = datetime.fromtimestamp(access_decoded["iat"], UTC)
        assert (exp_time - iat_time).total_seconds() == 300  # 5 minutes

        # Verify custom refresh token expiration
        refresh_decoded = jwt.decode(
            tokens["refresh_token"], "test-secret-key", algorithms=["HS256"]
        )
        exp_time = datetime.fromtimestamp(refresh_decoded["exp"], UTC)
        iat_time = datetime.fromtimestamp(refresh_decoded["iat"], UTC)
        assert (exp_time - iat_time).total_seconds() == 604800  # 7 days

    def test_token_includes_user_claims(self):
        """Test including additional user claims in tokens."""
        from fraiseql.auth.native.tokens import TokenManager

        manager = TokenManager(secret_key="test-secret-key")

        user_claims = {
            "email": "user@example.com",
            "roles": ["admin", "user"],
            "permissions": ["read:all", "write:all"],
        }

        tokens = manager.generate_tokens("user123", user_claims=user_claims)

        # Verify claims are in access token
        decoded = jwt.decode(tokens["access_token"], "test-secret-key", algorithms=["HS256"])

        assert decoded["email"] == "user@example.com"
        assert decoded["roles"] == ["admin", "user"]
        assert decoded["permissions"] == ["read:all", "write:all"]
