"""SQLite adapter implementation for Fraisier.

Provides async SQLite support using aiosqlite for compatibility with
development, testing, and light production deployments.

Uses ? placeholders for parameters (SQLite standard).
"""

import sqlite3
from pathlib import Path
from typing import Any

import aiosqlite

from .adapter import DatabaseType, FraiserDatabaseAdapter, PoolMetrics, QueryResult


class SqliteAdapter(FraiserDatabaseAdapter):
    """SQLite database adapter with async support.

    Features:
    - Async connection management via aiosqlite
    - Row factory for dict-based results
    - In-memory or file-based storage
    - Mock pool metrics (SQLite has no real pooling)
    """

    def __init__(self, db_path: str = ":memory:"):
        """Initialize SQLite adapter.

        Args:
            db_path: Path to database file (default: in-memory)
        """
        self.db_path = db_path
        self._conn: aiosqlite.Connection | None = None
        self._last_insert_id: int | None = None

    async def connect(self) -> None:
        """Open async connection to SQLite database.

        Creates the database file if it doesn't exist.
        Configures row factory to return dicts.

        Raises:
            ConnectionError: If database cannot be opened
        """
        try:
            # Ensure parent directory exists for file-based databases
            if self.db_path != ":memory:":
                path = Path(self.db_path)
                path.parent.mkdir(parents=True, exist_ok=True)

            self._conn = await aiosqlite.connect(self.db_path)
            # Configure to return rows as dicts
            self._conn.row_factory = aiosqlite.Row
        except Exception as e:
            raise ConnectionError(f"Failed to connect to SQLite at {self.db_path}: {e}") from e

    async def disconnect(self) -> None:
        """Close connection to database."""
        if self._conn is not None:
            await self._conn.close()
            self._conn = None

    async def execute_query(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        """Execute SELECT query and return results.

        Args:
            query: SQL query with ? placeholders
            params: Query parameters

        Returns:
            List of result rows as dictionaries

        Raises:
            RuntimeError: If not connected
            QueryError: If query execution fails
        """
        if self._conn is None:
            raise RuntimeError("Not connected to database")

        try:
            params_tuple = tuple(params) if params else None
            cursor = await self._conn.execute(query, params_tuple or ())
            rows = await cursor.fetchall()
            await cursor.close()
            # Convert Row objects to dicts
            return [dict(row) for row in rows]
        except sqlite3.Error as e:
            raise RuntimeError(f"Query execution failed: {e}") from e

    async def execute_update(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> int:
        """Execute INSERT, UPDATE, or DELETE query.

        Args:
            query: SQL query with ? placeholders
            params: Query parameters

        Returns:
            Number of rows affected

        Raises:
            RuntimeError: If not connected
            QueryError: If query execution fails
        """
        if self._conn is None:
            raise RuntimeError("Not connected to database")

        try:
            params_tuple = tuple(params) if params else None
            cursor = await self._conn.execute(query, params_tuple or ())
            await self._conn.commit()
            rows_affected = cursor.rowcount
            self._last_insert_id = cursor.lastrowid
            await cursor.close()
            return rows_affected
        except sqlite3.Error as e:
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
            ID of inserted record (SQLite ROWID)

        Raises:
            RuntimeError: If insert fails
        """
        if not data:
            raise ValueError("No data to insert")

        columns = list(data.keys())
        placeholders = ", ".join("?" * len(columns))
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

        set_clause = ", ".join(f"{col} = ?" for col in data.keys())
        values = list(data.values()) + [id_value]

        query = f"UPDATE {table} SET {set_clause} WHERE {id_column} = ?"
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
        query = f"DELETE FROM {table} WHERE {id_column} = ?"
        rows_affected = await self.execute_update(query, [id_value])
        return rows_affected > 0

    async def health_check(self) -> bool:
        """Verify connectivity by executing simple query.

        Returns:
            True if connected and responsive, False otherwise
        """
        if self._conn is None:
            return False

        try:
            await self.execute_query("SELECT 1")
            return True
        except Exception:
            return False

    def database_type(self) -> DatabaseType:
        """Return database type identifier."""
        return DatabaseType.SQLITE

    def pool_metrics(self) -> PoolMetrics:
        """Return pool metrics (mocked for SQLite).

        SQLite doesn't have connection pooling, so we return
        simplified metrics indicating single connection.

        Returns:
            Mock PoolMetrics with single connection
        """
        is_connected = self._conn is not None
        return PoolMetrics(
            total_connections=1 if is_connected else 0,
            active_connections=1 if is_connected else 0,
            idle_connections=0,
            waiting_requests=0,
        )

    async def begin_transaction(self) -> None:
        """Begin a transaction."""
        if self._conn is None:
            raise RuntimeError("Not connected to database")
        # SQLite auto-transactions; explicit BEGIN is optional but safe
        await self._conn.execute("BEGIN")

    async def commit_transaction(self) -> None:
        """Commit current transaction."""
        if self._conn is None:
            raise RuntimeError("Not connected to database")
        await self._conn.commit()

    async def rollback_transaction(self) -> None:
        """Rollback current transaction."""
        if self._conn is None:
            raise RuntimeError("Not connected to database")
        await self._conn.rollback()

    @property
    def last_insert_id(self) -> int | None:
        """Get ID of last inserted record."""
        return self._last_insert_id

    @property
    def is_connected(self) -> bool:
        """Check if connected to database."""
        return self._conn is not None


__all__ = ["SqliteAdapter"]
