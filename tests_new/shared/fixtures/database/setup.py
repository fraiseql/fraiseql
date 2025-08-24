"""Database fixture for FraiseQL testing with PostgreSQL integration.

This module provides database setup and teardown for FraiseQL blog demo tests
with real PostgreSQL database connections.
"""

import os
import asyncio
import logging
from typing import AsyncGenerator

import psycopg
import pytest
import pytest_asyncio

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Database configuration
DB_USER = os.getenv("DB_USER", "lionel")
DB_PASSWORD = os.getenv("DB_PASSWORD", "")
DB_HOST = os.getenv("DB_HOST", "localhost")
DB_PORT = int(os.getenv("DB_PORT", "5432"))
DB_NAME_TEMPLATE = "fraiseql_blog_{demo}_test"


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
    if schema_path and os.path.exists(schema_path):
        logger.info(f"Applying schema from {schema_path}")
        
        async with await psycopg.AsyncConnection.connect(get_db_connection_string(db_name)) as conn:
            try:
                with open(schema_path, 'r') as schema_file:
                    schema_sql = schema_file.read()
                
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
            await conn.execute(f"""
                SELECT pg_terminate_backend(pg_stat_activity.pid)
                FROM pg_stat_activity
                WHERE pg_stat_activity.datname = '{db_name}'
                  AND pid <> pg_backend_pid()
            """)
            
            # Drop database
            await conn.execute(f'DROP DATABASE IF EXISTS "{db_name}"')
            
            logger.info(f"✅ Database {db_name} dropped successfully")
    
    except Exception as e:
        logger.warning(f"⚠️ Could not drop database {db_name}: {e}")


@pytest_asyncio.fixture(scope="session")
async def database_simple():
    """Database fixture for blog demo simple tests."""
    db_name = DB_NAME_TEMPLATE.format(demo="simple")
    schema_path = "/home/lionel/code/fraiseql/tests_new/e2e/blog_demo_simple/db/create_full.sql"
    
    await create_test_database(db_name, schema_path)
    
    yield db_name
    
    await drop_test_database(db_name)


@pytest_asyncio.fixture(scope="session") 
async def database_enterprise():
    """Database fixture for blog demo enterprise tests."""
    db_name = DB_NAME_TEMPLATE.format(demo="enterprise")
    # For now, use simple schema - enterprise will extend later
    schema_path = "/home/lionel/code/fraiseql/tests_new/e2e/blog_demo_simple/db/create_full.sql"
    
    await create_test_database(db_name, schema_path)
    
    yield db_name
    
    await drop_test_database(db_name)


@pytest_asyncio.fixture
async def db_connection_simple(database_simple: str) -> AsyncGenerator[psycopg.AsyncConnection, None]:
    """Provide database connection for simple blog demo tests."""
    conn_str = get_db_connection_string(database_simple)
    
    async with await psycopg.AsyncConnection.connect(conn_str) as conn:
        # Start transaction for test isolation
        async with conn.transaction():
            yield conn
            # Transaction will rollback automatically


@pytest_asyncio.fixture
async def db_connection_enterprise(database_enterprise: str) -> AsyncGenerator[psycopg.AsyncConnection, None]:
    """Provide database connection for enterprise blog demo tests."""
    conn_str = get_db_connection_string(database_enterprise)
    
    async with await psycopg.AsyncConnection.connect(conn_str) as conn:
        # Start transaction for test isolation
        async with conn.transaction():
            yield conn
            # Transaction will rollback automatically


class DatabaseManager:
    """Utility class for database management during tests."""
    
    def __init__(self, connection: psycopg.AsyncConnection):
        self.connection = connection
    
    async def execute_query(self, query: str, params: dict = None) -> list[dict]:
        """Execute a query and return results as list of dicts."""
        async with self.connection.cursor() as cursor:
            await cursor.execute(query, params)
            columns = [desc[0] for desc in cursor.description] if cursor.description else []
            rows = await cursor.fetchall()
            return [dict(zip(columns, row)) for row in rows]
    
    async def execute_mutation(self, query: str, params: dict = None) -> dict:
        """Execute a mutation and return the result."""
        result = await self.execute_query(query, params)
        return result[0] if result else {}


@pytest_asyncio.fixture
async def db_manager_simple(db_connection_simple: psycopg.AsyncConnection) -> DatabaseManager:
    """Provide database manager for simple blog demo tests."""
    return DatabaseManager(db_connection_simple)


@pytest_asyncio.fixture  
async def db_manager_enterprise(db_connection_enterprise: psycopg.AsyncConnection) -> DatabaseManager:
    """Provide database manager for enterprise blog demo tests."""
    return DatabaseManager(db_connection_enterprise)


@pytest_asyncio.fixture
async def seeded_blog_database_simple(db_connection_simple: psycopg.AsyncConnection):
    """Provide a simple blog database with seeded test data."""
    from .seeding import BlogDataSeeder
    
    seeder = BlogDataSeeder(db_connection_simple)
    
    # Seed the data
    seeded_data = await seeder.seed_all_data()
    
    yield seeded_data
    
    # Cleanup after test
    await seeder.cleanup_all_data()


@pytest_asyncio.fixture
async def seeded_blog_database_enterprise(db_connection_enterprise: psycopg.AsyncConnection):
    """Provide an enterprise blog database with seeded test data."""
    from .seeding import BlogDataSeeder
    
    seeder = BlogDataSeeder(db_connection_enterprise)
    
    # Seed the data
    seeded_data = await seeder.seed_all_data()
    
    yield seeded_data
    
    # Cleanup after test
    await seeder.cleanup_all_data()