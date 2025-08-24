"""Integration tests for database repository operations.

These tests validate that FraiseQL's database repository correctly
interacts with PostgreSQL, handling CRUD operations, transactions,
query generation, and data consistency.
"""

from datetime import UTC, datetime
from uuid import uuid4

import pytest
from psycopg.types.json import Json

from tests_new.utilities.assertions.database import (
    assert_field_equals,
    assert_jsonb_field_equals,
    assert_row_exists,
    assert_row_not_exists,
    get_row_data,
)


@pytest.mark.integration
@pytest.mark.database
class TestBasicCRUDOperations:
    """Test basic Create, Read, Update, Delete operations."""

    async def test_create_user_record(self, db_with_schema):
        """Test creating a user record in the database."""
        user_id = str(uuid4())
        username = f"testuser_{user_id[:8]}"
        email = f"{username}@example.com"

        # Create user
        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role, created_at)
            VALUES (%s, %s, %s, %s, %s, %s)
        """,
            (user_id, username, email, "hashed_password", "user", datetime.now(UTC)),
        )

        # Verify user exists
        await assert_row_exists(db_with_schema, "users", "id = %s", (user_id,))

        # Verify field values
        await assert_field_equals(
            db_with_schema, "users", "username", username, "id = %s", (user_id,)
        )

        await assert_field_equals(db_with_schema, "users", "email", email, "id = %s", (user_id,))

    async def test_read_user_record(self, db_with_schema):
        """Test reading user records from database."""
        # Create test user
        user_id = str(uuid4())
        username = "readtest_user"

        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (user_id, username, "read@example.com", "hash", "user"),
        )

        # Read user data
        user_data = await get_row_data(db_with_schema, "users", "id = %s", (user_id,))

        assert user_data is not None
        assert str(user_data["id"]) == user_id
        assert user_data["username"] == username
        assert user_data["email"] == "read@example.com"
        assert user_data["role"] == "user"

    async def test_update_user_record(self, db_with_schema):
        """Test updating user records."""
        # Create test user
        user_id = str(uuid4())
        original_username = "original_user"
        updated_username = "updated_user"

        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (user_id, original_username, "update@example.com", "hash", "user"),
        )

        # Verify original state
        await assert_field_equals(
            db_with_schema, "users", "username", original_username, "id = %s", (user_id,)
        )

        # Update username
        await db_with_schema.execute(
            """
            UPDATE users SET username = %s, updated_at = NOW()
            WHERE id = %s
        """,
            (updated_username, user_id),
        )

        # Verify updated state
        await assert_field_equals(
            db_with_schema, "users", "username", updated_username, "id = %s", (user_id,)
        )

    async def test_delete_user_record(self, db_with_schema):
        """Test deleting user records."""
        # Create test user
        user_id = str(uuid4())

        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (user_id, "delete_test", "delete@example.com", "hash", "user"),
        )

        # Verify user exists
        await assert_row_exists(db_with_schema, "users", "id = %s", (user_id,))

        # Delete user
        await db_with_schema.execute(
            """
            DELETE FROM users WHERE id = %s
        """,
            (user_id,),
        )

        # Verify user is deleted
        await assert_row_not_exists(db_with_schema, "users", "id = %s", (user_id,))


@pytest.mark.integration
@pytest.mark.database
class TestJSONBOperations:
    """Test JSONB field operations and queries."""

    async def test_insert_jsonb_profile_data(self, db_with_schema):
        """Test inserting JSONB profile data."""
        user_id = str(uuid4())
        profile_data = {
            "first_name": "John",
            "last_name": "Doe",
            "bio": "Software developer",
            "location": "San Francisco",
            "social_links": {"twitter": "@johndoe", "github": "johndoe"},
        }

        # Insert user with JSONB profile
        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role, profile)
            VALUES (%s, %s, %s, %s, %s, %s)
        """,
            (user_id, "jsonb_user", "jsonb@example.com", "hash", "user", Json(profile_data)),
        )

        # Verify JSONB fields
        await assert_jsonb_field_equals(
            db_with_schema, "users", "profile", "first_name", "John", "id = %s", (user_id,)
        )

        await assert_jsonb_field_equals(
            db_with_schema,
            "users",
            "profile",
            "social_links.twitter",
            "@johndoe",
            "id = %s",
            (user_id,),
        )

    async def test_update_jsonb_fields(self, db_with_schema):
        """Test updating specific JSONB fields."""
        user_id = str(uuid4())
        initial_profile = {"first_name": "Jane", "last_name": "Smith", "bio": "Initial bio"}

        # Insert user
        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role, profile)
            VALUES (%s, %s, %s, %s, %s, %s)
        """,
            (user_id, "update_jsonb", "update@example.com", "hash", "user", Json(initial_profile)),
        )

        # Update specific JSONB field
        await db_with_schema.execute(
            """
            UPDATE users
            SET profile = profile || %s::jsonb
            WHERE id = %s
        """,
            (Json({"bio": "Updated bio", "location": "New York"}), user_id),
        )

        # Verify updates
        await assert_jsonb_field_equals(
            db_with_schema, "users", "profile", "bio", "Updated bio", "id = %s", (user_id,)
        )

        await assert_jsonb_field_equals(
            db_with_schema, "users", "profile", "location", "New York", "id = %s", (user_id,)
        )

        # Verify original fields still exist
        await assert_jsonb_field_equals(
            db_with_schema, "users", "profile", "first_name", "Jane", "id = %s", (user_id,)
        )

    async def test_query_by_jsonb_fields(self, db_with_schema):
        """Test querying records by JSONB field values."""
        user_ids = []

        # Create test users with different profile data
        for i in range(3):
            user_id = str(uuid4())
            user_ids.append(user_id)

            profile = {
                "first_name": f"User{i}",
                "location": "San Francisco" if i < 2 else "New York",
                "skills": ["python", "postgresql"] if i == 1 else ["javascript"],
            }

            await db_with_schema.execute(
                """
                INSERT INTO users (id, username, email, password_hash, role, profile)
                VALUES (%s, %s, %s, %s, %s, %s)
            """,
                (
                    user_id,
                    f"query_user_{i}",
                    f"query{i}@example.com",
                    "hash",
                    "user",
                    Json(profile),
                ),
            )

        # Query by location
        result = await db_with_schema.execute("""
            SELECT COUNT(*) FROM users
            WHERE profile->>'location' = 'San Francisco'
        """)
        san_francisco_count = (await result.fetchone())[0]
        assert san_francisco_count == 2

        # Query by array contains
        result = await db_with_schema.execute("""
            SELECT COUNT(*) FROM users
            WHERE profile->'skills' @> '["python"]'
        """)
        python_users = (await result.fetchone())[0]
        assert python_users == 1


@pytest.mark.integration
@pytest.mark.database
class TestRelationalOperations:
    """Test operations involving relationships between tables."""

    async def test_create_post_with_author(self, db_with_schema):
        """Test creating posts with author relationship."""
        # Create author
        author_id = str(uuid4())
        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (author_id, "author_user", "author@example.com", "hash", "author"),
        )

        # Create post
        post_id = str(uuid4())
        post_title = "Test Post with Author"

        await db_with_schema.execute(
            """
            INSERT INTO posts (id, title, slug, content, author_id, status, published_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s)
        """,
            (
                post_id,
                post_title,
                "test-post-author",
                "Content here",
                author_id,
                "published",
                datetime.now(UTC),
            ),
        )

        # Verify relationships
        await assert_row_exists(
            db_with_schema, "posts", "id = %s AND author_id = %s", (post_id, author_id)
        )

        # Verify join query works
        result = await db_with_schema.execute(
            """
            SELECT p.title, u.username
            FROM posts p
            JOIN users u ON p.author_id = u.id
            WHERE p.id = %s
        """,
            (post_id,),
        )

        row = await result.fetchone()
        assert row[0] == post_title
        assert row[1] == "author_user"

    async def test_create_comments_with_threading(self, db_with_schema):
        """Test creating threaded comments."""
        # Create author and post first
        author_id = str(uuid4())
        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (author_id, "commenter", "commenter@example.com", "hash", "user"),
        )

        post_id = str(uuid4())
        await db_with_schema.execute(
            """
            INSERT INTO posts (id, title, slug, content, author_id, status, published_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s)
        """,
            (
                post_id,
                "Post for Comments",
                "post-comments",
                "Content",
                author_id,
                "published",
                datetime.now(UTC),
            ),
        )

        # Create parent comment
        parent_comment_id = str(uuid4())
        await db_with_schema.execute(
            """
            INSERT INTO comments (id, post_id, author_id, content, status)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (parent_comment_id, post_id, author_id, "Parent comment", "approved"),
        )

        # Create child comment
        child_comment_id = str(uuid4())
        await db_with_schema.execute(
            """
            INSERT INTO comments (id, post_id, author_id, parent_id, content, status)
            VALUES (%s, %s, %s, %s, %s, %s)
        """,
            (child_comment_id, post_id, author_id, parent_comment_id, "Child comment", "approved"),
        )

        # Verify threading relationship
        await assert_field_equals(
            db_with_schema,
            "comments",
            "parent_id",
            parent_comment_id,
            "id = %s",
            (child_comment_id,),
        )

        # Verify hierarchical query
        result = await db_with_schema.execute(
            """
            WITH RECURSIVE comment_tree AS (
                -- Root comments
                SELECT id, content, parent_id, 0 as level
                FROM comments
                WHERE post_id = %s AND parent_id IS NULL

                UNION ALL

                -- Child comments
                SELECT c.id, c.content, c.parent_id, ct.level + 1
                FROM comments c
                JOIN comment_tree ct ON c.parent_id = ct.id
            )
            SELECT COUNT(*) FROM comment_tree
        """,
            (post_id,),
        )

        comment_count = (await result.fetchone())[0]
        assert comment_count == 2  # Parent and child

    async def test_cascade_delete_operations(self, db_with_schema):
        """Test cascade delete operations."""
        # Create user, post, and comment
        user_id = str(uuid4())
        post_id = str(uuid4())
        comment_id = str(uuid4())

        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (user_id, "cascade_user", "cascade@example.com", "hash", "user"),
        )

        await db_with_schema.execute(
            """
            INSERT INTO posts (id, title, slug, content, author_id, status, published_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s)
        """,
            (
                post_id,
                "Cascade Post",
                "cascade-post",
                "Content",
                user_id,
                "published",
                datetime.now(UTC),
            ),
        )

        await db_with_schema.execute(
            """
            INSERT INTO comments (id, post_id, author_id, content, status)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (comment_id, post_id, user_id, "Test comment", "approved"),
        )

        # Verify all records exist
        await assert_row_exists(db_with_schema, "users", "id = %s", (user_id,))
        await assert_row_exists(db_with_schema, "posts", "id = %s", (post_id,))
        await assert_row_exists(db_with_schema, "comments", "id = %s", (comment_id,))

        # Delete post (should cascade delete comments)
        await db_with_schema.execute("DELETE FROM posts WHERE id = %s", (post_id,))

        # Verify post and comment are deleted
        await assert_row_not_exists(db_with_schema, "posts", "id = %s", (post_id,))
        await assert_row_not_exists(db_with_schema, "comments", "id = %s", (comment_id,))

        # User should still exist
        await assert_row_exists(db_with_schema, "users", "id = %s", (user_id,))


@pytest.mark.integration
@pytest.mark.database
class TestTransactionHandling:
    """Test database transaction handling."""

    async def test_transaction_rollback_on_error(self, db_with_schema):
        """Test that transactions rollback properly on errors."""
        user_id = str(uuid4())

        # Start transaction
        await db_with_schema.execute("BEGIN")

        try:
            # Insert user successfully
            await db_with_schema.execute(
                """
                INSERT INTO users (id, username, email, password_hash, role)
                VALUES (%s, %s, %s, %s, %s)
            """,
                (user_id, "transaction_user", "transaction@example.com", "hash", "user"),
            )

            # This should fail due to duplicate username
            await db_with_schema.execute(
                """
                INSERT INTO users (id, username, email, password_hash, role)
                VALUES (%s, %s, %s, %s, %s)
            """,
                (str(uuid4()), "transaction_user", "different@example.com", "hash", "user"),
            )

            await db_with_schema.execute("COMMIT")

        except Exception:
            # Rollback on error
            await db_with_schema.execute("ROLLBACK")

        # User should not exist due to rollback
        await assert_row_not_exists(db_with_schema, "users", "id = %s", (user_id,))

    async def test_transaction_commit_success(self, db_with_schema):
        """Test successful transaction commit."""
        user_id = str(uuid4())
        post_id = str(uuid4())

        # Start transaction
        await db_with_schema.execute("BEGIN")

        try:
            # Insert user
            await db_with_schema.execute(
                """
                INSERT INTO users (id, username, email, password_hash, role)
                VALUES (%s, %s, %s, %s, %s)
            """,
                (user_id, "commit_user", "commit@example.com", "hash", "user"),
            )

            # Insert post
            await db_with_schema.execute(
                """
                INSERT INTO posts (id, title, slug, content, author_id, status)
                VALUES (%s, %s, %s, %s, %s, %s)
            """,
                (post_id, "Commit Post", "commit-post", "Content", user_id, "draft"),
            )

            # Commit transaction
            await db_with_schema.execute("COMMIT")

        except Exception:
            await db_with_schema.execute("ROLLBACK")
            raise

        # Both records should exist
        await assert_row_exists(db_with_schema, "users", "id = %s", (user_id,))
        await assert_row_exists(db_with_schema, "posts", "id = %s", (post_id,))


@pytest.mark.integration
@pytest.mark.database
class TestDatabaseConstraints:
    """Test database constraints and validation."""

    async def test_unique_constraint_enforcement(self, db_with_schema):
        """Test that unique constraints are enforced."""
        username = "unique_test_user"

        # Insert first user
        await db_with_schema.execute(
            """
            INSERT INTO users (id, username, email, password_hash, role)
            VALUES (%s, %s, %s, %s, %s)
        """,
            (str(uuid4()), username, "first@example.com", "hash", "user"),
        )

        # Try to insert second user with same username (should fail)
        with pytest.raises(Exception):  # psycopg will raise IntegrityError
            await db_with_schema.execute(
                """
                INSERT INTO users (id, username, email, password_hash, role)
                VALUES (%s, %s, %s, %s, %s)
            """,
                (str(uuid4()), username, "second@example.com", "hash", "user"),
            )

    async def test_foreign_key_constraint_enforcement(self, db_with_schema):
        """Test that foreign key constraints are enforced."""
        # Try to create post with non-existent author
        fake_author_id = str(uuid4())

        with pytest.raises(Exception):  # Should fail with foreign key violation
            await db_with_schema.execute(
                """
                INSERT INTO posts (id, title, slug, content, author_id, status)
                VALUES (%s, %s, %s, %s, %s, %s)
            """,
                (str(uuid4()), "Orphan Post", "orphan-post", "Content", fake_author_id, "draft"),
            )

    async def test_check_constraint_enforcement(self, db_with_schema):
        """Test that check constraints are enforced."""
        # Try to create user with invalid email format (should fail)
        with pytest.raises(Exception):
            await db_with_schema.execute(
                """
                INSERT INTO users (id, username, email, password_hash, role)
                VALUES (%s, %s, %s, %s, %s)
            """,
                (str(uuid4()), "invalid_email_user", "not-an-email", "hash", "user"),
            )
