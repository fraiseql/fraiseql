"""Database container management utilities for FraiseQL testing.

This module provides enhanced container management utilities including:
- Container lifecycle management with health checks
- Database initialization and seeding utilities
- Connection management and pooling helpers
- Container networking and port management
- Cleanup and resource management
"""

import asyncio
import logging
import os
import time
from contextlib import asynccontextmanager
from typing import Any, Dict, List, Optional

import psycopg
import pytest

try:
    import docker
    from testcontainers.postgres import PostgresContainer

    HAS_DOCKER = True
except ImportError:
    HAS_DOCKER = False
    PostgresContainer = None
    docker = None

logger = logging.getLogger(__name__)


class DatabaseContainer:
    """Enhanced PostgreSQL container for testing."""

    def __init__(
        self,
        image: str = "postgres:16-alpine",
        username: str = "fraiseql_test",
        password: str = "fraiseql_test",
        database: str = "fraiseql_test",
        port: int = 5432,
        **kwargs,
    ):
        """Initialize database container.

        Args:
            image: PostgreSQL Docker image
            username: Database username
            password: Database password
            database: Database name
            port: PostgreSQL port
            **kwargs: Additional container options
        """
        if not HAS_DOCKER:
            raise RuntimeError("Docker dependencies not available")

        self.image = image
        self.username = username
        self.password = password
        self.database = database
        self.port = port
        self.container_options = kwargs

        self._container: Optional[PostgresContainer] = None
        self._started = False

    @property
    def container(self) -> PostgresContainer:
        """Get the container instance.

        Returns:
            PostgresContainer: The container instance

        Raises:
            RuntimeError: If container not started
        """
        if not self._container:
            raise RuntimeError("Container not started")
        return self._container

    @property
    def connection_url(self) -> str:
        """Get PostgreSQL connection URL.

        Returns:
            str: Connection URL
        """
        if not self._started:
            raise RuntimeError("Container not started")

        url = self.container.get_connection_url()
        # Normalize for psycopg3
        return url.replace("postgresql+psycopg://", "postgresql://")

    def start(self, timeout: int = 60) -> "DatabaseContainer":
        """Start the container and wait for PostgreSQL to be ready.

        Args:
            timeout: Timeout in seconds

        Returns:
            Self for method chaining

        Raises:
            TimeoutError: If container doesn't start within timeout
        """
        if self._started:
            logger.warning("Container already started")
            return self

        logger.info(f"Starting PostgreSQL container with image {self.image}")

        self._container = PostgresContainer(
            image=self.image,
            username=self.username,
            password=self.password,
            dbname=self.database,
            port=self.port,
            driver="psycopg",
            **self.container_options,
        )

        self._container.start()

        # Wait for container to be ready
        start_time = time.time()
        while time.time() - start_time < timeout:
            try:
                # Test connection
                with psycopg.connect(self.connection_url) as conn:
                    with conn.cursor() as cur:
                        cur.execute("SELECT 1")
                        result = cur.fetchone()
                        if result and result[0] == 1:
                            logger.info("PostgreSQL container ready")
                            self._started = True
                            return self
            except Exception as e:
                logger.debug(f"Container not ready yet: {e}")
                time.sleep(1)

        raise TimeoutError(f"Container failed to start within {timeout} seconds")

    def stop(self) -> None:
        """Stop and cleanup the container."""
        if not self._started:
            logger.warning("Container not started")
            return

        logger.info("Stopping PostgreSQL container")

        if self._container:
            self._container.stop()
            self._container = None

        self._started = False

    async def initialize_database(
        self,
        schema_files: Optional[List[str]] = None,
        seed_files: Optional[List[str]] = None,
        extensions: Optional[List[str]] = None,
    ) -> None:
        """Initialize database with schema and seed data.

        Args:
            schema_files: List of SQL schema file paths
            seed_files: List of SQL seed data file paths
            extensions: List of PostgreSQL extensions to enable
        """
        if not self._started:
            raise RuntimeError("Container not started")

        logger.info("Initializing database schema and data")

        async with psycopg.AsyncConnection.connect(self.connection_url) as conn:
            # Enable extensions
            if extensions:
                for extension in extensions:
                    await conn.execute(f'CREATE EXTENSION IF NOT EXISTS "{extension}"')
                await conn.commit()
                logger.debug(f"Enabled extensions: {extensions}")

            # Load schema files
            if schema_files:
                for schema_file in schema_files:
                    logger.debug(f"Loading schema file: {schema_file}")
                    await self._execute_sql_file(conn, schema_file)

            # Load seed files
            if seed_files:
                for seed_file in seed_files:
                    logger.debug(f"Loading seed file: {seed_file}")
                    await self._execute_sql_file(conn, seed_file)

    async def reset_database(self, preserve_structure: bool = False) -> None:
        """Reset database to clean state.

        Args:
            preserve_structure: If True, only delete data, preserve schema
        """
        if not self._started:
            raise RuntimeError("Container not started")

        logger.info(f"Resetting database (preserve_structure={preserve_structure})")

        async with psycopg.AsyncConnection.connect(self.connection_url) as conn:
            if preserve_structure:
                # Get all user tables and truncate them
                result = await conn.execute("""
                    SELECT tablename FROM pg_tables
                    WHERE schemaname = 'public'
                    AND tablename NOT LIKE 'pg_%'
                """)

                tables = await result.fetchall()
                if tables:
                    table_names = [table[0] for table in tables]
                    truncate_sql = f"TRUNCATE TABLE {', '.join(table_names)} CASCADE"
                    await conn.execute(truncate_sql)
                    logger.debug(f"Truncated tables: {table_names}")
            else:
                # Drop all user objects
                await conn.execute("DROP SCHEMA public CASCADE")
                await conn.execute("CREATE SCHEMA public")
                await conn.execute("GRANT ALL ON SCHEMA public TO public")
                logger.debug("Dropped and recreated public schema")

            await conn.commit()

    async def get_database_stats(self) -> Dict[str, Any]:
        """Get database statistics for monitoring.

        Returns:
            Dict: Database statistics
        """
        if not self._started:
            raise RuntimeError("Container not started")

        async with psycopg.AsyncConnection.connect(self.connection_url) as conn:
            # Get basic stats
            stats = {}

            # Table count
            result = await conn.execute("""
                SELECT COUNT(*) FROM information_schema.tables
                WHERE table_schema = 'public'
            """)
            row = await result.fetchone()
            stats["table_count"] = row[0] if row else 0

            # View count
            result = await conn.execute("""
                SELECT COUNT(*) FROM information_schema.views
                WHERE table_schema = 'public'
            """)
            row = await result.fetchone()
            stats["view_count"] = row[0] if row else 0

            # Function count
            result = await conn.execute("""
                SELECT COUNT(*) FROM information_schema.routines
                WHERE routine_schema = 'public'
            """)
            row = await result.fetchone()
            stats["function_count"] = row[0] if row else 0

            # Database size
            result = await conn.execute("""
                SELECT pg_size_pretty(pg_database_size(current_database()))
            """)
            row = await result.fetchone()
            stats["database_size"] = row[0] if row else "0 bytes"

            # Connection count
            result = await conn.execute("""
                SELECT COUNT(*) FROM pg_stat_activity
                WHERE datname = current_database()
            """)
            row = await result.fetchone()
            stats["connection_count"] = row[0] if row else 0

            return stats

    async def _execute_sql_file(self, conn: psycopg.AsyncConnection, file_path: str) -> None:
        """Execute SQL file contents.

        Args:
            conn: Database connection
            file_path: Path to SQL file
        """
        if not os.path.exists(file_path):
            raise FileNotFoundError(f"SQL file not found: {file_path}")

        with open(file_path, encoding="utf-8") as f:
            sql_content = f.read()

        # Split on semicolons and execute each statement
        statements = [stmt.strip() for stmt in sql_content.split(";") if stmt.strip()]

        for statement in statements:
            try:
                await conn.execute(statement)
            except Exception as e:
                logger.error(f"Error executing SQL statement in {file_path}: {e}")
                logger.error(f"Statement: {statement}")
                raise

        await conn.commit()

    def health_check(self) -> bool:
        """Check if container is healthy.

        Returns:
            bool: True if container is healthy
        """
        if not self._started or not self._container:
            return False

        try:
            with psycopg.connect(self.connection_url, connect_timeout=5) as conn:
                with conn.cursor() as cur:
                    cur.execute("SELECT 1")
                    result = cur.fetchone()
                    return result is not None and result[0] == 1
        except Exception:
            return False


class ContainerManager:
    """Manager for multiple database containers."""

    def __init__(self):
        """Initialize container manager."""
        self._containers: Dict[str, DatabaseContainer] = {}
        self._cleanup_registered = False

    def create_container(self, name: str, **container_kwargs) -> DatabaseContainer:
        """Create a named container.

        Args:
            name: Container identifier
            **container_kwargs: Container configuration

        Returns:
            DatabaseContainer: Created container
        """
        if name in self._containers:
            logger.warning(f"Container '{name}' already exists")
            return self._containers[name]

        container = DatabaseContainer(**container_kwargs)
        self._containers[name] = container

        # Register cleanup if first container
        if not self._cleanup_registered:
            import atexit

            atexit.register(self.cleanup_all)
            self._cleanup_registered = True

        return container

    def get_container(self, name: str) -> Optional[DatabaseContainer]:
        """Get container by name.

        Args:
            name: Container identifier

        Returns:
            Optional[DatabaseContainer]: Container or None
        """
        return self._containers.get(name)

    def start_container(self, name: str, **start_kwargs) -> DatabaseContainer:
        """Start a named container.

        Args:
            name: Container identifier
            **start_kwargs: Start options

        Returns:
            DatabaseContainer: Started container

        Raises:
            KeyError: If container doesn't exist
        """
        container = self._containers.get(name)
        if not container:
            raise KeyError(f"Container '{name}' not found")

        return container.start(**start_kwargs)

    def stop_container(self, name: str) -> None:
        """Stop a named container.

        Args:
            name: Container identifier
        """
        container = self._containers.get(name)
        if container:
            container.stop()

    def cleanup_all(self) -> None:
        """Stop and cleanup all containers."""
        logger.info("Cleaning up all database containers")

        for name, container in self._containers.items():
            try:
                container.stop()
            except Exception as e:
                logger.error(f"Error stopping container '{name}': {e}")

        self._containers.clear()

    def get_health_status(self) -> Dict[str, bool]:
        """Get health status of all containers.

        Returns:
            Dict: Container name -> health status
        """
        return {name: container.health_check() for name, container in self._containers.items()}


# Global container manager instance
_container_manager = ContainerManager()


def get_container_manager() -> ContainerManager:
    """Get global container manager.

    Returns:
        ContainerManager: Global manager instance
    """
    return _container_manager


@asynccontextmanager
async def temporary_database_container(**container_kwargs):
    """Context manager for temporary database container.

    Args:
        **container_kwargs: Container configuration

    Yields:
        DatabaseContainer: Temporary container
    """
    container = DatabaseContainer(**container_kwargs)

    try:
        container.start()
        yield container
    finally:
        container.stop()


def skip_if_no_docker():
    """Pytest decorator to skip tests if Docker is not available."""
    return pytest.mark.skipif(not HAS_DOCKER, reason="Docker not available")


def check_docker_available() -> bool:
    """Check if Docker is available and running.

    Returns:
        bool: True if Docker is available
    """
    if not HAS_DOCKER:
        return False

    try:
        client = docker.from_env()
        client.ping()
        return True
    except Exception:
        return False


async def wait_for_database_ready(connection_url: str, timeout: int = 30) -> bool:
    """Wait for database to be ready for connections.

    Args:
        connection_url: PostgreSQL connection URL
        timeout: Timeout in seconds

    Returns:
        bool: True if database is ready
    """
    start_time = time.time()

    while time.time() - start_time < timeout:
        try:
            async with psycopg.AsyncConnection.connect(connection_url) as conn:
                async with conn.cursor() as cur:
                    await cur.execute("SELECT 1")
                    result = await cur.fetchone()
                    if result and result[0] == 1:
                        return True
        except Exception:
            await asyncio.sleep(1)

    return False


def get_database_container_logs(container: DatabaseContainer) -> str:
    """Get container logs for debugging.

    Args:
        container: Database container

    Returns:
        str: Container logs
    """
    if not container._container:
        return "Container not started"

    try:
        # Get Docker container
        docker_client = docker.from_env()
        docker_container = docker_client.containers.get(container.container.get_container_host_ip())

        logs = docker_container.logs(tail=100).decode("utf-8")
        return logs
    except Exception as e:
        return f"Error getting logs: {e}"
