"""Tests for native authentication database schema."""

import uuid
from pathlib import Path

import pytest
from tests.utils.schema_utils import get_current_schema


@pytest.fixture(autouse=True)
async def apply_native_auth_migration(db_connection_committed):
    """Apply native auth schema migration before running tests."""
    async with db_connection_committed.cursor() as cursor:
        schema = await get_current_schema(db_connection_committed)

        # Set search path for the migration
        await cursor.execute(f"SET search_path TO {schema}, public")

        # Read migration file
        migration_path = (
            Path(__file__).parent.parent.parent.parent
            / "src/fraiseql/auth/native/migrations/001_native_auth_schema.sql"
        )
        with open(migration_path) as f:
            migration_sql = f.read()

        # Execute migration (tables will be created in the current schema)
        await cursor.execute(migration_sql)


class TestUserTableSchema:
    """Test the tb_user table schema."""

    @pytest.mark.database
    async def test_user_table_exists(self, db_connection_committed):
        """Test that tb_user table exists with correct structure."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check table exists
            await cursor.execute(
                """
                SELECT column_name, data_type, is_nullable, column_default
                FROM information_schema.columns
                WHERE table_schema = %s AND table_name = 'tb_user'
                ORDER BY ordinal_position
            """,
                (schema,),
            )

            columns = await cursor.fetchall()
            column_dict = {
                col[0]: {"type": col[1], "nullable": col[2], "default": col[3]} for col in columns
            }

            # Verify required columns exist
            assert "pk_user" in column_dict
            assert column_dict["pk_user"]["type"] == "uuid"
            assert column_dict["pk_user"]["nullable"] == "NO"

            assert "email" in column_dict
            assert column_dict["email"]["type"] == "character varying"
            assert column_dict["email"]["nullable"] == "NO"

            assert "password_hash" in column_dict
            assert column_dict["password_hash"]["type"] == "character varying"
            assert column_dict["password_hash"]["nullable"] == "NO"

            assert "name" in column_dict
            assert column_dict["name"]["type"] == "character varying"

            assert "roles" in column_dict
            assert column_dict["roles"]["type"] == "ARRAY"

            assert "permissions" in column_dict
            assert column_dict["permissions"]["type"] == "ARRAY"

            assert "metadata" in column_dict
            assert column_dict["metadata"]["type"] == "jsonb"

            assert "is_active" in column_dict
            assert column_dict["is_active"]["type"] == "boolean"

            assert "created_at" in column_dict
            assert column_dict["created_at"]["type"] == "timestamp with time zone"

            assert "updated_at" in column_dict
            assert column_dict["updated_at"]["type"] == "timestamp with time zone"

    @pytest.mark.database
    async def test_user_email_unique_constraint(self, db_connection_committed):
        """Test that email has a unique constraint."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check for unique constraint on email
            await cursor.execute(
                """
                SELECT constraint_name
                FROM information_schema.table_constraints
                WHERE table_schema = %s 
                AND table_name = 'tb_user'
                AND constraint_type = 'UNIQUE'
            """,
                (schema,),
            )

            constraints = await cursor.fetchall()
            constraint_names = [c[0] for c in constraints]

            # Should have at least one unique constraint (email)
            assert len(constraint_names) > 0

            # Verify email uniqueness by attempting duplicate insert
            user_id1 = str(uuid.uuid4())
            user_id2 = str(uuid.uuid4())

            # First insert should succeed
            await cursor.execute(
                f"""
                INSERT INTO {schema}.tb_user (pk_user, email, password_hash, name)
                VALUES (%s, %s, %s, %s)
            """,
                (user_id1, "test@example.com", "hash123", "Test User"),
            )

            # Commit the first insert
            await db_connection_committed.commit()

            # Second insert with same email should fail
            try:
                await cursor.execute(
                    f"""
                    INSERT INTO {schema}.tb_user (pk_user, email, password_hash, name)
                    VALUES (%s, %s, %s, %s)
                """,
                    (user_id2, "test@example.com", "hash456", "Another User"),
                )
                await db_connection_committed.commit()
                pytest.fail("Expected unique constraint violation")
            except Exception as e:
                # Rollback the failed transaction
                await db_connection_committed.rollback()
                assert "unique" in str(e).lower() or "duplicate" in str(e).lower()


class TestSessionTableSchema:
    """Test the tb_session table schema."""

    @pytest.mark.database
    async def test_session_table_exists(self, db_connection_committed):
        """Test that tb_session table exists with correct structure."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check table exists
            await cursor.execute(
                """
                SELECT column_name, data_type, is_nullable
                FROM information_schema.columns
                WHERE table_schema = %s AND table_name = 'tb_session'
                ORDER BY ordinal_position
            """,
                (schema,),
            )

            columns = await cursor.fetchall()
            column_dict = {col[0]: {"type": col[1], "nullable": col[2]} for col in columns}

            # Verify required columns
            assert "pk_session" in column_dict
            assert column_dict["pk_session"]["type"] == "uuid"

            assert "fk_user" in column_dict
            assert column_dict["fk_user"]["type"] == "uuid"
            assert column_dict["fk_user"]["nullable"] == "NO"

            assert "token_family" in column_dict
            assert column_dict["token_family"]["type"] == "uuid"
            assert column_dict["token_family"]["nullable"] == "NO"

            assert "device_info" in column_dict
            assert column_dict["device_info"]["type"] == "jsonb"

            assert "ip_address" in column_dict
            assert column_dict["ip_address"]["type"] == "inet"

            assert "created_at" in column_dict
            assert column_dict["created_at"]["type"] == "timestamp with time zone"

            assert "last_active" in column_dict
            assert column_dict["last_active"]["type"] == "timestamp with time zone"

            assert "revoked_at" in column_dict
            assert column_dict["revoked_at"]["type"] == "timestamp with time zone"

    @pytest.mark.database
    async def test_session_foreign_key_to_user(self, db_connection_committed):
        """Test that tb_session has foreign key to tb_user."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check foreign key exists
            await cursor.execute(
                """
                SELECT constraint_name
                FROM information_schema.table_constraints
                WHERE table_schema = %s
                AND table_name = 'tb_session'
                AND constraint_type = 'FOREIGN KEY'
            """,
                (schema,),
            )

            fk_constraints = await cursor.fetchall()
            assert len(fk_constraints) > 0

    @pytest.mark.database
    async def test_session_indexes(self, db_connection_committed):
        """Test that tb_session has proper indexes for performance."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check for index on token_family for active sessions
            await cursor.execute(
                """
                SELECT indexname
                FROM pg_indexes
                WHERE schemaname = %s
                AND tablename = 'tb_session'
                AND indexdef LIKE '%%token_family%%'
            """,
                (schema,),
            )

            indexes = await cursor.fetchall()
            assert len(indexes) > 0, "Should have index on token_family"


class TestUsedRefreshTokenTableSchema:
    """Test the tb_used_refresh_token table schema."""

    @pytest.mark.database
    async def test_used_refresh_token_table_exists(self, db_connection_committed):
        """Test that tb_used_refresh_token table exists."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check table exists
            await cursor.execute(
                """
                SELECT column_name, data_type, is_nullable
                FROM information_schema.columns
                WHERE table_schema = %s AND table_name = 'tb_used_refresh_token'
                ORDER BY ordinal_position
            """,
                (schema,),
            )

            columns = await cursor.fetchall()
            column_dict = {col[0]: {"type": col[1], "nullable": col[2]} for col in columns}

            # Verify required columns
            assert "token_jti" in column_dict
            assert column_dict["token_jti"]["type"] == "text"
            assert column_dict["token_jti"]["nullable"] == "NO"

            assert "family_id" in column_dict
            assert column_dict["family_id"]["type"] == "uuid"
            assert column_dict["family_id"]["nullable"] == "NO"

            assert "used_at" in column_dict
            assert column_dict["used_at"]["type"] == "timestamp with time zone"

    @pytest.mark.database
    async def test_used_refresh_token_primary_key(self, db_connection_committed):
        """Test that token_jti is primary key."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check primary key
            await cursor.execute(
                """
                SELECT constraint_name
                FROM information_schema.table_constraints
                WHERE table_schema = %s
                AND table_name = 'tb_used_refresh_token'
                AND constraint_type = 'PRIMARY KEY'
            """,
                (schema,),
            )

            pk_constraint = await cursor.fetchone()
            assert pk_constraint is not None

    @pytest.mark.database
    async def test_used_refresh_token_cleanup_index(self, db_connection_committed):
        """Test index for cleanup of old tokens."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check for index on used_at for cleanup
            await cursor.execute(
                """
                SELECT indexname
                FROM pg_indexes
                WHERE schemaname = %s
                AND tablename = 'tb_used_refresh_token'
                AND indexdef LIKE '%%used_at%%'
            """,
                (schema,),
            )

            indexes = await cursor.fetchall()
            assert len(indexes) > 0, "Should have index on used_at for cleanup"


class TestPasswordResetTableSchema:
    """Test the tb_password_reset table schema."""

    @pytest.mark.database
    async def test_password_reset_table_exists(self, db_connection_committed):
        """Test that tb_password_reset table exists."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check table exists
            await cursor.execute(
                """
                SELECT column_name, data_type, is_nullable
                FROM information_schema.columns
                WHERE table_schema = %s AND table_name = 'tb_password_reset'
                ORDER BY ordinal_position
            """,
                (schema,),
            )

            columns = await cursor.fetchall()
            column_dict = {col[0]: {"type": col[1], "nullable": col[2]} for col in columns}

            # Verify required columns
            assert "pk_reset" in column_dict
            assert column_dict["pk_reset"]["type"] == "uuid"

            assert "fk_user" in column_dict
            assert column_dict["fk_user"]["type"] == "uuid"
            assert column_dict["fk_user"]["nullable"] == "NO"

            assert "token_hash" in column_dict
            assert column_dict["token_hash"]["type"] == "character varying"
            assert column_dict["token_hash"]["nullable"] == "NO"

            assert "created_at" in column_dict
            assert column_dict["created_at"]["type"] == "timestamp with time zone"

            assert "expires_at" in column_dict
            assert column_dict["expires_at"]["type"] == "timestamp with time zone"
            assert column_dict["expires_at"]["nullable"] == "NO"

            assert "used_at" in column_dict
            assert column_dict["used_at"]["type"] == "timestamp with time zone"


class TestAuthAuditTableSchema:
    """Test the tb_auth_audit table schema."""

    @pytest.mark.database
    async def test_auth_audit_table_exists(self, db_connection_committed):
        """Test that tb_auth_audit table exists for security logging."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check table exists
            await cursor.execute(
                """
                SELECT column_name, data_type, is_nullable
                FROM information_schema.columns
                WHERE table_schema = %s AND table_name = 'tb_auth_audit'
                ORDER BY ordinal_position
            """,
                (schema,),
            )

            columns = await cursor.fetchall()
            column_dict = {col[0]: {"type": col[1], "nullable": col[2]} for col in columns}

            # Verify audit columns
            assert "pk_audit" in column_dict
            assert column_dict["pk_audit"]["type"] == "uuid"

            assert "event_type" in column_dict
            assert column_dict["event_type"]["type"] == "text"
            assert column_dict["event_type"]["nullable"] == "NO"

            assert "user_id" in column_dict
            assert column_dict["user_id"]["type"] == "uuid"

            assert "ip_address" in column_dict
            assert column_dict["ip_address"]["type"] == "inet"

            assert "user_agent" in column_dict
            assert column_dict["user_agent"]["type"] == "text"

            assert "event_data" in column_dict
            assert column_dict["event_data"]["type"] == "jsonb"

            assert "created_at" in column_dict
            assert column_dict["created_at"]["type"] == "timestamp with time zone"

    @pytest.mark.database
    async def test_auth_audit_indexes(self, db_connection_committed):
        """Test indexes for audit trail queries."""
        async with db_connection_committed.cursor() as cursor:
            schema = await get_current_schema(db_connection_committed)

            # Check for index on user_id and created_at
            await cursor.execute(
                """
                SELECT indexname
                FROM pg_indexes
                WHERE schemaname = %s
                AND tablename = 'tb_auth_audit'
                AND (indexdef LIKE '%%user_id%%' OR indexdef LIKE '%%created_at%%')
            """,
                (schema,),
            )

            indexes = await cursor.fetchall()
            assert len(indexes) > 0, "Should have indexes for audit queries"
