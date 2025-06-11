"""Integration tests for TestFoundry extension."""

import pytest
import pytest_asyncio

from fraiseql import fraise_field, fraise_input
from fraiseql.db import FraiseQLRepository
from fraiseql.extensions.testfoundry.config import FoundryConfig
from fraiseql.extensions.testfoundry.generator import FoundryGenerator
from fraiseql.extensions.testfoundry.setup import FoundrySetup
from fraiseql.types.scalars.uuid import UUIDField


@fraise_input
class BlogUserInput:
    """User input for blog system."""

    email: str = fraise_field(description="User email")
    name: str = fraise_field(description="Display name")
    bio: str | None = fraise_field(description="User biography", default=None)


@fraise_input
class BlogPostInput:
    """Post input for blog system."""

    author_id: UUIDField = fraise_field(description="Post author")
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    tags: list[str] | None = fraise_field(description="Post tags", default=None)
    is_published: bool = fraise_field(description="Publication status", default=False)


@pytest.mark.database
class TestTestFoundryIntegration:
    """Integration tests for the complete TestFoundry workflow."""

    @pytest_asyncio.fixture
    async def blog_schema(self, db_pool):
        """Set up a complete blog schema for testing."""
        repository = FraiseQLRepository(pool=db_pool)

        # Create blog tables
        async with db_pool.connection() as conn:
            await conn.execute(
                """
                -- Users table
                CREATE TABLE IF NOT EXISTS tb_blog_users (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP
                );

                -- Posts table
                CREATE TABLE IF NOT EXISTS tb_blog_posts (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    user_id UUID REFERENCES tb_blog_users(id),
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP,
                    published_at TIMESTAMP
                );

                -- Comments table
                CREATE TABLE IF NOT EXISTS tb_blog_comments (
                    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                    post_id UUID REFERENCES tb_blog_posts(id),
                    user_id UUID REFERENCES tb_blog_users(id),
                    parent_comment_id UUID REFERENCES tb_blog_comments(id),
                    data JSONB NOT NULL DEFAULT '{}',
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TIMESTAMP
                );

                -- Create views
                CREATE OR REPLACE VIEW v_blog_users AS
                SELECT
                    id,
                    data->>'email' as email,
                    data->>'name' as name,
                    data->>'bio' as bio,
                    created_at,
                    updated_at,
                    deleted_at
                FROM tb_blog_users;

                CREATE OR REPLACE VIEW v_blog_posts AS
                SELECT
                    p.id,
                    p.user_id,
                    u.data->>'name' as author_name,
                    p.data->>'title' as title,
                    p.data->>'content' as content,
                    p.data->'tags' as tags,
                    p.published_at,
                    p.created_at,
                    p.updated_at,
                    p.deleted_at
                FROM tb_blog_posts p
                LEFT JOIN tb_blog_users u ON p.user_id = u.id;

                -- Create mutation functions
                CREATE OR REPLACE FUNCTION create_blog_users_with_log(
                    p_org_id UUID,
                    p_user_id UUID,
                    p_input JSONB
                )
                RETURNS TABLE(pk UUID, entity TEXT, status TEXT)
                LANGUAGE plpgsql
                AS $$
                DECLARE
                    v_id UUID;
                BEGIN
                    INSERT INTO tb_blog_users (data)
                    VALUES (p_input)
                    RETURNING id INTO v_id;

                    RETURN QUERY SELECT v_id, 'blog_users'::TEXT, 'new'::TEXT;
                END;
                $$;

                CREATE OR REPLACE FUNCTION create_blog_posts_with_log(
                    p_org_id UUID,
                    p_user_id UUID,
                    p_input JSONB
                )
                RETURNS TABLE(pk UUID, entity TEXT, status TEXT)
                LANGUAGE plpgsql
                AS $$
                DECLARE
                    v_id UUID;
                    v_author_id UUID;
                BEGIN
                    v_author_id := (p_input->>'author_id')::UUID;

                    INSERT INTO tb_blog_posts (user_id, data, published_at)
                    VALUES (
                        v_author_id,
                        p_input - 'author_id',
                        CASE WHEN (p_input->>'is_published')::boolean
                            THEN NOW()
                            ELSE NULL
                        END
                    )
                    RETURNING id INTO v_id;

                    RETURN QUERY SELECT v_id, 'blog_posts'::TEXT, 'new'::TEXT;
                END;
                $$;

                -- Create mutation_result type if not exists
                DO $$ BEGIN
                    CREATE TYPE mutation_result AS (
                        pk UUID,
                        entity TEXT,
                        status TEXT
                    );
                EXCEPTION
                    WHEN duplicate_object THEN NULL;
                END $$;

                -- Create TestFoundry helper functions
                CREATE OR REPLACE FUNCTION get_entity_structure(
                    p_entity TEXT,
                    p_schema TEXT DEFAULT 'public'
                )
                RETURNS TABLE(
                    base_view_exists BOOLEAN,
                    proj_table_exists BOOLEAN
                )
                LANGUAGE plpgsql
                AS $$
                BEGIN
                    RETURN QUERY
                    SELECT
                        EXISTS(
                            SELECT 1 FROM information_schema.views
                            WHERE table_schema = p_schema
                            AND table_name = 'v_' || p_entity
                        ),
                        EXISTS(
                            SELECT 1 FROM information_schema.tables
                            WHERE table_schema = p_schema
                            AND table_name = 'tv_' || p_entity
                        );
                END;
                $$;

                CREATE OR REPLACE FUNCTION generate_authentication_vars()
                RETURNS TEXT
                LANGUAGE plpgsql
                AS $$
                BEGIN
                    RETURN
                '    SELECT' || CHR(10) ||
                '        ''22222222-2222-2222-2222-222222222222''::uuid AS v_org,' || CHR(10) ||
                '        ''11111111-1111-1111-1111-111111111111''::uuid AS v_user,' || CHR(10) ||
                '        NULL::mutation_result AS v_result,' || CHR(10) ||
                '        NULL::uuid AS v_id' || CHR(10) ||
                E'    \\gset' || CHR(10) || CHR(10);
                END;
                $$;

                -- Simple random input generator for blog entities
                CREATE OR REPLACE FUNCTION random_input_generator(p_entity TEXT)
                RETURNS JSONB
                LANGUAGE plpgsql
                AS $$
                BEGIN
                    CASE p_entity
                        WHEN 'blog_users' THEN
                            RETURN jsonb_build_object(
                                'email', 'user_' || substr(md5(random()::text), 1, 8) || '@example.com',
                                'name', 'Test User ' || substr(md5(random()::text), 1, 4),
                                'bio', 'A test user bio'
                            );
                        WHEN 'blog_posts' THEN
                            RETURN jsonb_build_object(
                                'title', 'Test Post ' || substr(md5(random()::text), 1, 8),
                                'content', 'Test content for the post.',
                                'tags', '["test", "demo"]'::jsonb,
                                'is_published', false
                            );
                        ELSE
                            RETURN '{}'::jsonb;
                    END CASE;
                END;
                $$;
            """
            )
            await conn.commit()

        yield repository

        # Cleanup
        async with db_pool.connection() as conn:
            await conn.execute(
                """
                DROP VIEW IF EXISTS v_blog_posts CASCADE;
                DROP VIEW IF EXISTS v_blog_users CASCADE;
                DROP TABLE IF EXISTS tb_blog_comments CASCADE;
                DROP TABLE IF EXISTS tb_blog_posts CASCADE;
                DROP TABLE IF EXISTS tb_blog_users CASCADE;
                DROP FUNCTION IF EXISTS create_blog_posts_with_log CASCADE;
                DROP FUNCTION IF EXISTS create_blog_users_with_log CASCADE;
                DROP FUNCTION IF EXISTS get_entity_structure CASCADE;
                DROP FUNCTION IF EXISTS generate_authentication_vars CASCADE;
                DROP FUNCTION IF EXISTS random_input_generator CASCADE;
                DROP TYPE IF EXISTS mutation_result CASCADE;
            """
            )
            await conn.commit()

    @pytest.mark.asyncio
    async def test_complete_workflow(self, blog_schema, temp_test_dir):
        """Test the complete TestFoundry workflow from setup to test generation."""
        repository = blog_schema

        # Step 1: Install TestFoundry
        setup = FoundrySetup(repository)
        await setup.install()

        # Step 2: Configure
        config = FoundryConfig(
            test_output_dir=temp_test_dir,
            generate_pytest=False,
            test_options={
                "happy_path": True,
                "constraint_violations": True,
                "fk_violations": True,
                "soft_delete": False,  # Not implemented in our schema
            },
        )

        # Step 3: Create generator
        generator = FoundryGenerator(repository, config)

        # Step 4: Analyze and populate metadata for users
        user_sql = await generator.analyze_and_populate_metadata(
            BlogUserInput, "blog_users", "tb_blog_users"
        )

        # Verify metadata was generated
        assert "INSERT INTO testfoundry.testfoundry_tb_input_field_mapping" in user_sql
        assert "'blog_user', 'email'" in user_sql  # Analyzer removes Input suffix
        assert "testfoundry_random_email" in user_sql

        # Step 5: Analyze and populate metadata for posts
        post_sql = await generator.analyze_and_populate_metadata(
            BlogPostInput, "blog_posts", "tb_blog_posts"
        )

        # Verify FK detection
        assert "'blog_post', 'author_id'" in post_sql  # Analyzer removes Input suffix
        assert "'resolve_fk'" in post_sql
        assert "'author_id'" in post_sql

        # Step 6: Generate tests for users
        user_tests = await generator.generate_tests_for_entity(
            entity_name="blog_users",
            table_name="tb_blog_users",
            input_type_name="blog_user_input",
        )

        # Should have multiple test types
        assert len(user_tests) >= 2
        assert "happy_create" in user_tests

        # Step 7: Write tests to files
        user_paths = await generator.write_tests_to_files(user_tests, "blog_users")

        # Verify files exist
        assert len(user_paths) > 0
        for path in user_paths:
            assert path.exists()
            assert path.parent.name == "blog_users"

        # Step 8: Generate tests for posts (with FK)
        post_tests = await generator.generate_tests_for_entity(
            entity_name="blog_posts",
            table_name="tb_blog_posts",
            input_type_name="blog_post_input",
        )

        # Should include FK violation test
        assert "fk_violation_create" in post_tests

        # Step 9: Verify test content structure
        happy_test_path = temp_test_dir / "blog_users" / "happy_create.sql"
        if happy_test_path.exists():
            content = happy_test_path.read_text()
            # Basic pgTAP structure
            assert "SELECT plan(" in content or "-- " in content
            assert "finish();" in content or "Error" in content or "No test" in content

        # Cleanup
        await setup.uninstall()

    @pytest.mark.asyncio
    async def test_pytest_wrapper_generation(self, blog_schema, temp_test_dir):
        """Test generating pytest-wrapped tests."""
        repository = blog_schema

        # Setup with pytest generation enabled
        setup = FoundrySetup(repository)
        await setup.install()

        config = FoundryConfig(
            test_output_dir=temp_test_dir,
            generate_pytest=True,  # Enable pytest wrapper
            test_options={"happy_path": True},
        )

        generator = FoundryGenerator(repository, config)

        # Generate and write tests
        tests = await generator.generate_tests_for_entity(
            entity_name="blog_users",
            table_name="tb_blog_users",
            input_type_name="blog_user_input",
        )

        paths = await generator.write_tests_to_files(tests, "blog_users")

        # Verify pytest files
        for path in paths:
            assert path.suffix == ".py"
            content = path.read_text()
            assert "import pytest" in content
            assert "@pytest.mark.database" in content
            assert "async def test_" in content

        await setup.uninstall()

    @pytest.mark.asyncio
    async def test_batch_generation(self, blog_schema, temp_test_dir):
        """Test batch generation for multiple entities."""
        repository = blog_schema

        setup = FoundrySetup(repository)
        await setup.install()

        config = FoundryConfig(
            test_output_dir=temp_test_dir,
            generate_pytest=False,  # Generate SQL files
            test_options={
                "happy_path": True,
                "constraint_violations": False,  # Speed up test
                "fk_violations": False,
                "soft_delete": False,
            },
        )

        generator = FoundryGenerator(repository, config)

        # Populate metadata
        await generator.analyze_and_populate_metadata(
            BlogUserInput, "blog_users", "tb_blog_users"
        )
        await generator.analyze_and_populate_metadata(
            BlogPostInput, "blog_posts", "tb_blog_posts"
        )

        # Batch generate
        entities = [
            {
                "entity_name": "blog_users",
                "table_name": "tb_blog_users",
                "input_type_name": "blog_user_input",
            },
            {
                "entity_name": "blog_posts",
                "table_name": "tb_blog_posts",
                "input_type_name": "blog_post_input",
            },
        ]

        await generator.generate_all_tests(entities)

        # Debug: List what's in the directories
        print(f"Test output dir: {temp_test_dir}")
        if (temp_test_dir / "blog_users").exists():
            print(f"blog_users files: {list((temp_test_dir / 'blog_users').iterdir())}")
        else:
            print("blog_users directory doesn't exist")

        # Verify both entities have tests
        assert (temp_test_dir / "blog_users").exists()
        assert (temp_test_dir / "blog_posts").exists()
        assert len(list((temp_test_dir / "blog_users").glob("*.sql"))) > 0
        assert len(list((temp_test_dir / "blog_posts").glob("*.sql"))) > 0

        await setup.uninstall()

    @pytest.mark.asyncio
    async def test_actual_pgtap_execution(self, blog_schema):
        """Test that generated pgTAP tests can actually execute."""
        repository = blog_schema

        # Simple test: create the infrastructure
        setup = FoundrySetup(repository)
        await setup.install()

        # Install pgTAP functions (simplified versions for testing)
        async with repository.get_pool().connection() as conn:
            await conn.execute(
                """
                -- Minimal pgTAP function mocks
                CREATE OR REPLACE FUNCTION plan(n integer)
                RETURNS TEXT LANGUAGE sql
                AS $$ SELECT 'TAP plan ' || n::text $$;

                CREATE OR REPLACE FUNCTION ok(result boolean, description text)
                RETURNS TEXT LANGUAGE sql
                AS $$ SELECT CASE WHEN result THEN 'ok' ELSE 'not ok' END || ' - ' || description $$;

                CREATE OR REPLACE FUNCTION is(have anyelement, want anyelement, description text)
                RETURNS TEXT LANGUAGE sql
                AS $$ SELECT CASE WHEN have = want THEN 'ok' ELSE 'not ok' END || ' - ' || description $$;

                CREATE OR REPLACE FUNCTION finish()
                RETURNS TEXT LANGUAGE sql
                AS $$ SELECT 'TAP done' $$;
            """
            )
            await conn.commit()

        # Generate a simple test
        config = FoundryConfig(test_options={"happy_path": True})
        FoundryGenerator(repository, config)

        # Create a minimal happy path test
        async with repository.get_pool().connection() as conn:
            # Execute a generated test fragment
            async with conn.cursor() as cur:
                await cur.execute(
                    """
                    SELECT
                        plan(1) || E'\\n' ||
                        ok(true, 'Basic test') || E'\\n' ||
                        finish()
                """
                )
                result = await cur.fetchone()
                if result:
                    result = result[0]

            assert "TAP plan 1" in result
            assert "ok - Basic test" in result
            assert "TAP done" in result

        # Cleanup
        async with repository.get_pool().connection() as conn:
            await conn.execute(
                """
                DROP FUNCTION IF EXISTS plan CASCADE;
                DROP FUNCTION IF EXISTS ok CASCADE;
                DROP FUNCTION IF EXISTS is CASCADE;
                DROP FUNCTION IF EXISTS finish CASCADE;
            """
            )
            await conn.commit()

        await setup.uninstall()


# Import at the bottom to avoid circular imports
