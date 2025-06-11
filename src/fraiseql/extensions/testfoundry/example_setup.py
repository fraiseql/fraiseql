"""Example script showing how to set up TestFoundry for a FraiseQL project."""

import asyncio
import os
from pathlib import Path

from fraiseql.cqrs import CQRSRepository
from fraiseql.extensions.testfoundry import (
    FoundryConfig,
    FoundryGenerator,
    FoundrySetup,
)


async def setup_testfoundry_for_blog():
    """Example: Set up TestFoundry for the blog example."""

    # Database connection
    DATABASE_URL = os.getenv(
        "DATABASE_URL", "postgresql://postgres:postgres@localhost:5432/blog_db"
    )

    # Create repository
    repository = CQRSRepository(DATABASE_URL)

    try:
        # 1. Install TestFoundry schema
        print("Installing TestFoundry...")
        setup = FoundrySetup(repository)
        await setup.install()

        # 2. Populate metadata for blog entities
        print("\nPopulating TestFoundry metadata...")
        await populate_blog_metadata(repository)

        # 3. Generate tests
        print("\nGenerating tests...")
        config = FoundryConfig(
            test_output_dir=Path("tests/generated/testfoundry"), generate_pytest=True
        )
        generator = FoundryGenerator(repository, config)

        # Define blog entities
        blog_entities = [
            {
                "entity_name": "users",
                "table_name": "tb_users",
                "input_type_name": "user_input",
            },
            {
                "entity_name": "posts",
                "table_name": "tb_posts",
                "input_type_name": "post_input",
            },
            {
                "entity_name": "comments",
                "table_name": "tb_comments",
                "input_type_name": "comment_input",
            },
        ]

        await generator.generate_all_tests(blog_entities)

        print("\nTestFoundry setup complete!")

    finally:
        await repository.close()


async def populate_blog_metadata(repository: CQRSRepository):
    """Populate TestFoundry metadata for blog entities."""

    sql = """
    SET search_path TO testfoundry, public;

    -- User input type mapping
    INSERT INTO testfoundry_tb_input_field_mapping
    (input_type, field_name, generator_type, fk_mapping_key, random_function, required)
    VALUES
    ('user_input', 'email', 'random', NULL, 'testfoundry_random_email', TRUE),
    ('user_input', 'name', 'random', NULL, NULL, TRUE),
    ('user_input', 'bio', 'random', NULL, NULL, FALSE),
    ('user_input', 'avatar_url', 'random', NULL, 'testfoundry_random_url', FALSE)
    ON CONFLICT (input_type, field_name) DO NOTHING;

    -- Post input type mapping
    INSERT INTO testfoundry_tb_input_field_mapping
    (input_type, field_name, generator_type, fk_mapping_key, fk_dependency_fields, required)
    VALUES
    ('post_input', 'author_id', 'resolve_fk', 'user_id', NULL, TRUE),
    ('post_input', 'title', 'random', NULL, NULL, TRUE),
    ('post_input', 'content', 'random', NULL, NULL, TRUE),
    ('post_input', 'excerpt', 'random', NULL, NULL, FALSE),
    ('post_input', 'tags', 'random', NULL, NULL, FALSE),
    ('post_input', 'is_published', 'random', NULL, NULL, FALSE)
    ON CONFLICT (input_type, field_name) DO NOTHING;

    -- Comment input type mapping
    INSERT INTO testfoundry_tb_input_field_mapping
    (input_type, field_name, generator_type, fk_mapping_key, fk_dependency_fields, required)
    VALUES
    ('comment_input', 'post_id', 'resolve_fk', 'post_id', NULL, TRUE),
    ('comment_input', 'author_id', 'resolve_fk', 'user_id', NULL, TRUE),
    ('comment_input', 'content', 'random', NULL, NULL, TRUE),
    ('comment_input', 'is_approved', 'random', NULL, NULL, FALSE)
    ON CONFLICT (input_type, field_name) DO NOTHING;

    -- FK mappings
    INSERT INTO testfoundry_tb_fk_mapping
    (input_type, from_expression, select_field, random_pk_field, random_value_field, random_select_where)
    VALUES
    ('user_id', 'tb_users', 'id', 'id', 'email', 'deleted_at IS NULL'),
    ('post_id', 'tb_posts', 'id', 'id', 'title', 'deleted_at IS NULL')
    ON CONFLICT (input_type) DO NOTHING;

    -- Define entity dependencies
    INSERT INTO testfoundry_tb_entity_dependents
    (entity_name, dependent_entity_name, dependency_type, parent_pk_fields, parent_view, child_fk_fields, child_view, deleted_indicator, deleted_value, archive_timestamp_field)
    VALUES
    ('users', 'posts', 'RESTRICT', ARRAY['id'], 'v_users', ARRAY['author_id'], 'v_posts', 'deleted_at', NULL, 'deleted_at'),
    ('posts', 'comments', 'CASCADE', ARRAY['id'], 'v_posts', ARRAY['post_id'], 'v_comments', 'deleted_at', NULL, 'deleted_at')
    ON CONFLICT DO NOTHING;
    """

    await repository.execute(sql)


# Custom random functions that might be needed
CUSTOM_RANDOM_FUNCTIONS = """
-- Random email generator
CREATE OR REPLACE FUNCTION testfoundry_random_email()
RETURNS TEXT AS $$
BEGIN
    RETURN LOWER(
        CONCAT(
            'user_',
            SUBSTRING(MD5(RANDOM()::TEXT), 1, 8),
            '@example.com'
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Random URL generator
CREATE OR REPLACE FUNCTION testfoundry_random_url()
RETURNS TEXT AS $$
BEGIN
    RETURN CONCAT(
        'https://example.com/',
        SUBSTRING(MD5(RANDOM()::TEXT), 1, 16)
    );
END;
$$ LANGUAGE plpgsql;

-- Random email validation
CREATE OR REPLACE FUNCTION testfoundry_random_email_valid()
RETURNS TEXT AS $$
BEGIN
    RETURN testfoundry_random_email();
END;
$$ LANGUAGE plpgsql;
"""


if __name__ == "__main__":
    asyncio.run(setup_testfoundry_for_blog())
