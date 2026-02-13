"""Abstract database adapter interface for multi-database support.

Defines the FraiserDatabaseAdapter trait that all database implementations
must follow. Aligns with FraiseQL's trait-based abstraction pattern.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Any


class DatabaseType(Enum):
    """Supported database types."""

    SQLITE = "sqlite"
    POSTGRESQL = "postgresql"
    MYSQL = "mysql"


@dataclass
class PoolMetrics:
    """Unified pool metrics across all database types."""

    total_connections: int = 0
    active_connections: int = 0
    idle_connections: int = 0
    waiting_requests: int = 0


@dataclass
class QueryResult:
    """Typed result wrapper for query results."""

    rows: list[dict[str, Any]]
    row_count: int
    columns: list[str] | None = None
    execution_time_ms: float = 0.0


class FraiserDatabaseAdapter(ABC):
    """Abstract adapter for database operations.

    All database implementations (SQLite, PostgreSQL, MySQL) must implement
    this interface to provide a consistent API across different backends.

    Supports:
    - CRUD operations (insert, update, delete, query)
    - Transaction management
    - Health checks
    - Pool metrics
    - Database-specific parameter handling
    """

    @abstractmethod
    async def connect(self) -> None:
        """Establish connection to database.

        For connection pools, this opens the pool.
        For single connections, this establishes the connection.

        Raises:
            ConnectionError: If connection cannot be established
        """

    @abstractmethod
    async def disconnect(self) -> None:
        """Close connection to database.

        For connection pools, this closes and cleans up the pool.
        """

    @abstractmethod
    async def execute_query(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> list[dict[str, Any]]:
        """Execute a SELECT query.

        Args:
            query: SQL query string (use ? for parameters)
            params: Query parameters

        Returns:
            List of result rows as dictionaries

        Raises:
            QueryError: If query execution fails
        """

    @abstractmethod
    async def execute_update(
        self,
        query: str,
        params: list[Any] | None = None,
    ) -> int:
        """Execute an INSERT, UPDATE, or DELETE query.

        Args:
            query: SQL query string (use ? for parameters)
            params: Query parameters

        Returns:
            Number of rows affected

        Raises:
            QueryError: If query execution fails
        """

    @abstractmethod
    async def insert(
        self,
        table: str,
        data: dict[str, Any],
    ) -> str | int:
        """Insert a record and return its ID.

        Args:
            table: Table name
            data: Column-value pairs to insert

        Returns:
            ID of inserted record (as string or int depending on database)

        Raises:
            QueryError: If insert fails
        """

    @abstractmethod
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
            id_column: Name of ID column (default: "id")

        Returns:
            True if record was updated, False if not found

        Raises:
            QueryError: If update fails
        """

    @abstractmethod
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
            id_column: Name of ID column (default: "id")

        Returns:
            True if record was deleted, False if not found

        Raises:
            QueryError: If delete fails
        """

    @abstractmethod
    async def health_check(self) -> bool:
        """Verify connectivity to database.

        Returns:
            True if database is reachable, False otherwise
        """

    @abstractmethod
    def database_type(self) -> DatabaseType:
        """Return the type of this database.

        Returns:
            DatabaseType enum value
        """

    @abstractmethod
    def pool_metrics(self) -> PoolMetrics:
        """Return current connection pool metrics.

        Returns:
            PoolMetrics with connection statistics
        """

    @abstractmethod
    async def begin_transaction(self) -> None:
        """Begin a transaction.

        Raises:
            TransactionError: If transaction cannot be started
        """

    @abstractmethod
    async def commit_transaction(self) -> None:
        """Commit current transaction.

        Raises:
            TransactionError: If transaction cannot be committed
        """

    @abstractmethod
    async def rollback_transaction(self) -> None:
        """Rollback current transaction.

        Raises:
            TransactionError: If transaction cannot be rolled back
        """

    @property
    def last_insert_id(self) -> int | str | None:
        """Get ID of last inserted record.

        Returns:
            ID of last insert, or None if not applicable
        """
        return None

    @property
    def is_connected(self) -> bool:
        """Check if adapter is connected to database.

        Returns:
            True if connected, False otherwise
        """
        return True


__all__ = [
    "DatabaseType",
    "FraiserDatabaseAdapter",
    "PoolMetrics",
    "QueryResult",
]
