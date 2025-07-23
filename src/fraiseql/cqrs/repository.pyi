from collections.abc import AsyncGenerator
from typing import Any, Dict, List

class CQRSRepository:
    """Command Query Responsibility Segregation repository for database operations.
    
    Provides high-level methods for querying database views and executing
    PostgreSQL functions with automatic JSON serialization.
    """

    def __init__(self, connection_or_pool: Any) -> None:
        """Initialize repository with database connection or pool.
        
        Args:
            connection_or_pool: Database connection or connection pool
        """

    async def find(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        limit: int | None = None,
        offset: int | None = None,
        order_by: str | List[str] | None = None,
        select_fields: List[str] | None = None,
        distinct: bool = False,
    ) -> List[Dict[str, Any]]:
        """Find multiple records from a database view.
        
        Args:
            view: Name of the database view or table
            filters: WHERE clause filters as key-value pairs
            limit: Maximum number of records to return
            offset: Number of records to skip
            order_by: Field(s) to order by
            select_fields: Specific fields to select
            distinct: Whether to return distinct records only
            
        Returns:
            List of records as dictionaries
        """

    async def find_one(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        select_fields: List[str] | None = None,
    ) -> Dict[str, Any] | None:
        """Find a single record from a database view.
        
        Args:
            view: Name of the database view or table
            filters: WHERE clause filters as key-value pairs
            select_fields: Specific fields to select
            
        Returns:
            Single record as dictionary, or None if not found
        """

    async def count(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
    ) -> int:
        """Count records in a database view.
        
        Args:
            view: Name of the database view or table
            filters: WHERE clause filters as key-value pairs
            
        Returns:
            Number of matching records
        """

    async def execute_function(
        self,
        function_name: str,
        params: Dict[str, Any] | None = None,
        *,
        schema: str | None = None,
    ) -> Dict[str, Any]:
        """Execute a PostgreSQL function.
        
        Args:
            function_name: Name of the function to execute
            params: Function parameters as key-value pairs
            schema: Database schema (defaults to 'graphql')
            
        Returns:
            Function result as dictionary
        """

    async def execute_function_with_context(
        self,
        function_name: str,
        context_args: List[Any],
        params: Dict[str, Any] | None = None,
        *,
        schema: str | None = None,
    ) -> Dict[str, Any]:
        """Execute a PostgreSQL function with context parameters.
        
        Args:
            function_name: Name of the function to execute
            context_args: Context arguments (e.g., tenant_id, user_id)
            params: Function parameters as key-value pairs
            schema: Database schema (defaults to 'graphql')
            
        Returns:
            Function result as dictionary
        """

    async def stream(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
        *,
        batch_size: int = 1000,
        order_by: str | List[str] | None = None,
    ) -> AsyncGenerator[Dict[str, Any]]:
        """Stream records from a database view in batches.
        
        Args:
            view: Name of the database view or table
            filters: WHERE clause filters as key-value pairs
            batch_size: Number of records per batch
            order_by: Field(s) to order by
            
        Yields:
            Individual records as dictionaries
        """

    async def exists(
        self,
        view: str,
        filters: Dict[str, Any] | None = None,
    ) -> bool:
        """Check if any records exist matching the filters.
        
        Args:
            view: Name of the database view or table
            filters: WHERE clause filters as key-value pairs
            
        Returns:
            True if at least one record exists, False otherwise
        """

    async def begin_transaction(self) -> None:
        """Begin a database transaction."""

    async def commit_transaction(self) -> None:
        """Commit the current transaction."""

    async def rollback_transaction(self) -> None:
        """Rollback the current transaction."""

    async def close(self) -> None:
        """Close the database connection."""

class CQRSExecutor:
    """Executor for CQRS operations with caching and optimization."""

    def __init__(
        self,
        repository: CQRSRepository,
        *,
        enable_caching: bool = False,
        cache_ttl: int = 300,
        enable_query_logging: bool = False,
    ) -> None:
        """Initialize CQRS executor.
        
        Args:
            repository: The repository instance to use
            enable_caching: Whether to enable query result caching
            cache_ttl: Cache time-to-live in seconds
            enable_query_logging: Whether to log executed queries
        """

    async def execute_query(
        self,
        query_name: str,
        params: Dict[str, Any] | None = None,
        *,
        cache_key: str | None = None,
    ) -> Any:
        """Execute a named query with optional caching.
        
        Args:
            query_name: Name of the query to execute
            params: Query parameters
            cache_key: Optional cache key for result caching
            
        Returns:
            Query result
        """

    async def execute_mutation(
        self,
        mutation_name: str,
        params: Dict[str, Any] | None = None,
        *,
        context: Dict[str, Any] | None = None,
    ) -> Any:
        """Execute a mutation with context.
        
        Args:
            mutation_name: Name of the mutation to execute
            params: Mutation parameters
            context: Execution context (user, tenant, etc.)
            
        Returns:
            Mutation result
        """

# Pagination helpers
class PaginationResult:
    """Result of a paginated query."""

    items: List[Dict[str, Any]]
    total_count: int
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None
    end_cursor: str | None

async def paginate(
    repository: CQRSRepository,
    view: str,
    *,
    filters: Dict[str, Any] | None = None,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
    order_by: str | List[str] | None = None,
) -> PaginationResult:
    """Paginate results from a repository query.
    
    Args:
        repository: Repository instance
        view: Database view name
        filters: Query filters
        first: Number of items from start
        after: Cursor to start after
        last: Number of items from end
        before: Cursor to end before
        order_by: Fields to order by
        
    Returns:
        Paginated result with metadata
    """

__all__ = [
    "CQRSExecutor",
    "CQRSRepository",
    "PaginationResult",
    "paginate",
]
