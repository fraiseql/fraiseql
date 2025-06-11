"""Test fixtures for TestFoundry extension tests."""

import shutil
import tempfile
from pathlib import Path

import pytest
import pytest_asyncio

from fraiseql.db import FraiseQLRepository
from fraiseql.extensions.testfoundry.config import FoundryConfig
from fraiseql.extensions.testfoundry.setup import FoundrySetup


@pytest_asyncio.fixture
async def testfoundry_repository(db_pool):
    """Create a repository with TestFoundry installed."""
    repository = FraiseQLRepository(pool=db_pool)

    # Install TestFoundry
    setup = FoundrySetup(repository)
    try:
        # Try to uninstall first in case it already exists
        await setup.uninstall()
    except Exception:
        pass  # Ignore if doesn't exist

    await setup.install()

    yield repository

    # Cleanup
    await setup.uninstall()


@pytest.fixture
def temp_test_dir():
    """Create a temporary directory for test output."""
    temp_dir = tempfile.mkdtemp()
    yield Path(temp_dir)
    shutil.rmtree(temp_dir)


@pytest.fixture
def test_config(temp_test_dir):
    """Create a test configuration."""
    return FoundryConfig(
        test_output_dir=temp_test_dir,
        generate_pytest=False,  # Default to raw SQL for most tests
        test_options={
            "happy_path": True,
            "constraint_violations": True,
            "fk_violations": True,
            "soft_delete": True,
            "blocked_delete": False,
            "authorization": False,
        },
    )


@pytest_asyncio.fixture
async def setup_test_schema(testfoundry_repository):
    """Set up a test schema with tables and views."""
    async with testfoundry_repository.get_pool().connection() as conn:
        # Create test tables
        await conn.execute(
            """
            -- Create mutation_result type
            CREATE TYPE mutation_result AS (
                pk UUID,
                entity TEXT,
                status TEXT
            );

            CREATE TABLE IF NOT EXISTS public.tb_users (
                id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                data JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS public.tb_posts (
                id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
                user_id UUID REFERENCES tb_users(id),
                data JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP
            );
        """
        )

        # Create views
        await conn.execute(
            """
            CREATE OR REPLACE VIEW public.v_users AS
            SELECT
                id,
                data->>'email' as email,
                data->>'name' as name,
                data->>'bio' as bio,
                created_at,
                deleted_at
            FROM public.tb_users;

            CREATE OR REPLACE VIEW public.v_posts AS
            SELECT
                p.id,
                p.user_id,
                u.data->>'name' as author_name,
                p.data->>'title' as title,
                p.data->>'content' as content,
                p.data->>'tags' as tags,
                p.created_at,
                p.deleted_at
            FROM public.tb_posts p
            LEFT JOIN public.tb_users u ON p.user_id = u.id;
        """
        )

        # Create mutation functions
        await conn.execute(
            """
            CREATE OR REPLACE FUNCTION public.create_users_with_log(
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

                -- Return result
                RETURN QUERY
                SELECT v_id, 'users'::TEXT, 'new'::TEXT;
            END;
            $$;

            CREATE OR REPLACE FUNCTION public.create_posts_with_log(
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
                -- Extract author_id from input
                v_author_id := (p_input->>'author_id')::UUID;

                -- Insert post
                INSERT INTO tb_posts (user_id, data)
                VALUES (v_author_id, p_input - 'author_id')
                RETURNING id INTO v_id;

                -- Return result
                RETURN QUERY
                SELECT v_id, 'posts'::TEXT, 'new'::TEXT;
            END;
            $$;
        """
        )

        # Create helper functions that TestFoundry expects
        # First create the functions in the testfoundry schema since that's where they'll be looked for
        await conn.execute(
            """
            -- Entity structure helper in testfoundry schema
            CREATE OR REPLACE FUNCTION testfoundry.get_entity_structure(
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

            -- Also create in public for compatibility
            CREATE OR REPLACE FUNCTION public.get_entity_structure(
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

            -- Authentication variables generator
            CREATE OR REPLACE FUNCTION testfoundry.generate_authentication_vars()
            RETURNS TEXT
            LANGUAGE plpgsql
            AS $func$
            BEGIN
                RETURN
            '    SELECT' || CHR(10) ||
            '        ''22222222-2222-2222-2222-222222222222''::uuid AS v_org,' || CHR(10) ||
            '        ''11111111-1111-1111-1111-111111111111''::uuid AS v_user,' || CHR(10) ||
            '        NULL::mutation_result AS v_result,' || CHR(10) ||
            '        NULL::uuid AS v_id' || CHR(10) ||
            E'    \\gset' || CHR(10) || CHR(10);
            END;
            $func$;

            CREATE OR REPLACE FUNCTION public.generate_authentication_vars()
            RETURNS TEXT
            LANGUAGE plpgsql
            AS $func$
            BEGIN
                RETURN
            '    SELECT' || CHR(10) ||
            '        ''22222222-2222-2222-2222-222222222222''::uuid AS v_org,' || CHR(10) ||
            '        ''11111111-1111-1111-1111-111111111111''::uuid AS v_user,' || CHR(10) ||
            '        NULL::mutation_result AS v_result,' || CHR(10) ||
            '        NULL::uuid AS v_id' || CHR(10) ||
            E'    \\gset' || CHR(10) || CHR(10);
            END;
            $func$;

            -- Random input generator wrapper (delegates to testfoundry_generate_random_input)
            CREATE OR REPLACE FUNCTION testfoundry.random_input_generator(p_entity TEXT)
            RETURNS JSONB
            LANGUAGE plpgsql
            AS $$
            BEGIN
                -- Try to use the real function if available, otherwise return simple test data
                BEGIN
                    RETURN testfoundry_generate_random_input(p_entity || '_input');
                EXCEPTION
                    WHEN OTHERS THEN
                        -- Fallback for testing
                        CASE p_entity
                            WHEN 'users' THEN
                                RETURN jsonb_build_object(
                                    'email', 'user_' || substr(md5(random()::text), 1, 8) || '@example.com',
                                    'name', 'Test User ' || substr(md5(random()::text), 1, 4),
                                    'bio', 'A test user bio'
                                );
                            WHEN 'posts' THEN
                                RETURN jsonb_build_object(
                                    'title', 'Test Post ' || substr(md5(random()::text), 1, 8),
                                    'content', 'Test content for the post.',
                                    'tags', '["test", "demo"]'::jsonb
                                );
                            ELSE
                                RETURN '{}'::jsonb;
                        END CASE;
                END;
            END;
            $$;

            CREATE OR REPLACE FUNCTION public.random_input_generator(p_entity TEXT)
            RETURNS JSONB
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN testfoundry.random_input_generator(p_entity);
            END;
            $$;
        """
        )

        await conn.commit()

    yield testfoundry_repository

    # Cleanup
    async with testfoundry_repository.get_pool().connection() as conn:
        await conn.execute(
            """
            DROP FUNCTION IF EXISTS public.create_posts_with_log CASCADE;
            DROP FUNCTION IF EXISTS public.create_users_with_log CASCADE;
            DROP FUNCTION IF EXISTS public.random_input_generator CASCADE;
            DROP FUNCTION IF EXISTS public.generate_authentication_vars CASCADE;
            DROP FUNCTION IF EXISTS public.get_entity_structure CASCADE;
            DROP FUNCTION IF EXISTS testfoundry.random_input_generator CASCADE;
            DROP FUNCTION IF EXISTS testfoundry.generate_authentication_vars CASCADE;
            DROP FUNCTION IF EXISTS testfoundry.get_entity_structure CASCADE;
            DROP VIEW IF EXISTS public.v_posts CASCADE;
            DROP VIEW IF EXISTS public.v_users CASCADE;
            DROP TABLE IF EXISTS public.tb_posts CASCADE;
            DROP TABLE IF EXISTS public.tb_users CASCADE;
            DROP TYPE IF EXISTS mutation_result CASCADE;
        """
        )
        await conn.commit()


@pytest_asyncio.fixture
async def populated_metadata(setup_test_schema):
    """Populate TestFoundry metadata for testing."""
    repository = setup_test_schema

    async with repository.get_pool().connection() as conn:
        # Populate field mappings
        await conn.execute(
            """
            INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
            (input_type, field_name, generator_type, fk_mapping_key, random_function, required)
            VALUES
            ('user_input', 'email', 'random', NULL, 'testfoundry_random_email', TRUE),
            ('user_input', 'name', 'random', NULL, NULL, TRUE),
            ('user_input', 'bio', 'random', NULL, NULL, FALSE),
            ('post_input', 'author_id', 'resolve_fk', 'user_id', NULL, TRUE),
            ('post_input', 'title', 'random', NULL, NULL, TRUE),
            ('post_input', 'content', 'random', NULL, NULL, TRUE),
            ('post_input', 'tags', 'random', NULL, 'testfoundry_random_array', FALSE)
            ON CONFLICT DO NOTHING;
        """
        )

        # Populate FK mappings
        await conn.execute(
            """
            INSERT INTO testfoundry.testfoundry_tb_fk_mapping
            (input_type, from_expression, select_field, random_pk_field,
             random_value_field, random_select_where)
            VALUES
            ('user_id', 'public.tb_users', 'id', 'id', 'email', 'deleted_at IS NULL')
            ON CONFLICT DO NOTHING;
        """
        )

        await conn.commit()

    return repository
