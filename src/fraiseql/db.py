"""Database utilities and repository layer for FraiseQL using psycopg and connection pooling."""

import logging
import os
from collections.abc import Awaitable, Callable, Mapping
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Optional, TypeVar, Union, get_args, get_origin
from uuid import UUID

from psycopg.rows import dict_row
from psycopg.sql import SQL, Composed
from psycopg_pool import AsyncConnectionPool

from fraiseql.utils.casing import to_snake_case

logger = logging.getLogger(__name__)

T = TypeVar("T")


@dataclass
class DatabaseQuery:
    """Encapsulates a SQL query, parameters, and fetch flag."""

    statement: Composed | SQL
    params: Mapping[str, object]
    fetch_result: bool = True


class FraiseQLRepository:
    """Asynchronous repository for executing SQL queries via a pooled psycopg connection."""

    def __init__(self, pool: AsyncConnectionPool, context: Optional[dict[str, Any]] = None) -> None:
        """Initialize with an async connection pool and optional context."""
        self._pool = pool
        self.context = context or {}
        self.mode = self._determine_mode()

    async def run(self, query: DatabaseQuery) -> list[dict[str, object]]:
        """Execute a SQL query using a connection from the pool.

        Args:
            query: SQL statement, parameters, and fetch flag.

        Returns:
            List of rows as dictionaries if `fetch_result` is True, else an empty list.
        """
        try:
            async with (
                self._pool.connection() as conn,
                conn.cursor(row_factory=dict_row) as cursor,
            ):
                await cursor.execute(query.statement, query.params)
                if query.fetch_result:
                    return await cursor.fetchall()
                return []
        except Exception:
            logger.exception("❌ Database error executing query")
            raise

    async def run_in_transaction(
        self,
        func: Callable[..., Awaitable[T]],
        *args: object,
        **kwargs: object,
    ) -> T:
        """Run a user function inside a transaction with a connection from the pool.

        The given `func` must accept the connection as its first argument.
        On exception, the transaction is rolled back.

        Example:
            async def do_stuff(conn):
                await conn.execute("...")
                return ...

            await repo.run_in_transaction(do_stuff)

        Returns:
            Result of the function, if successful.
        """
        async with self._pool.connection() as conn, conn.transaction():
            return await func(conn, *args, **kwargs)

    def get_pool(self) -> AsyncConnectionPool:
        """Expose the underlying connection pool."""
        return self._pool

    async def execute_function(
        self,
        function_name: str,
        input_data: dict[str, object],
    ) -> dict[str, object]:
        """Execute a PostgreSQL function and return the result.

        Args:
            function_name: Fully qualified function name (e.g., 'graphql.create_user')
            input_data: Dictionary to pass as JSONB to the function

        Returns:
            Dictionary result from the function (mutation_result type)
        """
        import json

        # Check if this is psycopg pool or asyncpg pool
        if hasattr(self._pool, "connection"):
            # psycopg pool
            async with (
                self._pool.connection() as conn,
                conn.cursor(row_factory=dict_row) as cursor,
            ):
                # Validate function name to prevent SQL injection
                if not function_name.replace("_", "").replace(".", "").isalnum():
                    msg = f"Invalid function name: {function_name}"
                    raise ValueError(msg)

                await cursor.execute(
                    f"SELECT * FROM {function_name}(%s::jsonb)",
                    (json.dumps(input_data),),
                )
                result = await cursor.fetchone()
                return result if result else {}
        else:
            # asyncpg pool
            async with self._pool.acquire() as conn:
                # Set up JSON codec for asyncpg
                await conn.set_type_codec(
                    "jsonb",
                    encoder=json.dumps,
                    decoder=json.loads,
                    schema="pg_catalog",
                )
                # Validate function name to prevent SQL injection
                if not function_name.replace("_", "").replace(".", "").isalnum():
                    msg = f"Invalid function name: {function_name}"
                    raise ValueError(msg)

                result = await conn.fetchrow(
                    f"SELECT * FROM {function_name}($1::jsonb)",
                    input_data,  # Pass the dict directly, asyncpg will encode it
                )
                return dict(result) if result else {}

    def _determine_mode(self) -> str:
        """Determine if we're in dev or production mode."""
        # Check context first (allows per-request override)
        if "mode" in self.context:
            return self.context["mode"]

        # Then environment
        env = os.getenv("FRAISEQL_ENV", "production")
        return "development" if env == "development" else "production"

    async def find(self, view_name: str, **kwargs) -> list[Union[dict[str, Any], Any]]:
        """Find records with mode-appropriate return type."""
        # Build and execute query
        query = self._build_find_query(view_name, **kwargs)
        rows = await self.run(query)

        if self.mode == "production":
            # Production: Return raw dicts
            return rows

        # Development: Full instantiation
        type_class = self._get_type_for_view(view_name)
        return [self._instantiate_recursive(type_class, row) for row in rows]

    async def find_one(self, view_name: str, **kwargs) -> Optional[Union[dict[str, Any], Any]]:
        """Find single record with mode-appropriate return type."""
        # Build and execute query
        query = self._build_find_one_query(view_name, **kwargs)

        # Execute query to get single row
        async with (
            self._pool.connection() as conn,
            conn.cursor(row_factory=dict_row) as cursor,
        ):
            await cursor.execute(query.statement, query.params)
            row = await cursor.fetchone()

        if not row:
            return None

        if self.mode == "production":
            return row

        type_class = self._get_type_for_view(view_name)
        return self._instantiate_recursive(type_class, row)

    def _instantiate_recursive(
        self,
        type_class: type,
        data: dict[str, Any],
        cache: Optional[dict[str, Any]] = None,
        depth: int = 0,
    ) -> Any:
        """Recursively instantiate nested objects (dev mode only)."""
        if cache is None:
            cache = {}

        # Check cache for circular references
        if isinstance(data, dict) and "id" in data:
            obj_id = data["id"]
            if obj_id in cache:
                return cache[obj_id]

        # Max recursion check
        if depth > 10:
            raise ValueError(f"Max recursion depth exceeded for {type_class.__name__}")

        # Convert camelCase to snake_case
        snake_data = {}
        for key, orig_value in data.items():
            if key == "__typename":
                continue
            snake_key = to_snake_case(key)

            # Start with original value
            processed_value = orig_value

            # Check if this field should be recursively instantiated
            if (
                hasattr(type_class, "__gql_type_hints__")
                and isinstance(processed_value, dict)
                and snake_key in type_class.__gql_type_hints__
            ):
                field_type = type_class.__gql_type_hints__[snake_key]
                # Extract the actual type from Optional, List, etc.
                actual_type = self._extract_type(field_type)
                if actual_type and hasattr(actual_type, "__fraiseql_definition__"):
                    processed_value = self._instantiate_recursive(
                        actual_type,
                        processed_value,
                        cache,
                        depth + 1,
                    )
            elif (
                hasattr(type_class, "__gql_type_hints__")
                and isinstance(processed_value, list)
                and snake_key in type_class.__gql_type_hints__
            ):
                field_type = type_class.__gql_type_hints__[snake_key]
                item_type = self._extract_list_type(field_type)
                if item_type and hasattr(item_type, "__fraiseql_definition__"):
                    processed_value = [
                        self._instantiate_recursive(item_type, item, cache, depth + 1)
                        for item in processed_value
                    ]

            # Handle UUID conversion
            if (
                hasattr(type_class, "__gql_type_hints__")
                and snake_key in type_class.__gql_type_hints__
            ):
                field_type = type_class.__gql_type_hints__[snake_key]
                # Extract actual type from Optional
                actual_field_type = self._extract_type(field_type)
                # Check if field is UUID and value is string
                if actual_field_type == UUID and isinstance(processed_value, str):
                    try:
                        processed_value = UUID(processed_value)
                    except ValueError:  # noqa: SIM105
                        pass  # Keep original value if not valid UUID
                # Check if field is datetime and value is string
                elif actual_field_type == datetime and isinstance(processed_value, str):
                    try:
                        # Try ISO format first
                        processed_value = datetime.fromisoformat(
                            processed_value.replace("Z", "+00:00"),
                        )
                    except ValueError:  # noqa: SIM105
                        pass  # Keep original value if not valid datetime

            snake_data[snake_key] = processed_value

        # Create instance
        instance = type_class(**snake_data)

        # Cache it
        if "id" in data:
            cache[data["id"]] = instance

        return instance

    def _extract_type(self, field_type: type) -> Optional[type]:
        """Extract the actual type from Optional, Union, etc."""
        origin = get_origin(field_type)
        if origin is Union:
            args = get_args(field_type)
            # Filter out None type
            non_none_args = [arg for arg in args if arg is not type(None)]
            if non_none_args:
                return non_none_args[0]
        return field_type if origin is None else None

    def _extract_list_type(self, field_type: type) -> Optional[type]:
        """Extract the item type from List[T]."""
        origin = get_origin(field_type)
        if origin is list:
            args = get_args(field_type)
            if args:
                return args[0]
        # Handle Optional[List[T]]
        if origin is Union:
            args = get_args(field_type)
            for arg in args:
                if arg is not type(None):
                    item_type = self._extract_list_type(arg)
                    if item_type:
                        return item_type
        return None

    def _get_type_for_view(self, view_name: str) -> type:
        """Get the type class for a given view name."""
        # This would normally look up the type from a registry
        # For now, we'll raise NotImplementedError
        # In real implementation, this would be populated from @fraise_type decorators
        raise NotImplementedError(f"Type registry lookup for {view_name} not implemented")

    def _build_find_query(self, view_name: str, **kwargs) -> DatabaseQuery:
        """Build a SELECT query for finding multiple records."""
        # Simple implementation for testing
        # In real implementation, this would use the SQL generator
        if kwargs:
            where_parts = []
            params = {}
            for key, value in kwargs.items():
                where_parts.append(f"{key} = %({key})s")
                params[key] = value
            where_clause = " WHERE " + " AND ".join(where_parts)
            statement = SQL(f"SELECT * FROM {view_name}{where_clause}")
        else:
            statement = SQL(f"SELECT * FROM {view_name}")
            params = {}

        return DatabaseQuery(statement=statement, params=params, fetch_result=True)

    def _build_find_one_query(self, view_name: str, **kwargs) -> DatabaseQuery:
        """Build a SELECT query for finding a single record."""
        # Build base query
        query = self._build_find_query(view_name, **kwargs)
        # Add LIMIT 1
        statement = SQL(str(query.statement) + " LIMIT 1")
        return DatabaseQuery(statement=statement, params=query.params, fetch_result=True)
