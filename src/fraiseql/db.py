"""Database utilities and repository layer for FraiseQL using psycopg and connection pooling."""

import contextlib
import logging
import os
from collections.abc import Awaitable, Callable, Mapping
from dataclasses import dataclass
from datetime import datetime
from decimal import Decimal
from typing import Any, Optional, TypeVar, Union, get_args, get_origin
from uuid import UUID

from psycopg.rows import dict_row
from psycopg.sql import SQL, Composed
from psycopg_pool import AsyncConnectionPool

from fraiseql.audit import get_security_logger
from fraiseql.partial_instantiation import create_partial_instance
from fraiseql.utils.casing import to_snake_case

logger = logging.getLogger(__name__)

T = TypeVar("T")

# Type registry for development mode
_type_registry: dict[str, type] = {}


@dataclass
class DatabaseQuery:
    """Encapsulates a SQL query, parameters, and fetch flag."""

    statement: Composed | SQL
    params: Mapping[str, object]
    fetch_result: bool = True


def register_type_for_view(view_name: str, type_class: type) -> None:
    """Register a type class for a specific view name.

    This is used in development mode to instantiate proper types from view data.

    Args:
        view_name: The database view name
        type_class: The Python type class decorated with @fraise_type
    """
    _type_registry[view_name] = type_class


class FraiseQLRepository:
    """Asynchronous repository for executing SQL queries via a pooled psycopg connection."""

    def __init__(self, pool: AsyncConnectionPool, context: Optional[dict[str, Any]] = None) -> None:
        """Initialize with an async connection pool and optional context."""
        self._pool = pool
        self.context = context or {}
        self.mode = self._determine_mode()
        # Get query timeout from context or use default (30 seconds)
        self.query_timeout = self.context.get("query_timeout", 30)

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
                # Set statement timeout for this query
                if self.query_timeout:
                    # Use literal value, not prepared statement parameters
                    # PostgreSQL doesn't support parameters in SET LOCAL
                    timeout_ms = int(self.query_timeout * 1000)
                    await cursor.execute(
                        f"SET LOCAL statement_timeout = '{timeout_ms}ms'",
                    )

                await cursor.execute(query.statement, query.params)
                if query.fetch_result:
                    return await cursor.fetchall()
                return []
        except Exception as e:
            logger.exception("❌ Database error executing query")

            # Log query timeout specifically
            error_msg = str(e)
            if "statement timeout" in error_msg or "canceling statement" in error_msg:
                security_logger = get_security_logger()
                security_logger.log_query_timeout(
                    user_id=self.context.get("user_id"),
                    execution_time=self.query_timeout,
                    metadata={
                        "error": str(e),
                        "query_type": "database_query",
                    },
                )

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
                # Set statement timeout for this query
                if self.query_timeout:
                    # Use literal value, not prepared statement parameters
                    # PostgreSQL doesn't support parameters in SET LOCAL
                    timeout_ms = int(self.query_timeout * 1000)
                    await cursor.execute(
                        f"SET LOCAL statement_timeout = '{timeout_ms}ms'",
                    )

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

    async def execute_function_with_context(
        self,
        function_name: str,
        context_args: list[object],
        input_data: dict[str, object],
    ) -> dict[str, object]:
        """Execute a PostgreSQL function with context parameters.

        Args:
            function_name: Fully qualified function name (e.g., 'app.create_location')
            context_args: List of context arguments (e.g., [tenant_id, user_id])
            input_data: Dictionary to pass as JSONB to the function

        Returns:
            Dictionary result from the function (mutation_result type)
        """
        import json

        # Validate function name to prevent SQL injection
        if not function_name.replace("_", "").replace(".", "").isalnum():
            msg = f"Invalid function name: {function_name}"
            raise ValueError(msg)

        # Build parameter placeholders
        param_count = len(context_args) + 1  # +1 for the JSONB parameter

        # Check if this is psycopg pool or asyncpg pool
        if hasattr(self._pool, "connection"):
            # psycopg pool
            placeholders = ", ".join(["%s"] * len(context_args)) + ", %s::jsonb"
            params = [*list(context_args), json.dumps(input_data)]

            async with (
                self._pool.connection() as conn,
                conn.cursor(row_factory=dict_row) as cursor,
            ):
                # Set statement timeout for this query
                if self.query_timeout:
                    # Use literal value, not prepared statement parameters
                    # PostgreSQL doesn't support parameters in SET LOCAL
                    timeout_ms = int(self.query_timeout * 1000)
                    await cursor.execute(
                        f"SET LOCAL statement_timeout = '{timeout_ms}ms'",
                    )

                await cursor.execute(
                    f"SELECT * FROM {function_name}({placeholders})",
                    params,
                )
                result = await cursor.fetchone()
                return result if result else {}
        else:
            # asyncpg pool
            placeholders = (
                ", ".join([f"${i + 1}" for i in range(len(context_args))])
                + f", ${param_count}::jsonb"
            )
            params = [*list(context_args), input_data]

            async with self._pool.acquire() as conn:
                # Set up JSON codec for asyncpg
                await conn.set_type_codec(
                    "jsonb",
                    encoder=json.dumps,
                    decoder=json.loads,
                    schema="pg_catalog",
                )

                result = await conn.fetchrow(
                    f"SELECT * FROM {function_name}({placeholders})",
                    *params,
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
            # Production: Extract JSONB data if present, otherwise return raw dicts
            if rows and len(rows) > 0 and "data" in rows[0]:
                # Clean UNSET values from extracted JSONB data before returning
                from fraiseql.fastapi.json_encoder import clean_unset_values

                return [clean_unset_values(row["data"]) for row in rows]
            return rows

        # Development: Full instantiation
        type_class = self._get_type_for_view(view_name)
        return [self._instantiate_from_row(type_class, row) for row in rows]

    async def find_one(self, view_name: str, **kwargs) -> Optional[Union[dict[str, Any], Any]]:
        """Find single record with mode-appropriate return type."""
        # Build and execute query
        query = self._build_find_one_query(view_name, **kwargs)

        # Execute query to get single row
        async with (
            self._pool.connection() as conn,
            conn.cursor(row_factory=dict_row) as cursor,
        ):
            # Set statement timeout for this query
            if self.query_timeout:
                await cursor.execute(
                    "SET LOCAL statement_timeout = %s",
                    (f"{self.query_timeout * 1000}ms",),
                )

            await cursor.execute(query.statement, query.params)
            row = await cursor.fetchone()

        if not row:
            return None

        if self.mode == "production":
            # Production: Extract JSONB data if present, otherwise return raw dict
            if "data" in row:
                # Clean UNSET values from extracted JSONB data before returning
                from fraiseql.fastapi.json_encoder import clean_unset_values

                return clean_unset_values(row["data"])
            return row

        type_class = self._get_type_for_view(view_name)
        return self._instantiate_from_row(type_class, row)

    def _instantiate_from_row(self, type_class: type, row: dict[str, Any]) -> Any:
        """Instantiate a type from the 'data' JSONB column."""
        return self._instantiate_recursive(type_class, row["data"])

    def _instantiate_recursive(
        self,
        type_class: type,
        data: dict[str, Any],
        cache: Optional[dict[str, Any]] = None,
        depth: int = 0,
        partial: bool = True,
    ) -> Any:
        """Recursively instantiate nested objects (dev mode only).

        Args:
            type_class: The type to instantiate
            data: The data dictionary
            cache: Cache for circular reference detection
            depth: Current recursion depth
            partial: Whether to allow partial instantiation (default True in dev mode)
        """
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
                        partial=partial,
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
                        self._instantiate_recursive(
                            item_type,
                            item,
                            cache,
                            depth + 1,
                            partial=partial,
                        )
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
                    with contextlib.suppress(ValueError):
                        processed_value = UUID(processed_value)
                # Check if field is datetime and value is string
                elif actual_field_type == datetime and isinstance(processed_value, str):
                    with contextlib.suppress(ValueError):
                        # Try ISO format first
                        processed_value = datetime.fromisoformat(
                            processed_value.replace("Z", "+00:00"),
                        )
                # Check if field is Decimal and value is numeric
                elif actual_field_type == Decimal and isinstance(
                    processed_value,
                    (int, float, str),
                ):
                    with contextlib.suppress(ValueError, TypeError):
                        processed_value = Decimal(str(processed_value))

            snake_data[snake_key] = processed_value

        # Create instance - use partial instantiation in development mode
        if partial and self.mode == "development":
            # Always use partial instantiation in development mode
            # This allows GraphQL queries to request only needed fields
            instance = create_partial_instance(type_class, snake_data)
        else:
            # Production mode or explicit non-partial - use regular instantiation
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
        # Check the global type registry
        if view_name in _type_registry:
            return _type_registry[view_name]

        # Try to find type by convention (remove _view suffix and check)
        type_name = view_name.replace("_view", "")
        for registered_view, type_class in _type_registry.items():
            if registered_view.lower().replace("_", "") == type_name.lower().replace("_", ""):
                return type_class

        available_views = list(_type_registry.keys())
        raise NotImplementedError(
            f"Type registry lookup for {view_name} not implemented. "
            f"Available views: {available_views}",
        )

    def _build_find_query(self, view_name: str, **kwargs) -> DatabaseQuery:
        """Build a SELECT query for finding multiple records.

        Supports both simple key-value filters and where types with to_sql() methods.
        """
        from psycopg.sql import SQL, Identifier, Literal

        where_parts = []
        params = {}
        param_counter = 0

        # Extract special parameters
        where_obj = kwargs.pop("where", None)
        limit = kwargs.pop("limit", None)
        offset = kwargs.pop("offset", None)
        order_by = kwargs.pop("order_by", None)

        # Process where object - convert GraphQL input to SQL where if needed
        if where_obj:
            # Check if this is a GraphQL where input that needs conversion
            if hasattr(where_obj, "_to_sql_where"):
                where_obj = where_obj._to_sql_where()

            # Process the SQL where type
            if hasattr(where_obj, "to_sql"):
                where_composed = where_obj.to_sql()
                if where_composed:
                    # The where type returns a Composed object with JSONB paths
                    # We need to add it as a SQL fragment
                    where_parts.append(where_composed)

        # Process remaining kwargs as simple equality filters
        for param_counter, (key, value) in enumerate(kwargs.items()):
            param_name = f"param_{param_counter}"
            where_parts.append(f"{key} = %({param_name})s")
            params[param_name] = value

        # Build SQL using proper composition
        query_parts = [SQL("SELECT * FROM ") + Identifier(view_name)]

        if where_parts:
            # Separate SQL/Composed objects from string parts
            where_sql_parts = []
            for part in where_parts:
                if isinstance(part, (SQL, Composed)):
                    where_sql_parts.append(part)
                else:
                    where_sql_parts.append(SQL(part))

            query_parts.append(SQL(" WHERE "))
            for i, part in enumerate(where_sql_parts):
                if i > 0:
                    query_parts.append(SQL(" AND "))
                query_parts.append(part)

        # Handle order_by
        if order_by:
            query_parts.append(SQL(" ORDER BY ") + SQL(order_by))

        # Handle limit and offset
        if limit is not None:
            query_parts.append(SQL(" LIMIT ") + Literal(limit))
            if offset is not None:
                query_parts.append(SQL(" OFFSET ") + Literal(offset))

        statement = SQL("").join(query_parts)
        return DatabaseQuery(statement=statement, params=params, fetch_result=True)

    def _build_find_one_query(self, view_name: str, **kwargs) -> DatabaseQuery:
        """Build a SELECT query for finding a single record."""
        # Force limit=1 for find_one
        kwargs["limit"] = 1
        return self._build_find_query(view_name, **kwargs)
