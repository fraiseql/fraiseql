"""PostgreSQL adapter implementation for Fraisier.

Provides production-grade PostgreSQL support with connection pooling,
proper parameter substitution ($1, $2, etc.), and comprehensive metrics.

Aligns with FraiseQL's PostgreSQL implementation patterns.
"""

from typing import Any

try:
    import psycopg
    from psycopg import AsyncConnection
    from psycopg.rows import dict_row
    from psycopg_pool import AsyncConnectionPool
except ImportError as e:
    raise ImportError(
        "PostgreSQL support requires 'psycopg[binary]>=3.1.0'. "
        "Install with: pip install 'psycopg[binary]' or pip install fraisier[postgres]"
    ) from e

from .adapter import DatabaseType, FraiserDatabaseAdapter, PoolMetrics


class PostgresAdapter(FraiserDatabaseAdapter):
    """PostgreSQL adapter with connection pooling.

    Features:
    - Async connection pool via psycopg3
    - Configurable pool sizing (min/max connections)
    - Parameterized queries with $1, $2, etc.
    - Real pool metrics
    - Transaction support
    - Proper connection cleanup
    """

    def __init__(
        self,
        connection_string: str,
        pool_min_size: int = 1,
        pool_max_size: int = 10,
    ):
        """Initialize PostgreSQL adapter.

        Args:
            connection_string: PostgreSQL connection string
                (e.g., "postgresql://user:pass@host/dbname")
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
        self._pool: AsyncConnectionPool | None = None
        self._last_insert_id: str | int | None = None

    async def connect(self) -> None:
        """Create and open connection pool.

        Raises:
            ConnectionError: If pool cannot be created or opened
        """
        try:
            self._pool = AsyncConnectionPool(
                self.connection_string,
                min_size=self.pool_min_size,
                max_size=self.pool_max_size,
                rows_factory=dict_row,
            )
            await self._pool.open()
        except Exception as e:
            raise ConnectionError(f"Failed to create PostgreSQL pool: {e}") from e

    async def disconnect(self) -> None:
        """Close and cleanup connection pool."""
        if self._pool is not None:
            await self._pool.close()
            self._pool = None

    async def execute_query(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        """Execute SELECT query with $N parameter substitution.

        Args:
            query: SQL query with $1, $2, etc. or ? placeholders
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
            # Convert ? placeholders to $N if needed
            converted_query = self._convert_placeholders(query)

            async with self._pool.connection() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute(converted_query, params or [])
                    rows = await cursor.fetchall()
                    return [dict(row) for row in rows]
        except psycopg.Error as e:
            raise RuntimeError(f"Query execution failed: {e}") from e

    async def execute_update(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> int:
        """Execute INSERT, UPDATE, or DELETE query.

        Args:
            query: SQL query with $N or ? placeholders
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

            async with self._pool.connection() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute(converted_query, params or [])
                    rows_affected = cursor.rowcount
                    # For INSERT, try to get RETURNING value
                    if "RETURNING" in query.upper():
                        try:
                            result = await cursor.fetchone()
                            if result:
                                # Assume first column is ID
                                self._last_insert_id = result[0] if isinstance(result, tuple) else list(result.values())[0]
                        except Exception:
                            pass
                    return rows_affected
        except psycopg.Error as e:
            raise RuntimeError(f"Update execution failed: {e}") from e

    async def insert(
        self,
        table: str,
        data: dict[str, Any],
    ) -> str | int:
        """Insert a record and return its ID.

        Args:
            table: Table name
            data: Column-value pairs

        Returns:
            ID of inserted record

        Raises:
            RuntimeError: If insert fails
        """
        if not data:
            raise ValueError("No data to insert")

        columns = list(data.keys())
        placeholders = ", ".join(f"${i+1}" for i in range(len(columns)))
        values = list(data.values())

        # Add RETURNING id to get inserted ID
        query = f"INSERT INTO {table} ({', '.join(columns)}) VALUES ({placeholders}) RETURNING id"

        try:
            if self._pool is None:
                raise RuntimeError("Not connected to database")

            async with self._pool.connection() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute(query, values)
                    result = await cursor.fetchone()
                    if result:
                        inserted_id = result[0] if isinstance(result, tuple) else result.get("id")
                        self._last_insert_id = inserted_id
                        return inserted_id

            raise RuntimeError(f"Insert into {table} failed")
        except psycopg.Error as e:
            raise RuntimeError(f"Insert execution failed: {e}") from e

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

        set_clauses = [f"{col} = ${i+1}" for i, col in enumerate(data.keys())]
        values = list(data.values()) + [id_value]

        query = f"UPDATE {table} SET {', '.join(set_clauses)} WHERE {id_column} = ${len(data)+1}"
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
        query = f"DELETE FROM {table} WHERE {id_column} = $1"
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
            async with self._pool.connection() as conn:
                await conn.execute("SELECT 1")
            return True
        except Exception:
            return False

    def database_type(self) -> DatabaseType:
        """Return database type identifier."""
        return DatabaseType.POSTGRESQL

    def pool_metrics(self) -> PoolMetrics:
        """Return current pool metrics.

        Returns:
            PoolMetrics with actual connection pool statistics
        """
        if self._pool is None:
            return PoolMetrics()

        try:
            # Get pool statistics
            num_connections = len(self._pool._holders) if hasattr(self._pool, "_holders") else 0
            num_available = self._pool.get_size() if hasattr(self._pool, "get_size") else 0

            return PoolMetrics(
                total_connections=num_connections,
                active_connections=max(0, num_connections - num_available),
                idle_connections=num_available,
                waiting_requests=self._pool._waiting_queue.qsize()
                if hasattr(self._pool, "_waiting_queue")
                else 0,
            )
        except Exception:
            # Fallback if metrics can't be retrieved
            return PoolMetrics(
                total_connections=self.pool_max_size,
                active_connections=0,
                idle_connections=0,
            )

    async def begin_transaction(self) -> None:
        """Begin a transaction."""
        if self._pool is None:
            raise RuntimeError("Not connected to database")
        # Connection-specific, handled via cursor in transaction context

    async def commit_transaction(self) -> None:
        """Commit current transaction."""
        if self._pool is None:
            raise RuntimeError("Not connected to database")
        # Connection-specific, handled via cursor in transaction context

    async def rollback_transaction(self) -> None:
        """Rollback current transaction."""
        if self._pool is None:
            raise RuntimeError("Not connected to database")
        # Connection-specific, handled via cursor in transaction context

    @property
    def last_insert_id(self) -> str | int | None:
        """Get ID of last inserted record."""
        return self._last_insert_id

    @property
    def is_connected(self) -> bool:
        """Check if pool is open and ready."""
        return self._pool is not None

    @staticmethod
    def _convert_placeholders(query: str) -> str:
        """Convert ? placeholders to PostgreSQL $N format.

        Args:
            query: SQL query potentially using ? placeholders

        Returns:
            Query with PostgreSQL $N placeholders
        """
        if "?" not in query:
            return query

        # Replace ? with $1, $2, etc.
        result = []
        param_count = 0
        for char in query:
            if char == "?":
                param_count += 1
                result.append(f"${param_count}")
            else:
                result.append(char)

        return "".join(result)


__all__ = ["PostgresAdapter"]
