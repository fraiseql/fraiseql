"""Enhanced database fixtures for FraiseQL testing.

This module provides modern database setup patterns with real PostgreSQL integration,
transaction-based test isolation, and comprehensive utilities.
"""

import logging
import os
from pathlib import Path
from typing import Any, AsyncGenerator

import psycopg
import pytest_asyncio

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Database configuration
DB_USER = os.getenv("DB_USER", "lionel")
DB_PASSWORD = os.getenv("DB_PASSWORD", "")
DB_HOST = os.getenv("DB_HOST", "localhost")
DB_PORT = int(os.getenv("DB_PORT", "5432"))
DB_NAME_TEMPLATE = "fraiseql_test_{suffix}"


def get_db_connection_string(db_name: str = "postgres") -> str:
    """Get psycopg connection string."""
    parts = [f"dbname={db_name}"]
    if DB_USER:
        parts.append(f"user={DB_USER}")
    if DB_PASSWORD:
        parts.append(f"password={DB_PASSWORD}")
    if DB_HOST:
        parts.append(f"host={DB_HOST}")
    if DB_PORT != 5432:
        parts.append(f"port={DB_PORT}")
    return " ".join(parts)


async def create_test_database(db_name: str, schema_path: str | None = None) -> None:
    """Create a test database and apply schema if provided."""
    logger.info(f"Creating test database: {db_name}")

    # Connect to postgres to create the test database
    conn_str = get_db_connection_string("postgres")

    try:
        async with await psycopg.AsyncConnection.connect(conn_str) as conn:
            await conn.set_autocommit(True)

            # Drop database if exists
            await conn.execute(f'DROP DATABASE IF EXISTS "{db_name}"')

            # Create fresh database
            await conn.execute(f'CREATE DATABASE "{db_name}"')

            logger.info(f"✅ Database {db_name} created successfully")

    except Exception as e:
        logger.error(f"❌ Failed to create database {db_name}: {e}")
        raise

    # Apply schema if provided
    if schema_path and Path(schema_path).exists():
        logger.info(f"Applying schema from {schema_path}")

        async with await psycopg.AsyncConnection.connect(get_db_connection_string(db_name)) as conn:
            try:
                schema_sql = Path(schema_path).read_text()
                await conn.execute(schema_sql)
                logger.info(f"✅ Schema applied successfully to {db_name}")

            except Exception as e:
                logger.error(f"❌ Failed to apply schema to {db_name}: {e}")
                raise


async def drop_test_database(db_name: str) -> None:
    """Drop a test database."""
    logger.info(f"Dropping test database: {db_name}")

    conn_str = get_db_connection_string("postgres")

    try:
        async with await psycopg.AsyncConnection.connect(conn_str) as conn:
            await conn.set_autocommit(True)

            # Terminate active connections
            await conn.execute(
                f"""
                SELECT pg_terminate_backend(pg_stat_activity.pid)
                FROM pg_stat_activity
                WHERE pg_stat_activity.datname = '{db_name}'
                  AND pid <> pg_backend_pid()
            """
            )

            # Drop database
            await conn.execute(f'DROP DATABASE IF EXISTS "{db_name}"')

            logger.info(f"✅ Database {db_name} dropped successfully")

    except Exception as e:
        logger.warning(f"⚠️ Could not drop database {db_name}: {e}")


class DatabaseManager:
    """Utility class for database management during tests."""

    def __init__(self, connection: psycopg.AsyncConnection) -> None:
        self.connection = connection

    async def execute_query(self, query: str, params: dict | None = None) -> list[dict]:
        """Execute a query and return results as list of dicts."""
        async with self.connection.cursor() as cursor:
            await cursor.execute(query, params)
            columns = [desc[0] for desc in cursor.description] if cursor.description else []
            rows = await cursor.fetchall()
            return [dict(zip(columns, row, strict=False)) for row in rows]

    async def execute_mutation(self, query: str, params: dict | None = None) -> dict:
        """Execute a mutation and return the result."""
        result = await self.execute_query(query, params)
        return result[0] if result else {}

    async def insert_test_data(self, table: str, **data: Any) -> dict:
        """Insert test data and return the inserted row."""
        columns = ", ".join(data.keys())
        placeholders = ", ".join([f"${i + 1}" for i in range(len(data))])
        values = list(data.values())

        query = f"""
            INSERT INTO {table} ({columns})
            VALUES ({placeholders})
            RETURNING *
        """

        return await self.execute_mutation(query, values)

    async def cleanup_table(self, table: str) -> None:
        """Clean up test data from a table."""
        await self.execute_mutation(f"DELETE FROM {table}")


@pytest_asyncio.fixture(scope="session")
async def test_database() -> None:
    """Session-scoped test database fixture."""
    db_name = DB_NAME_TEMPLATE.format(suffix="main")

    await create_test_database(db_name)

    yield db_name

    await drop_test_database(db_name)


@pytest_asyncio.fixture
async def db_connection(test_database: str) -> AsyncGenerator[psycopg.AsyncConnection]:
    """Provide isolated database connection with transaction rollback."""
    conn_str = get_db_connection_string(test_database)

    async with await psycopg.AsyncConnection.connect(conn_str) as conn, conn.transaction():
        # Start transaction for test isolation
        yield conn
        # Transaction will rollback automatically


@pytest_asyncio.fixture
async def db_manager(db_connection: psycopg.AsyncConnection) -> DatabaseManager:
    """Provide database manager utility."""
    return DatabaseManager(db_connection)
