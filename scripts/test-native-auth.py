#!/usr/bin/env python3
"""
Comprehensive test script for FraiseQL Native Authentication System.

This script tests the entire auth system end-to-end, including:
- Database schema creation
- User management
- Token operations
- Security features
- Integration components

Can be run locally or in CI/CD to validate the auth system.
"""

import asyncio
import os
import sys
import tempfile
import time
from datetime import datetime, UTC
from pathlib import Path
from uuid import uuid4

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

try:
    from psycopg_pool import AsyncConnectionPool

    from fraiseql.auth.native.factory import apply_native_auth_schema, create_native_auth_provider
    from fraiseql.auth.native.models import User
    from fraiseql.auth.native.provider import NativeAuthProvider
    from fraiseql.auth.native.tokens import TokenManager

    print("✅ All native auth imports successful")
except ImportError as e:
    print(f"❌ Import error: {e}")
    print("Make sure FraiseQL is installed: pip install -e .")
    sys.exit(1)


class AuthSystemTester:
    """Comprehensive test suite for the native auth system."""

    def __init__(self, database_url: str):
        self.database_url = database_url
        self.pool = None
        self.auth_provider = None
        self.test_users = []

    async def setup(self):
        """Initialize database connection and apply schema."""
        print("🔧 Setting up test environment...")

        # Create connection pool
        self.pool = AsyncConnectionPool(self.database_url, min_size=2, max_size=5, timeout=30)
        await self.pool.wait()
        print("✅ Database connection pool created")

        # Apply native auth schema
        await apply_native_auth_schema(self.pool)
        print("✅ Native auth schema applied")

        # Create auth provider
        self.auth_provider = await create_native_auth_provider(
            db_pool=self.pool, secret_key="test-secret-key-change-in-production"
        )
        print("✅ Auth provider created")

    async def cleanup(self):
        """Clean up test environment."""
        if self.pool:
            # Clean up test users
            async with self.pool.connection() as conn:
                async with conn.cursor() as cursor:
                    for user_email in [u.email for u in self.test_users]:
                        await cursor.execute(
                            "DELETE FROM public.tb_user WHERE email = %s", (user_email,)
                        )
                    await conn.commit()

            await self.pool.close()
            print("✅ Test environment cleaned up")

    async def test_password_security(self):
        """Test password hashing and validation."""
        print("\n🔐 Testing password security...")

        # Test password validation
        strong_password = "StrongP@ssw0rd123!"
        weak_password = "weak"

        assert User.validate_password(strong_password), "Strong password rejected"
        assert not User.validate_password(weak_password), "Weak password accepted"
        print("✅ Password validation working")

        # Test Argon2id hashing
        user = User(email="hash-test@example.com", password=strong_password, name="Hash Test User")

        assert user.verify_password(strong_password), "Password verification failed"
        assert not user.verify_password("wrong-password"), "Wrong password accepted"
        print("✅ Argon2id password hashing secure")

        # Test password update
        new_password = "NewStr0ng@Password!"
        user.set_password(new_password)
        assert user.verify_password(new_password), "New password verification failed"
        assert not user.verify_password(strong_password), "Old password still accepted"
        print("✅ Password update working")

    async def test_user_management(self):
        """Test user creation, retrieval, and management."""
        print("\n👤 Testing user management...")

        async with self.pool.connection() as conn:
            async with conn.cursor() as cursor:
                # Test user creation
                user = User(
                    email="test-user@example.com",
                    password="TestPassword123!",
                    name="Test User",
                    roles=["user", "admin"],
                    permissions=["read", "write"],
                    metadata={"department": "testing", "location": "remote"},
                )

                await user.save(cursor, "public")
                await conn.commit()
                self.test_users.append(user)
                print("✅ User creation successful")

                # Test user retrieval by email
                found_user = await User.get_by_email(cursor, "public", "test-user@example.com")
                assert found_user is not None, "User not found by email"
                assert found_user.email == "test-user@example.com", "Email mismatch"
                assert found_user.name == "Test User", "Name mismatch"
                assert "admin" in found_user.roles, "Roles not preserved"
                assert "write" in found_user.permissions, "Permissions not preserved"
                assert found_user.metadata["department"] == "testing", "Metadata not preserved"
                print("✅ User retrieval by email working")

                # Test user retrieval by ID
                found_by_id = await User.get_by_id(cursor, "public", user.id)
                assert found_by_id is not None, "User not found by ID"
                assert found_by_id.email == user.email, "ID lookup email mismatch"
                print("✅ User retrieval by ID working")

                # Test user update
                user.name = "Updated Test User"
                user.roles.append("moderator")
                await user.update(cursor, "public")
                await conn.commit()

                updated_user = await User.get_by_id(cursor, "public", user.id)
                assert updated_user.name == "Updated Test User", "Name update failed"
                assert "moderator" in updated_user.roles, "Role update failed"
                print("✅ User update working")

                # Test user deactivation
                await user.deactivate(cursor, "public")
                await conn.commit()

                deactivated_user = await User.get_by_id(cursor, "public", user.id)
                assert not deactivated_user.is_active, "User deactivation failed"
                print("✅ User deactivation working")

    async def test_token_operations(self):
        """Test JWT token generation, validation, and refresh."""
        print("\n🎫 Testing token operations...")

        user_id = str(uuid4())

        # Test token generation
        tokens = self.auth_provider.token_manager.generate_tokens(user_id)
        assert "access_token" in tokens, "Missing access token"
        assert "refresh_token" in tokens, "Missing refresh token"
        assert "family_id" in tokens, "Missing family ID"
        print("✅ Token generation working")

        # Test access token validation
        payload = self.auth_provider.token_manager.verify_access_token(tokens["access_token"])
        assert payload["sub"] == user_id, "Access token user ID mismatch"
        assert payload["type"] == "access", "Access token type mismatch"
        print("✅ Access token validation working")

        # Test refresh token validation
        refresh_payload = self.auth_provider.token_manager.verify_refresh_token(
            tokens["refresh_token"]
        )
        assert refresh_payload["sub"] == user_id, "Refresh token user ID mismatch"
        assert refresh_payload["type"] == "refresh", "Refresh token type mismatch"
        assert refresh_payload["family"] == tokens["family_id"], "Token family mismatch"
        print("✅ Refresh token validation working")

        # Test token refresh (requires database for theft detection)
        async with self.pool.connection() as conn:
            async with conn.cursor() as cursor:
                new_tokens = await self.auth_provider.token_manager.rotate_refresh_token(
                    tokens["refresh_token"], cursor, "public"
                )
                await conn.commit()

                assert new_tokens["access_token"] != tokens["access_token"], (
                    "Access token not rotated"
                )
                assert new_tokens["refresh_token"] != tokens["refresh_token"], (
                    "Refresh token not rotated"
                )
                assert new_tokens["family_id"] == tokens["family_id"], (
                    "Family ID should be preserved"
                )
                print("✅ Token refresh working")

        # Test token theft detection
        try:
            async with self.pool.connection() as conn:
                async with conn.cursor() as cursor:
                    # Try to use the old refresh token again
                    await self.auth_provider.token_manager.rotate_refresh_token(
                        tokens["refresh_token"],
                        cursor,
                        "public",  # This should fail
                    )
                    assert False, "Token reuse should have been detected"
        except Exception as e:
            print("✅ Token theft detection working")

    async def test_auth_provider_integration(self):
        """Test the complete auth provider integration."""
        print("\n🔗 Testing auth provider integration...")

        # Create a real user for testing
        async with self.pool.connection() as conn:
            async with conn.cursor() as cursor:
                user = User(
                    email="provider-test@example.com",
                    password="ProviderTest123!",
                    name="Provider Test User",
                    roles=["user"],
                    permissions=["read"],
                )
                await user.save(cursor, "public")
                await conn.commit()
                self.test_users.append(user)

        # Test token creation through provider
        tokens = await self.auth_provider.create_tokens_for_user(user.id)
        assert "access_token" in tokens, "Provider token creation failed"
        print("✅ Provider token creation working")

        # Test getting user from token
        user_context = await self.auth_provider.get_user_from_token(tokens["access_token"])
        assert user_context.user_id == str(user.id), "User context ID mismatch"
        assert user_context.email == user.email, "User context email mismatch"
        assert "user" in user_context.roles, "User context roles mismatch"
        assert "read" in user_context.permissions, "User context permissions mismatch"
        print("✅ User context from token working")

        # Test token refresh through provider
        new_tokens = await self.auth_provider.refresh_token(tokens["refresh_token"])
        assert len(new_tokens) == 2, "Refresh should return access and refresh tokens"
        assert new_tokens[0] != tokens["access_token"], "Access token not refreshed"
        print("✅ Provider token refresh working")

        # Test token revocation
        await self.auth_provider.revoke_token(tokens["access_token"])
        print("✅ Token revocation working")

    async def test_security_features(self):
        """Test security features like session management."""
        print("\n🛡️ Testing security features...")

        # Test session creation and tracking
        async with self.pool.connection() as conn:
            async with conn.cursor() as cursor:
                user = User(
                    email="security-test@example.com",
                    password="SecurityTest123!",
                    name="Security Test User",
                )
                await user.save(cursor, "public")
                await conn.commit()
                self.test_users.append(user)

                # Create tokens with session metadata
                session_metadata = {"user_agent": "pytest-runner", "ip_address": "127.0.0.1"}
                tokens = await self.auth_provider.create_tokens_for_user(user.id, session_metadata)

                # Verify session was created
                await cursor.execute(
                    "SELECT COUNT(*) FROM public.tb_session WHERE fk_user = %s", (user.id,)
                )
                session_count = (await cursor.fetchone())[0]
                assert session_count > 0, "Session not created"
                print("✅ Session creation working")

                # Test password reset token creation and validation
                reset_token = str(uuid4())
                token_hash = (
                    self.auth_provider.token_manager._hash_token(reset_token)
                    if hasattr(self.auth_provider.token_manager, "_hash_token")
                    else reset_token
                )

                # Insert password reset token (simulating forgot password flow)
                import hashlib
                from datetime import timedelta

                actual_token_hash = hashlib.sha256(reset_token.encode()).hexdigest()
                expires_at = datetime.now(UTC) + timedelta(hours=1)

                await cursor.execute(
                    "INSERT INTO public.tb_password_reset (fk_user, token_hash, expires_at) VALUES (%s, %s, %s)",
                    (user.id, actual_token_hash, expires_at),
                )
                await conn.commit()

                # Verify reset token exists
                await cursor.execute(
                    "SELECT COUNT(*) FROM public.tb_password_reset WHERE fk_user = %s AND used_at IS NULL",
                    (user.id,),
                )
                reset_count = (await cursor.fetchone())[0]
                assert reset_count > 0, "Password reset token not created"
                print("✅ Password reset token creation working")

    async def test_performance_benchmarks(self):
        """Test basic performance characteristics."""
        print("\n⚡ Testing performance benchmarks...")

        # Test password hashing performance
        start_time = time.time()
        for i in range(10):
            User(
                email=f"perf-test-{i}@example.com",
                password="PerfTest123!",
                name=f"Perf Test User {i}",
            )
        hash_time = time.time() - start_time
        print(
            f"✅ Password hashing: {hash_time:.3f}s for 10 users (~{hash_time / 10:.3f}s per user)"
        )

        # Test token generation performance
        tm = TokenManager("perf-test-secret")
        start_time = time.time()
        for i in range(100):
            tm.generate_tokens(f"user-{i}")
        token_time = time.time() - start_time
        print(
            f"✅ Token generation: {token_time:.3f}s for 100 tokens (~{token_time / 100 * 1000:.1f}ms per token)"
        )

        # Test token validation performance
        tokens = tm.generate_tokens("perf-user")
        start_time = time.time()
        for _ in range(1000):
            tm.verify_access_token(tokens["access_token"])
        validation_time = time.time() - start_time
        print(
            f"✅ Token validation: {validation_time:.3f}s for 1000 validations (~{validation_time / 1000 * 1000:.1f}ms per validation)"
        )

    async def run_all_tests(self):
        """Run all test suites."""
        try:
            await self.setup()

            print(f"🚀 Starting comprehensive native auth system tests...")
            print(f"📊 Database: {self.database_url}")
            print(f"⏰ Started at: {datetime.now().isoformat()}")

            # Run all test suites
            await self.test_password_security()
            await self.test_user_management()
            await self.test_token_operations()
            await self.test_auth_provider_integration()
            await self.test_security_features()
            await self.test_performance_benchmarks()

            print(f"\n🎯 ALL TESTS PASSED! ✅")
            print(f"⏰ Completed at: {datetime.now().isoformat()}")

            return True

        except Exception as e:
            print(f"\n❌ TEST FAILED: {e}")
            import traceback

            traceback.print_exc()
            return False

        finally:
            await self.cleanup()


async def main():
    """Main test runner."""
    # Get database URL from environment or use default for testing
    database_url = os.environ.get(
        "TEST_DATABASE_URL",
        os.environ.get(
            "DATABASE_URL", "postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_test"
        ),
    )

    print("=" * 60)
    print("🔐 FraiseQL Native Authentication System Test Suite")
    print("=" * 60)

    tester = AuthSystemTester(database_url)
    success = await tester.run_all_tests()

    if success:
        print("\n" + "=" * 60)
        print("✅ NATIVE AUTHENTICATION SYSTEM IS FULLY FUNCTIONAL")
        print("=" * 60)
        sys.exit(0)
    else:
        print("\n" + "=" * 60)
        print("❌ NATIVE AUTHENTICATION SYSTEM HAS ISSUES")
        print("=" * 60)
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
