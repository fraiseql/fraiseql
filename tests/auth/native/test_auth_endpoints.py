import hashlib
from datetime import datetime, timedelta, UTC
from uuid import uuid4

import jwt
import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from tests.utils.schema_utils import get_current_schema

from fraiseql.auth.native.models import User

pytestmark = pytest.mark.database


@pytest.fixture
async def app(db_with_native_auth):
    """Create test FastAPI app with auth routes."""
    from fraiseql.auth.native.router import auth_router

    app = FastAPI()

    # Get the current schema for the test
    schema = await get_current_schema(db_with_native_auth)

    # Store schema in app state for use in endpoints
    app.state.test_schema = schema
    app.state.db_connection = db_with_native_auth

    app.include_router(auth_router, prefix="/auth")

    # Override the database dependency
    from fraiseql.auth.native.router import get_db

    async def override_get_db():
        yield db_with_native_auth

    app.dependency_overrides[get_db] = override_get_db

    return app


@pytest.fixture
def client(app):
    """Create test client."""
    return TestClient(app)


@pytest.fixture
async def test_user(db_with_native_auth):
    """Create a test user."""
    schema = await get_current_schema(db_with_native_auth)
    user = User(
        email="test@example.com",
        password="Test123!@#",
        name="Test User",
        roles=["user"],
        is_active=True,
        email_verified=True,
    )
    async with db_with_native_auth.cursor() as cursor:
        await user.save(cursor, schema)
    yield user
    # Cleanup
    async with db_with_native_auth.cursor() as cursor:
        await cursor.execute(f"DELETE FROM {schema}.tb_user WHERE pk_user = %s", (user.id,))


class TestAuthEndpoints:
    """Test auth REST API endpoints."""

    async def test_register_success(self, client, db_with_native_auth):
        """Test successful user registration."""
        response = client.post(
            "/auth/register",
            json={"email": "newuser@example.com", "password": "SecurePass123!", "name": "New User"},
        )

        assert response.status_code == 201
        data = response.json()
        assert data["user"]["email"] == "newuser@example.com"
        assert data["user"]["name"] == "New User"
        assert "access_token" in data
        assert "refresh_token" in data
        assert data["token_type"] == "bearer"

        # Verify user was created in database
        schema = await get_current_schema(db_with_native_auth)
        async with db_with_native_auth.cursor() as cursor:
            user = await User.get_by_email(cursor, schema, "newuser@example.com")
        assert user is not None
        assert user.email == "newuser@example.com"

    async def test_register_duplicate_email(self, client, test_user):
        """Test registration with existing email."""
        response = client.post(
            "/auth/register",
            json={"email": test_user.email, "password": "SecurePass123!", "name": "Duplicate User"},
        )

        assert response.status_code == 409
        assert response.json()["detail"] == "Email already registered"

    async def test_register_weak_password(self, client):
        """Test registration with weak password."""
        response = client.post(
            "/auth/register",
            json={"email": "weak@example.com", "password": "weak", "name": "Weak Password User"},
        )

        assert response.status_code == 422
        assert "password" in response.json()["detail"][0]["loc"]

    async def test_login_success(self, client, test_user, db_with_native_auth):
        """Test successful login."""
        response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )

        assert response.status_code == 200
        data = response.json()
        assert data["user"]["email"] == test_user.email
        assert "access_token" in data
        assert "refresh_token" in data
        assert data["token_type"] == "bearer"

        # Verify session was created
        schema = await get_current_schema(db_with_native_auth)
        async with db_with_native_auth.cursor() as cursor:
            await cursor.execute(
                f"SELECT * FROM {schema}.tb_session WHERE fk_user = %s",
                (test_user.id,)
            )
            result = await cursor.fetchone()
        assert result is not None

    async def test_login_invalid_credentials(self, client, test_user):
        """Test login with invalid credentials."""
        response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "WrongPassword123!"}
        )

        assert response.status_code == 401
        assert response.json()["detail"] == "Invalid email or password"

    async def test_login_inactive_user(self, client, db_with_native_auth):
        """Test login with inactive user."""
        user = User(
            email="inactive@example.com",
            password="Test123!@#",
            name="Inactive User",
            is_active=False,
        )
        schema = await get_current_schema(db_with_native_auth)
        async with db_with_native_auth.cursor() as cursor:
            await user.save(cursor, schema)

        response = client.post("/auth/login", json={"email": user.email, "password": "Test123!@#"})

        assert response.status_code == 403
        assert response.json()["detail"] == "Account is disabled"

    async def test_refresh_token_success(self, client, test_user, db_with_native_auth):
        """Test successful token refresh."""
        # First login to get tokens
        login_response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        refresh_token = login_response.json()["refresh_token"]

        # Use refresh token
        response = client.post("/auth/refresh", json={"refresh_token": refresh_token})

        assert response.status_code == 200
        data = response.json()
        assert "access_token" in data
        assert "refresh_token" in data
        assert data["refresh_token"] != refresh_token  # New refresh token

    async def test_refresh_token_reuse_detection(self, client, test_user, db_with_native_auth):
        """Test refresh token reuse detection."""
        # Login and refresh once
        login_response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        old_refresh_token = login_response.json()["refresh_token"]

        refresh_response = client.post("/auth/refresh", json={"refresh_token": old_refresh_token})
        new_refresh_token = refresh_response.json()["refresh_token"]

        # Try to reuse old refresh token
        response = client.post("/auth/refresh", json={"refresh_token": old_refresh_token})

        assert response.status_code == 401
        assert "Token theft detected" in response.json()["detail"]

        # Verify new refresh token is also invalidated
        response = client.post("/auth/refresh", json={"refresh_token": new_refresh_token})
        assert response.status_code == 401

    async def test_get_current_user(self, client, test_user, db_with_native_auth):
        """Test getting current user info."""
        # Login first
        login_response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        access_token = login_response.json()["access_token"]

        # Get current user
        response = client.get("/auth/me", headers={"Authorization": f"Bearer {access_token}"})

        assert response.status_code == 200
        data = response.json()
        assert data["email"] == test_user.email
        assert data["name"] == test_user.name
        assert "password_hash" not in data

    async def test_get_current_user_unauthorized(self, client):
        """Test getting current user without auth."""
        response = client.get("/auth/me")
        assert response.status_code == 401

    async def test_logout(self, client, test_user, db_with_native_auth):
        """Test logout endpoint."""
        # Login first
        login_response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        access_token = login_response.json()["access_token"]
        refresh_token = login_response.json()["refresh_token"]

        # Logout
        response = client.post(
            "/auth/logout",
            headers={"Authorization": f"Bearer {access_token}"},
            json={"refresh_token": refresh_token},
        )

        assert response.status_code == 200
        assert response.json()["message"] == "Successfully logged out"

        # Verify refresh token is invalidated
        refresh_response = client.post("/auth/refresh", json={"refresh_token": refresh_token})
        assert refresh_response.status_code == 401

    async def test_forgot_password(self, client, test_user, db_with_native_auth):
        """Test forgot password endpoint."""
        response = client.post("/auth/forgot-password", json={"email": test_user.email})

        assert response.status_code == 200
        assert response.json()["message"] == "If the email exists, a reset link has been sent"

        # Verify reset token was created
        schema = await get_current_schema(db_with_native_auth)
        async with db_with_native_auth.cursor() as cursor:
            await cursor.execute(
                f"SELECT * FROM {schema}.tb_password_reset WHERE fk_user = %s",
                (test_user.id,)
            )
            result = await cursor.fetchone()
        assert result is not None
        assert result[5] is None  # 'used_at' column should be NULL for unused tokens

    async def test_reset_password(self, client, test_user, db_with_native_auth):
        """Test password reset endpoint."""
        # Create reset token
        reset_token = str(uuid4())
        token_hash = hashlib.sha256(reset_token.encode()).hexdigest()
        schema = await get_current_schema(db_with_native_auth)
        async with db_with_native_auth.cursor() as cursor:
            await cursor.execute(
                f"""
                INSERT INTO {schema}.tb_password_reset (fk_user, token_hash, expires_at)
                VALUES (%s, %s, %s)
                """,
                (test_user.id, token_hash, datetime.now(UTC) + timedelta(hours=1))
            )

        # Reset password
        response = client.post(
            "/auth/reset-password", json={"token": reset_token, "new_password": "NewSecure123!@#"}
        )

        assert response.status_code == 200
        assert response.json()["message"] == "Password reset successfully"

        # Verify can login with new password
        login_response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "NewSecure123!@#"}
        )
        assert login_response.status_code == 200

    async def test_list_sessions(self, client, test_user, db_with_native_auth):
        """Test listing active sessions."""
        # Create multiple sessions
        for i in range(3):
            client.post("/auth/login", json={"email": test_user.email, "password": "Test123!@#"})

        # Get one access token
        login_response = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        access_token = login_response.json()["access_token"]

        # List sessions
        response = client.get("/auth/sessions", headers={"Authorization": f"Bearer {access_token}"})

        assert response.status_code == 200
        sessions = response.json()
        assert len(sessions) >= 4  # At least 4 sessions created

    async def test_revoke_session(self, client, test_user, db_with_native_auth):
        """Test revoking a specific session."""
        # Create two sessions
        login1 = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        token1 = login1.json()["access_token"]

        login2 = client.post(
            "/auth/login", json={"email": test_user.email, "password": "Test123!@#"}
        )
        token2 = login2.json()["access_token"]
        refresh2 = login2.json()["refresh_token"]

        # Get session ID from token
        payload = jwt.decode(token2, options={"verify_signature": False})
        session_id = payload["session_id"]

        # Revoke second session using first session's token
        response = client.delete(
            f"/auth/sessions/{session_id}", headers={"Authorization": f"Bearer {token1}"}
        )

        assert response.status_code == 200

        # Verify second session's refresh token is invalidated
        refresh_response = client.post("/auth/refresh", json={"refresh_token": refresh2})
        assert refresh_response.status_code == 401
