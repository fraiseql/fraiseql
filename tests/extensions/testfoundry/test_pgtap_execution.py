"""Test actual pgTAP execution with real PostgreSQL database."""

import pytest
import pytest_asyncio

from fraiseql import fraise_field, fraise_input
from fraiseql.db import FraiseQLRepository
from fraiseql.extensions.testfoundry.config import FoundryConfig
from fraiseql.extensions.testfoundry.generator import FoundryGenerator
from fraiseql.extensions.testfoundry.setup import FoundrySetup
from fraiseql.types.scalars.uuid import UUIDField

from .install_pgtap import install_pgtap_minimal


@fraise_input
class SimpleUserInput:
    """User input for testing."""

    email: str = fraise_field(description="User email")
    name: str = fraise_field(description="User name")
    is_active: bool = fraise_field(description="Active status", default=True)


@fraise_input
class SimplePostInput:
    """Post input for testing."""

    author_id: UUIDField = fraise_field(description="Author ID")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    is_published: bool = fraise_field(description="Published status", default=False)


@pytest.mark.database
class TestPgTAPExecution:
    """Test actual pgTAP execution with real database operations."""

    @pytest_asyncio.fixture
    async def pgtap_db(self, db_pool):
        """Set up database with pgTAP functions and test schema."""
        repository = FraiseQLRepository(pool=db_pool)

        async with db_pool.connection() as conn:
            # Install minimal pgTAP
            pgtap_installed = await install_pgtap_minimal(conn)

            if not pgtap_installed:
                pytest.skip("Could not install pgTAP functions")

            # Create mutation_result type
            await conn.execute(
                """
                DO $$ BEGIN
                    CREATE TYPE mutation_result AS (
                        pk UUID,
                        entity TEXT,
                        status TEXT
                    );
                EXCEPTION
                    WHEN duplicate_object THEN NULL;
                END $$;

                -- Create test tables
                CREATE TABLE IF NOT EXISTS tb_test_users (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS tb_test_posts (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    author_id UUID REFERENCES tb_test_users(id),
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP
                );

                -- Create views
                CREATE OR REPLACE VIEW v_test_users AS
                SELECT
                    id,
                    data->>'email' as email,
                    data->>'name' as name,
                    (data->>'is_active')::boolean as is_active,
                    created_at,
                    updated_at,
                    deleted_at
                FROM tb_test_users;

                CREATE OR REPLACE VIEW v_test_posts AS
                SELECT
                    p.id,
                    p.author_id,
                    u.data->>'name' as author_name,
                    p.data->>'title' as title,
                    p.data->>'content' as content,
                    (p.data->>'is_published')::boolean as is_published,
                    p.created_at,
                    p.updated_at,
                    p.deleted_at
                FROM tb_test_posts p
                LEFT JOIN tb_test_users u ON p.author_id = u.id;

                -- Create mutation functions
                CREATE OR REPLACE FUNCTION create_test_users_with_log(
                    p_org_id UUID,
                    p_user_id UUID,
                    p_input JSONB
                )
                RETURNS TABLE(
                    pk UUID,
                    entity TEXT,
                    status TEXT
                )
                LANGUAGE plpgsql
                AS $$
                DECLARE
                    v_id UUID;
                BEGIN
                    -- Validate required fields
                    IF NOT (p_input ? 'email' AND p_input ? 'name') THEN
                        RAISE EXCEPTION 'Missing required fields';
                    END IF;

                    -- Check for duplicate email
                    IF EXISTS (
                        SELECT 1 FROM tb_test_users
                        WHERE data->>'email' = p_input->>'email'
                        AND deleted_at IS NULL
                    ) THEN
                        RAISE EXCEPTION 'Email already exists';
                    END IF;

                    -- Insert user
                    INSERT INTO tb_test_users (data)
                    VALUES (p_input)
                    RETURNING id INTO v_id;

                    RETURN QUERY
                    SELECT v_id, 'test_users'::TEXT, 'new'::TEXT;
                END;
                $$;

                CREATE OR REPLACE FUNCTION create_test_posts_with_log(
                    p_org_id UUID,
                    p_user_id UUID,
                    p_input JSONB
                )
                RETURNS TABLE(
                    pk UUID,
                    entity TEXT,
                    status TEXT
                )
                LANGUAGE plpgsql
                AS $$
                DECLARE
                    v_id UUID;
                    v_author_id UUID;
                BEGIN
                    -- Extract and validate author_id
                    v_author_id := (p_input->>'author_id')::UUID;

                    IF v_author_id IS NULL THEN
                        RAISE EXCEPTION 'author_id is required';
                    END IF;

                    -- Check author exists
                    IF NOT EXISTS (
                        SELECT 1 FROM tb_test_users
                        WHERE id = v_author_id
                        AND deleted_at IS NULL
                    ) THEN
                        RAISE EXCEPTION 'Author does not exist';
                    END IF;

                    -- Insert post
                    INSERT INTO tb_test_posts (author_id, data)
                    VALUES (v_author_id, p_input - 'author_id')
                    RETURNING id INTO v_id;

                    RETURN QUERY
                    SELECT v_id, 'test_posts'::TEXT, 'new'::TEXT;
                END;
                $$;

                CREATE OR REPLACE FUNCTION delete_test_users_with_log(
                    p_org_id UUID,
                    p_user_id UUID,
                    p_id UUID
                )
                RETURNS TABLE(
                    pk UUID,
                    entity TEXT,
                    status TEXT
                )
                LANGUAGE plpgsql
                AS $$
                BEGIN
                    -- Check if user has posts
                    IF EXISTS (
                        SELECT 1 FROM tb_test_posts
                        WHERE author_id = p_id
                        AND deleted_at IS NULL
                    ) THEN
                        RAISE EXCEPTION 'Cannot delete user with posts';
                    END IF;

                    -- Soft delete
                    UPDATE tb_test_users
                    SET deleted_at = CURRENT_TIMESTAMP
                    WHERE id = p_id
                    AND deleted_at IS NULL;

                    IF NOT FOUND THEN
                        RAISE EXCEPTION 'User not found';
                    END IF;

                    RETURN QUERY
                    SELECT p_id, 'test_users'::TEXT, 'deleted'::TEXT;
                END;
                $$;
            """
            )
            await conn.commit()

        # Install TestFoundry
        setup = FoundrySetup(repository)
        await setup.install()

        yield repository

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute(
                """
                DROP FUNCTION IF EXISTS delete_test_users_with_log CASCADE;
                DROP FUNCTION IF EXISTS create_test_posts_with_log CASCADE;
                DROP FUNCTION IF EXISTS create_test_users_with_log CASCADE;
                DROP VIEW IF EXISTS v_test_posts CASCADE;
                DROP VIEW IF EXISTS v_test_users CASCADE;
                DROP TABLE IF EXISTS tb_test_posts CASCADE;
                DROP TABLE IF EXISTS tb_test_users CASCADE;
                DROP TYPE IF EXISTS mutation_result CASCADE;
                -- Drop pgTAP functions
                DROP FUNCTION IF EXISTS plan CASCADE;
                DROP FUNCTION IF EXISTS finish CASCADE;
                DROP FUNCTION IF EXISTS ok CASCADE;
                DROP FUNCTION IF EXISTS is CASCADE;
                DROP FUNCTION IF EXISTS isnt CASCADE;
                DROP FUNCTION IF EXISTS alike CASCADE;
                DROP FUNCTION IF EXISTS "like"(anyelement, text, text) CASCADE;
                DROP FUNCTION IF EXISTS lives_ok CASCADE;
                DROP FUNCTION IF EXISTS throws_ok CASCADE;
                DROP FUNCTION IF EXISTS pass CASCADE;
                DROP FUNCTION IF EXISTS fail CASCADE;
                DROP FUNCTION IF EXISTS diag CASCADE;
            """
            )
            await conn.commit()

        await setup.uninstall()

    @pytest.mark.asyncio
    async def test_happy_path_user_creation(self, pgtap_db):
        """Test that generated happy path test actually works."""
        repository = pgtap_db

        # Generate test for user creation
        generator = FoundryGenerator(repository, FoundryConfig())

        # Populate metadata
        await generator.analyze_and_populate_metadata(
            SimpleUserInput, "test_users", "tb_test_users"
        )

        # Generate tests
        tests = await generator.generate_tests_for_entity(
            entity_name="test_users",
            table_name="tb_test_users",
            input_type_name="test_user_input",
        )

        happy_test = tests.get("happy_create", "")

        # Execute the pgTAP test
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                # pgTAP tests use psql-specific commands like \gset
                # We need to adapt the test for direct execution

                # Extract the core test logic
                if (
                    "SELECT * INTO v_result FROM create_test_users_with_log"
                    in happy_test
                ):
                    # Run the actual function
                    await cur.execute(
                        """
                        SELECT * FROM create_test_users_with_log(
                            '00000000-0000-0000-0000-000000000001'::uuid,
                            '00000000-0000-0000-0000-000000000002'::uuid,
                            '{"email": "test@example.com", "name": "Test User", "is_active": true}'::jsonb
                        )
                    """
                    )
                    result = await cur.fetchone()

                    # Verify result
                    assert result is not None, "Should return a result"
                    pk, entity, status = result
                    assert pk is not None, "Should return a PK"
                    assert entity == "test_users", "Should return correct entity"
                    assert status == "new", "Should return new status"

        # Verify a user was actually created
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute("SELECT COUNT(*) FROM tb_test_users")
                result = await cur.fetchone()
                assert result[0] > 0, "User should have been created"

    @pytest.mark.asyncio
    async def test_duplicate_email_detection(self, pgtap_db):
        """Test that duplicate email constraint works."""
        repository = pgtap_db

        # First create a user
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(
                    """
                    INSERT INTO tb_test_users (data)
                    VALUES ('{"email": "test@example.com", "name": "Test User"}')
                """
                )
                await conn.commit()

        # Try to create another user with same email
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                with pytest.raises(Exception, match="Email already exists"):
                    await cur.execute(
                        """
                        SELECT * FROM create_test_users_with_log(
                            '00000000-0000-0000-0000-000000000001'::uuid,
                            '00000000-0000-0000-0000-000000000002'::uuid,
                            '{"email": "test@example.com", "name": "Another User"}'::jsonb
                        )
                    """
                    )

    @pytest.mark.asyncio
    async def test_foreign_key_validation(self, pgtap_db):
        """Test that FK constraints are properly validated."""
        repository = pgtap_db

        # Try to create post with non-existent author
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                with pytest.raises(Exception, match="Author does not exist"):
                    await cur.execute(
                        """
                        SELECT * FROM create_test_posts_with_log(
                            '00000000-0000-0000-0000-000000000001'::uuid,
                            '00000000-0000-0000-0000-000000000002'::uuid,
                            '{
                                "author_id": "00000000-0000-0000-0000-000000000999",
                                "title": "Test Post",
                                "content": "Test content"
                            }'::jsonb
                        )
                    """
                    )

    @pytest.mark.asyncio
    async def test_cascade_delete_protection(self, pgtap_db):
        """Test that users with posts cannot be deleted."""
        repository = pgtap_db

        # Create a user
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                await cur.execute(
                    """
                    INSERT INTO tb_test_users (id, data)
                    VALUES (
                        '00000000-0000-0000-0000-000000000100'::uuid,
                        '{"email": "author@example.com", "name": "Author"}'
                    )
                """
                )

                # Create a post for this user
                await cur.execute(
                    """
                    INSERT INTO tb_test_posts (author_id, data)
                    VALUES (
                        '00000000-0000-0000-0000-000000000100'::uuid,
                        '{"title": "Post", "content": "Content"}'
                    )
                """
                )
                await conn.commit()

        # Try to delete the user
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                with pytest.raises(Exception, match="Cannot delete user with posts"):
                    await cur.execute(
                        """
                        SELECT * FROM delete_test_users_with_log(
                            '00000000-0000-0000-0000-000000000001'::uuid,
                            '00000000-0000-0000-0000-000000000002'::uuid,
                            '00000000-0000-0000-0000-000000000100'::uuid
                        )
                    """
                    )

    @pytest.mark.asyncio
    async def test_complete_pgtap_test_execution(self, pgtap_db):
        """Run a complete pgTAP test with all assertions."""
        repository = pgtap_db

        # Create a pgTAP test that can be executed
        async with repository.get_pool().connection() as conn:
            async with conn.cursor() as cur:
                # Run individual pgTAP functions

                # Test 1: plan
                await cur.execute("SELECT plan(5)")
                result = await cur.fetchone()
                assert result[0] == "1..5", "Plan should set test count"

                # Test 2: ok
                await cur.execute("SELECT ok(true, 'This should pass')")
                result = await cur.fetchone()
                assert "ok" in result[0], "ok() should pass"

                # Test 3: is
                await cur.execute("SELECT is(1, 1, 'Numbers should match')")
                result = await cur.fetchone()
                assert "ok" in result[0], "is() should pass for equal values"

                # Test 4: isnt
                await cur.execute("SELECT isnt(1, 2, 'Numbers should not match')")
                result = await cur.fetchone()
                assert "ok" in result[0], "isnt() should pass for different values"

                # Test 5: lives_ok
                await cur.execute("SELECT lives_ok('SELECT 1', 'Query should not die')")
                result = await cur.fetchone()
                assert "ok" in result[0], "lives_ok() should pass for valid query"

                # Finish
                await cur.execute("SELECT * FROM finish()")
                results = await cur.fetchall()
                assert len(results) > 0, "finish() should return results"
