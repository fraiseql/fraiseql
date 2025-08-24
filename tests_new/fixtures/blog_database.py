"""Blog-specific database fixtures for FraiseQL E2E testing.

This module extends the base database fixtures with blog-specific schema setup
and seed data management, following the printoptim_backend patterns.
"""

import os
from pathlib import Path
from typing import List, Optional

import psycopg
import pytest_asyncio


@pytest_asyncio.fixture
async def blog_schema_setup(db_connection) -> psycopg.AsyncConnection:
    """Set up blog database schema and seed data within transaction isolation.

    This fixture loads the complete blog schema including:
    - Extensions and types
    - Command tables (tb_*)
    - Query views (v_*)
    - Common seed data

    All within the transaction-isolated connection, so each test gets a clean state.

    Args:
        db_connection: Base database connection with transaction isolation

    Returns:
        AsyncConnection: Connection with blog schema loaded
    """
    # Get the path to the database setup files
    blog_demo_path = Path(__file__).parent.parent / "e2e" / "blog_demo"

    # Change to the blog demo directory so relative paths work correctly
    original_cwd = os.getcwd()
    try:
        os.chdir(blog_demo_path)

        # Execute each SQL file in order within the transaction
        sql_files = [
            "db/0_schema/00_common/000_extensions.sql",
            "db/0_schema/00_common/001_types.sql",
            "db/0_schema/01_write_side/011_users/01101_tb_user.sql",
            "db/0_schema/01_write_side/012_content/01201_tb_post.sql",
            "db/0_schema/01_write_side/012_content/01202_tb_comment.sql",
            "db/0_schema/01_write_side/013_taxonomy/01301_tb_tag.sql",
            "db/0_schema/01_write_side/014_associations/01401_tb_post_tag.sql",
            "db/0_schema/02_query_side/021_users/02101_v_user.sql",
            "db/0_schema/02_query_side/022_content/02201_v_post.sql",
            "db/0_schema/02_query_side/022_content/02202_v_comment.sql",
            "db/0_schema/02_query_side/023_taxonomy/02301_v_tag.sql",
            "db/1_seed_data/11_seed_common/11001_seed_users.sql",
            "db/1_seed_data/11_seed_common/11002_seed_tags.sql",
            "db/1_seed_data/11_seed_common/11003_seed_posts.sql",
            "db/1_seed_data/11_seed_common/11004_seed_post_tags.sql",
            "db/1_seed_data/11_seed_common/11005_seed_comments.sql",
        ]

        for sql_file in sql_files:
            file_path = Path(sql_file)
            if file_path.exists():
                sql_content = file_path.read_text()
                try:
                    # Execute within the existing transaction
                    async with db_connection.cursor() as cursor:
                        await cursor.execute(sql_content)
                    print(f"✅ Executed {sql_file}")
                except Exception as e:
                    print(f"❌ Error executing {sql_file}: {e}")
                    # Re-raise to fail the test setup
                    raise
            else:
                print(f"⚠️  Warning: SQL file not found: {sql_file}")

        print("✅ Blog schema setup complete with transaction isolation")

    finally:
        os.chdir(original_cwd)

    return db_connection


@pytest_asyncio.fixture
async def blog_with_test_data(
    blog_schema_setup, test_name: Optional[str] = None
) -> psycopg.AsyncConnection:
    """Blog database with additional test-specific seed data.

    Loads the base blog schema plus test-specific seed data based on the test name.

    Args:
        blog_schema_setup: Base blog schema setup
        test_name: Optional test name for loading specific seed data

    Returns:
        AsyncConnection: Connection with blog schema and test data
    """
    if test_name:
        # Load test-specific seed data
        blog_demo_path = Path(__file__).parent.parent / "e2e" / "blog_demo"
        test_seed_path = (
            blog_demo_path / "db" / "1_seed_data" / "12_seed_by_test" / f"{test_name}.sql"
        )

        if test_seed_path.exists():
            test_seed_sql = test_seed_path.read_text()
            await blog_schema_setup.execute(test_seed_sql)

    return blog_schema_setup


async def load_sql_file(connection: psycopg.AsyncConnection, file_path: Path) -> None:
    """Load and execute SQL file.

    Args:
        connection: Database connection
        file_path: Path to SQL file

    Raises:
        FileNotFoundError: If SQL file doesn't exist
        psycopg.Error: If SQL execution fails
    """
    if not file_path.exists():
        raise FileNotFoundError(f"SQL file not found: {file_path}")

    sql_content = file_path.read_text()
    await connection.execute(sql_content)


async def execute_sql_files(
    connection: psycopg.AsyncConnection, base_path: Path, files: List[str]
) -> None:
    """Execute multiple SQL files in order.

    Args:
        connection: Database connection
        base_path: Base directory path
        files: List of relative file paths to execute

    Raises:
        Exception: If any SQL file execution fails
    """
    for file_path in files:
        full_path = base_path / file_path
        try:
            await load_sql_file(connection, full_path)
        except Exception as e:
            print(f"Error executing {file_path}: {e}")
            raise


# Convenience fixtures for specific test scenarios
@pytest_asyncio.fixture
async def blog_e2e_workflow(blog_schema_setup) -> psycopg.AsyncConnection:
    """Blog database setup for E2E workflow tests."""
    return await blog_with_test_data(blog_schema_setup, "12001_e2e_workflow_test")


@pytest_asyncio.fixture
async def clean_blog_db(blog_schema_setup) -> psycopg.AsyncConnection:
    """Clean blog database with only common seed data."""
    return blog_schema_setup
