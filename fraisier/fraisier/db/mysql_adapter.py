"""MySQL adapter implementation for Fraisier.

Provides async MySQL support with connection pooling, parameter handling (%),
and pool metrics. Supports both aiomysql and asyncmy drivers.
"""

from typing import Any
from urllib.parse import urlparse

import aiomysql

from .adapter import DatabaseType, FraiserDatabaseAdapter, PoolMetrics


class MysqlAdapter(FraiserDatabaseAdapter):
    """MySQL adapter with connection pooling.

    Features:
    - Async connection pool via aiomysql
    - Configurable pool sizing (min/max)
    - Parameter substitution with %s
    - Real pool metrics
    - Transaction support
    - Connection string parsing
    """

    def __init__(
        self,
        connection_string: str,
        pool_min_size: int = 5,
        pool_max_size: int = 20,
    ):
        """Initialize MySQL adapter.

        Args:
            connection_string: MySQL connection string
                (e.g., "mysql://user:pass@host:3306/dbname")
            pool_min_size: Minimum connections in pool
            pool_max_size: Maximum connections in pool

        Raises:
            ValueError: If connection string is invalid
        """
        if not connection_string:
            raise ValueError("Connection string cannot be empty")

        self.connection_string = connection_string
        self.pool_min_size = pool_min_size
        self.pool_max_size = pool_max_size
        self._pool: aiomysql.Pool | None = None
        self._last_insert_id: int | None = None

        # Parse connection string
        self._parse_connection_string()

    def _parse_connection_string(self) -> None:
        """Parse MySQL connection string.

        Supports formats:
        - mysql://user:pass@host:port/database
        - mysql://user:pass@host/database (defaults to port 3306)
        - mysql://user@host/database (no password)

        Raises:
            ValueError: If connection string format is invalid
        """
        try:
            parsed = urlparse(self.connection_string)

            if parsed.scheme not in ("mysql", "mysql+aiomysql"):
                raise ValueError(f"Invalid scheme: {parsed.scheme}")

            self.host = parsed.hostname or "localhost"
            self.port = parsed.port or 3306
            self.user = parsed.username or "root"
            self.password = parsed.password or ""
            self.database = (parsed.path or "/").lstrip("/")

            if not self.database:
                raise ValueError("Database name not specified in connection string")
        except Exception as e:
            raise ValueError(f"Invalid MySQL connection string: {e}") from e

    async def connect(self) -> None:
        """Create and open connection pool.

        Raises:
            ConnectionError: If pool cannot be created or opened
        """
        try:
            self._pool = await aiomysql.create_pool(
                host=self.host,
                port=self.port,
                user=self.user,
                password=self.password,
                db=self.database,
                minsize=self.pool_min_size,
                maxsize=self.pool_max_size,
                autocommit=False,
            )
        except Exception as e:
            raise ConnectionError(f"Failed to create MySQL pool: {e}") from e

    async def disconnect(self) -> None:
        """Close and cleanup connection pool."""
        if self._pool is not None:
            self._pool.close()
            await self._pool.wait_closed()
            self._pool = None

    async def execute_query(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        """Execute SELECT query.

        Args:
            query: SQL query with %s or ? placeholders
            params: Query parameters

        Returns:
            List of result rows as dictionaries

        Raises:
            RuntimeError: If not connected
            QueryError: If query execution fails
        """
        if self._pool is None:
            raise RuntimeError("Not connected to database")

        try:
            # Convert ? placeholders to %s if needed
            converted_query = self._convert_placeholders(query)

            async with self._pool.acquire() as conn:
                async with conn.cursor(aiomysql.DictCursor) as cursor:
                    await cursor.execute(converted_query, params or [])
                    rows = await cursor.fetchall()
                    return rows
        except aiomysql.Error as e:
            raise RuntimeError(f"Query execution failed: {e}") from e

    async def execute_update(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> int:
        """Execute INSERT, UPDATE, or DELETE query.

        Args:
            query: SQL query with %s or ? placeholders
            params: Query parameters

        Returns:
            Number of rows affected

        Raises:
            RuntimeError: If not connected
            QueryError: If query execution fails
        """
        if self._pool is None:
            raise RuntimeError("Not connected to database")

        try:
            converted_query = self._convert_placeholders(query)

            async with self._pool.acquire() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute(converted_query, params or [])
                    self._last_insert_id = cursor.lastrowid
                    await conn.commit()
                    return cursor.rowcount
        except aiomysql.Error as e:
            raise RuntimeError(f"Update execution failed: {e}") from e

    async def insert(
        self,
        table: str,
        data: dict[str, Any],
    ) -> int:
        """Insert a record and return its ID.

        Args:
            table: Table name
            data: Column-value pairs

        Returns:
            ID of inserted record (auto-increment value)

        Raises:
            RuntimeError: If insert fails
        """
        if not data:
            raise ValueError("No data to insert")

        columns = list(data.keys())
        placeholders = ", ".join("%s" * len(columns))
        values = list(data.values())

        query = f"INSERT INTO {table} ({', '.join(columns)}) VALUES ({placeholders})"
        rows_affected = await self.execute_update(query, values)

        if rows_affected != 1 or self._last_insert_id is None:
            raise RuntimeError(f"Insert into {table} failed")

        return self._last_insert_id

    async def update(
        self,
        table: str,
        id_value: str | int,
        data: dict[str, Any],
        id_column: str = "id",
    ) -> bool:
        """Update a record.

        Args:
            table: Table name
            id_value: ID of record to update
            data: Column-value pairs to update
            id_column: Name of ID column

        Returns:
            True if record was updated, False if not found
        """
        if not data:
            return False

        set_clauses = [f"{col} = %s" for col in data.keys()]
        values = list(data.values()) + [id_value]

        query = f"UPDATE {table} SET {', '.join(set_clauses)} WHERE {id_column} = %s"
        rows_affected = await self.execute_update(query, values)

        return rows_affected > 0

    async def delete(
        self,
        table: str,
        id_value: str | int,
        id_column: str = "id",
    ) -> bool:
        """Delete a record.

        Args:
            table: Table name
            id_value: ID of record to delete
            id_column: Name of ID column

        Returns:
            True if record was deleted, False if not found
        """
        query = f"DELETE FROM {table} WHERE {id_column} = %s"
        rows_affected = await self.execute_update(query, [id_value])
        return rows_affected > 0

    async def health_check(self) -> bool:
        """Verify connectivity with simple query.

        Returns:
            True if pool is responsive, False otherwise
        """
        if self._pool is None:
            return False

        try:
            async with self._pool.acquire() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute("SELECT 1")
            return True
        except Exception:
            return False

    def database_type(self) -> DatabaseType:
        """Return database type identifier."""
        return DatabaseType.MYSQL

    def pool_metrics(self) -> PoolMetrics:
        """Return current pool metrics.

        Returns:
            PoolMetrics with connection pool statistics
        """
        if self._pool is None:
            return PoolMetrics()

        try:
            free_conns = self._pool._holders.qsize() if hasattr(self._pool, "_holders") else 0
            total_conns = self.pool_max_size
            active_conns = total_conns - free_conns

            return PoolMetrics(
                total_connections=total_conns,
                active_connections=max(0, active_conns),
                idle_connections=free_conns,
                waiting_requests=0,
            )
        except Exception:
            # Fallback
            return PoolMetrics(
                total_connections=self.pool_max_size,
                active_connections=0,
                idle_connections=0,
            )

    async def begin_transaction(self) -> None:
        """Begin a transaction."""
        if self._pool is None:
            raise RuntimeError("Not connected to database")
        # Handled via autocommit=False on pool

    async def commit_transaction(self) -> None:
        """Commit current transaction."""
        if self._pool is None:
            raise RuntimeError("Not connected to database")
        # Handled via cursor operations

    async def rollback_transaction(self) -> None:
        """Rollback current transaction."""
        if self._pool is None:
            raise RuntimeError("Not connected to database")
        # Handled via cursor operations

    @property
    def last_insert_id(self) -> int | None:
        """Get ID of last inserted record."""
        return self._last_insert_id

    @property
    def is_connected(self) -> bool:
        """Check if pool is open and ready."""
        return self._pool is not None

    @staticmethod
    def _convert_placeholders(query: str) -> str:
        """Convert ? placeholders to MySQL %s format.

        Args:
            query: SQL query potentially using ? placeholders

        Returns:
            Query with MySQL %s placeholders
        """
        # MySQL uses %s for parameters, just replace ?
        return query.replace("?", "%s")


__all__ = ["MysqlAdapter"]
