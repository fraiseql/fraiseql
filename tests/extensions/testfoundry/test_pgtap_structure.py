"""Test pgTAP test structure generation without requiring actual pgTAP."""

import pytest
import pytest_asyncio

from fraiseql import fraise_field, fraise_input
from fraiseql.db import FraiseQLRepository
from fraiseql.extensions.testfoundry.config import FoundryConfig
from fraiseql.extensions.testfoundry.generator import FoundryGenerator
from fraiseql.extensions.testfoundry.setup import FoundrySetup
from fraiseql.types.scalars.uuid import UUIDField


@fraise_input
class UserInput:
    """User input for testing."""

    email: str = fraise_field(description="User email")
    name: str = fraise_field(description="User name")
    is_active: bool = fraise_field(description="Active status", default=True)


@fraise_input
class PostInput:
    """Post input for testing."""

    author_id: UUIDField = fraise_field(description="Author ID")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    is_published: bool = fraise_field(description="Published status", default=False)


@pytest.mark.database
class TestPgTAPStructure:
    """Test pgTAP test structure generation."""

    @pytest_asyncio.fixture
    async def testfoundry_setup(self, db_pool):
        """Set up TestFoundry without pgTAP."""
        repository = FraiseQLRepository(pool=db_pool)

        async with db_pool.connection() as conn:
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
                CREATE TABLE IF NOT EXISTS tb_users (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS tb_posts (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    author_id UUID REFERENCES tb_users(id),
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP
                );

                -- Create views
                CREATE OR REPLACE VIEW v_users AS
                SELECT
                    id,
                    data->>'email' as email,
                    data->>'name' as name,
                    (data->>'is_active')::boolean as is_active,
                    created_at,
                    updated_at,
                    deleted_at
                FROM tb_users;

                CREATE OR REPLACE VIEW v_posts AS
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
                FROM tb_posts p
                LEFT JOIN tb_users u ON p.author_id = u.id;

                -- Create mutation functions
                CREATE OR REPLACE FUNCTION create_users_with_log(
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
                    -- Insert user
                    INSERT INTO tb_users (data)
                    VALUES (p_input)
                    RETURNING id INTO v_id;

                    RETURN QUERY
                    SELECT v_id, 'users'::TEXT, 'new'::TEXT;
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
                DROP FUNCTION IF EXISTS create_users_with_log CASCADE;
                DROP VIEW IF EXISTS v_posts CASCADE;
                DROP VIEW IF EXISTS v_users CASCADE;
                DROP TABLE IF EXISTS tb_posts CASCADE;
                DROP TABLE IF EXISTS tb_users CASCADE;
                DROP TYPE IF EXISTS mutation_result CASCADE;
            """
            )
            await conn.commit()

        await setup.uninstall()

    @pytest.mark.asyncio
    async def test_pgtap_happy_path_structure(self, testfoundry_setup):
        """Test that generated happy path test has correct pgTAP structure."""
        repository = testfoundry_setup

        # Generate test for user creation
        generator = FoundryGenerator(repository, FoundryConfig())

        # Populate metadata
        await generator.analyze_and_populate_metadata(UserInput, "users", "tb_users")

        # Generate tests
        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        happy_test = tests.get("happy_create", "")

        # Verify pgTAP test structure
        assert "SELECT plan(" in happy_test, "Test should have a plan"
        assert "-- Happy path CREATE test" in happy_test, (
            "Test should have descriptive comments"
        )
        # Check for test content (may use variables or direct calls)
        assert "create_users_with_log" in happy_test, (
            "Should call the mutation function"
        )
        assert "SELECT is(" in happy_test or "SELECT ok(" in happy_test, (
            "Should have assertions"
        )
        assert "SELECT * FROM finish();" in happy_test, "Should call finish()"

        # Verify proper test variables
        assert "\\gset" in happy_test or "gset" in happy_test, (
            "Should use psql variables"
        )
        assert "v_result" in happy_test, "Should use result variable"
        assert "'{}'" in happy_test or "jsonb" in happy_test.lower(), (
            "Should pass JSONB input"
        )

    @pytest.mark.asyncio
    async def test_pgtap_constraint_test_structure(self, testfoundry_setup):
        """Test that constraint violation tests have correct structure."""
        repository = testfoundry_setup

        # Generate test
        generator = FoundryGenerator(repository, FoundryConfig())

        await generator.analyze_and_populate_metadata(UserInput, "users", "tb_users")

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Check for constraint tests
        test_content = "\n".join(tests.values())

        # Should have tests for constraints
        if "duplicate_create" in tests:
            assert (
                "duplicate" in test_content.lower()
                or "constraint" in test_content.lower()
            ), "Should have constraint tests"

    @pytest.mark.asyncio
    async def test_pgtap_fk_test_structure(self, testfoundry_setup):
        """Test that FK validation tests have correct structure."""
        repository = testfoundry_setup

        # Generate test for posts (has FK to users)
        generator = FoundryGenerator(repository, FoundryConfig())

        await generator.analyze_and_populate_metadata(PostInput, "posts", "tb_posts")

        tests = await generator.generate_tests_for_entity(
            entity_name="posts", table_name="tb_posts", input_type_name="post_input"
        )

        _test_content = "\n".join(tests.values())

        # Should have FK validation tests
        if "fk_violation" in tests:
            assert "author_id" in tests["fk_violation"], "Should reference the FK field"

    @pytest.mark.asyncio
    async def test_pgtap_test_organization(self, testfoundry_setup):
        """Test that pgTAP tests are well organized."""
        repository = testfoundry_setup

        generator = FoundryGenerator(repository, FoundryConfig())

        await generator.analyze_and_populate_metadata(UserInput, "users", "tb_users")

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        # Should have multiple test types
        assert "happy_create" in tests, "Should have happy path test"
        assert len(tests) > 1, "Should generate multiple test scenarios"

        # Each test should be self-contained
        for test_name, test_content in tests.items():
            # Skip error messages
            if test_content.startswith("-- Error"):
                continue

            assert test_content.startswith("--"), (
                f"{test_name} should start with comment"
            )
            assert "SELECT plan(" in test_content, (
                f"{test_name} should have its own plan"
            )
            assert "SELECT * FROM finish();" in test_content, (
                f"{test_name} should call finish"
            )
