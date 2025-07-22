"""Tests for User model in native authentication."""

import pytest
from tests.utils.schema_utils import get_current_schema


class TestUserModel:
    """Test the User model functionality."""

    @pytest.mark.database
    async def test_create_user_with_valid_data(self, db_with_native_auth):
        """Test creating a user with valid data."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create user
            user = User(email="test@example.com", password="SecurePassword123!", name="Test User")

            # Save to database
            await user.save(cursor, schema)

            # Verify user was saved
            await cursor.execute(
                f"""
                SELECT pk_user, email, password_hash, name, is_active, email_verified
                FROM {schema}.tb_user
                WHERE email = %s
            """,
                ("test@example.com",),
            )

            row = await cursor.fetchone()
            assert row is not None
            assert row[1] == "test@example.com"
            assert row[3] == "Test User"
            assert row[4] is True  # is_active
            assert row[5] is False  # email_verified

            # Verify password was hashed
            assert row[2] != "SecurePassword123!"
            assert row[2].startswith("$argon2id$")  # Argon2id hash prefix

    @pytest.mark.database
    async def test_create_user_duplicate_email_fails(self, db_with_native_auth):
        """Test that creating a user with duplicate email fails."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create first user
            user1 = User(email="duplicate@example.com", password="Password123!", name="User 1")
            await user1.save(cursor, schema)
            await db_with_native_auth.commit()

            # Try to create second user with same email
            user2 = User(
                email="duplicate@example.com", password="DifferentPassword123!", name="User 2"
            )

            try:
                await user2.save(cursor, schema)
                pytest.fail("Expected duplicate email error")
            except Exception as e:
                # Rollback the failed transaction
                await db_with_native_auth.rollback()
                assert "duplicate" in str(e).lower() or "unique" in str(e).lower()

    @pytest.mark.database
    async def test_password_hashing_and_verification(self, db_with_native_auth):
        """Test password hashing and verification works correctly."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create user
            password = "MySecurePassword123!"
            user = User(email="hash_test@example.com", password=password, name="Hash Test User")
            await user.save(cursor, schema)

            # Load user from database
            loaded_user = await User.get_by_email(cursor, schema, "hash_test@example.com")

            # Verify correct password
            assert loaded_user.verify_password(password) is True

            # Verify incorrect password
            assert loaded_user.verify_password("WrongPassword") is False
            assert loaded_user.verify_password("") is False
            assert loaded_user.verify_password("MySecurePassword123") is False  # Missing !

    @pytest.mark.database
    async def test_user_roles_and_permissions(self, db_with_native_auth):
        """Test user roles and permissions storage."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create user with roles and permissions
            user = User(
                email="roles_test@example.com",
                password="Password123!",
                name="Roles Test User",
                roles=["admin", "user"],
                permissions=["users:read", "users:write", "posts:read"],
            )
            await user.save(cursor, schema)

            # Load user from database
            loaded_user = await User.get_by_email(cursor, schema, "roles_test@example.com")

            # Verify roles
            assert "admin" in loaded_user.roles
            assert "user" in loaded_user.roles
            assert len(loaded_user.roles) == 2

            # Verify permissions
            assert "users:read" in loaded_user.permissions
            assert "users:write" in loaded_user.permissions
            assert "posts:read" in loaded_user.permissions
            assert len(loaded_user.permissions) == 3

    @pytest.mark.database
    async def test_user_metadata_storage(self, db_with_native_auth):
        """Test user metadata JSONB storage."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create user with metadata
            metadata = {
                "preferences": {"theme": "dark", "language": "en"},
                "profile": {"bio": "Software developer", "location": "San Francisco"},
            }

            user = User(
                email="metadata_test@example.com",
                password="Password123!",
                name="Metadata Test User",
                metadata=metadata,
            )
            await user.save(cursor, schema)

            # Load user from database
            loaded_user = await User.get_by_email(cursor, schema, "metadata_test@example.com")

            # Verify metadata
            assert loaded_user.metadata["preferences"]["theme"] == "dark"
            assert loaded_user.metadata["preferences"]["language"] == "en"
            assert loaded_user.metadata["profile"]["bio"] == "Software developer"
            assert loaded_user.metadata["profile"]["location"] == "San Francisco"

    @pytest.mark.database
    async def test_get_by_email_not_found(self, db_with_native_auth):
        """Test getting user by email when not found."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Try to get non-existent user
            user = await User.get_by_email(cursor, schema, "nonexistent@example.com")
            assert user is None

    @pytest.mark.database
    async def test_get_by_id(self, db_with_native_auth):
        """Test getting user by ID."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create user
            user = User(email="id_test@example.com", password="Password123!", name="ID Test User")
            await user.save(cursor, schema)

            # Get user by ID
            loaded_user = await User.get_by_id(cursor, schema, user.id)

            assert loaded_user is not None
            assert loaded_user.id == user.id
            assert loaded_user.email == "id_test@example.com"
            assert loaded_user.name == "ID Test User"

    @pytest.mark.database
    async def test_update_user(self, db_with_native_auth):
        """Test updating user data."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create user
            user = User(
                email="update_test@example.com", password="Password123!", name="Original Name"
            )
            await user.save(cursor, schema)
            await db_with_native_auth.commit()

            # Update user
            user.name = "Updated Name"
            user.roles = ["moderator"]
            user.metadata = {"updated": True}
            await user.update(cursor, schema)
            await db_with_native_auth.commit()

            # Load updated user
            loaded_user = await User.get_by_id(cursor, schema, user.id)

            assert loaded_user.name == "Updated Name"
            assert "moderator" in loaded_user.roles
            assert loaded_user.metadata["updated"] is True

            # Verify email didn't change
            assert loaded_user.email == "update_test@example.com"

    @pytest.mark.database
    async def test_deactivate_user(self, db_with_native_auth):
        """Test deactivating a user."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create active user
            user = User(
                email="deactivate_test@example.com",
                password="Password123!",
                name="Deactivate Test User",
            )
            await user.save(cursor, schema)
            assert user.is_active is True

            # Deactivate user
            await user.deactivate(cursor, schema)
            await db_with_native_auth.commit()

            # Load user and verify
            loaded_user = await User.get_by_id(cursor, schema, user.id)
            assert loaded_user.is_active is False

    @pytest.mark.database
    async def test_verify_email(self, db_with_native_auth):
        """Test email verification."""
        from fraiseql.auth.native.models import User

        async with db_with_native_auth.cursor() as cursor:
            schema = await get_current_schema(db_with_native_auth)

            # Create unverified user
            user = User(
                email="verify_test@example.com", password="Password123!", name="Verify Test User"
            )
            await user.save(cursor, schema)
            assert user.email_verified is False

            # Verify email
            await user.verify_email(cursor, schema)
            await db_with_native_auth.commit()

            # Load user and verify
            loaded_user = await User.get_by_id(cursor, schema, user.id)
            assert loaded_user.email_verified is True

    def test_password_validation(self):
        """Test password validation rules."""
        from fraiseql.auth.native.models import User

        # Valid passwords
        assert User.validate_password("SecurePass123!") is True
        assert User.validate_password("Str0ng@Password") is True
        assert User.validate_password("C0mplex#Pass") is True

        # Invalid passwords
        assert User.validate_password("short") is False  # Too short
        assert User.validate_password("nouppercase123!") is False  # No uppercase
        assert User.validate_password("NOLOWERCASE123!") is False  # No lowercase
        assert User.validate_password("NoNumbers!") is False  # No numbers
        assert User.validate_password("NoSpecialChar123") is False  # No special chars
        assert User.validate_password("") is False  # Empty
        assert User.validate_password("        ") is False  # Whitespace only

    def test_email_validation(self):
        """Test email validation."""
        from fraiseql.auth.native.models import User

        # Valid emails
        assert User.validate_email("user@example.com") is True
        assert User.validate_email("test.user@example.co.uk") is True
        assert User.validate_email("user+tag@example.com") is True

        # Invalid emails
        assert User.validate_email("notanemail") is False
        assert User.validate_email("@example.com") is False
        assert User.validate_email("user@") is False
        assert User.validate_email("") is False
        assert User.validate_email("user @example.com") is False  # Space
        assert User.validate_email("user@example") is False  # No TLD
