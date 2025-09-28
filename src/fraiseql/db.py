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
from fraiseql.core.raw_json_executor import (
    RawJSONResult,
    execute_raw_json_list_query,
    execute_raw_json_query,
)
from fraiseql.partial_instantiation import create_partial_instance
from fraiseql.repositories.intelligent_passthrough import IntelligentPassthroughMixin
from fraiseql.repositories.passthrough_mixin import PassthroughMixin
from fraiseql.utils.casing import to_snake_case

logger = logging.getLogger(__name__)

T = TypeVar("T")

# Type registry for development mode
_type_registry: dict[str, type] = {}

# Table metadata registry - stores column information at registration time
# This avoids expensive runtime introspection
_table_metadata: dict[str, dict[str, Any]] = {}


@dataclass
class DatabaseQuery:
    """Encapsulates a SQL query, parameters, and fetch flag."""

    statement: Composed | SQL
    params: Mapping[str, object]
    fetch_result: bool = True


def register_type_for_view(
    view_name: str,
    type_class: type,
    table_columns: set[str] | None = None,
    has_jsonb_data: bool | None = None,
) -> None:
    """Register a type class for a specific view name with optional metadata.

    This is used in development mode to instantiate proper types from view data.
    Storing metadata at registration time avoids expensive runtime introspection.

    Args:
        view_name: The database view name
        type_class: The Python type class decorated with @fraise_type
        table_columns: Optional set of actual database columns (for hybrid tables)
        has_jsonb_data: Optional flag indicating if table has a JSONB 'data' column
    """
    _type_registry[view_name] = type_class
    logger.debug(f"Registered type {type_class.__name__} for view {view_name}")

    # Store metadata if provided
    if table_columns is not None or has_jsonb_data is not None:
        _table_metadata[view_name] = {
            "columns": table_columns or set(),
            "has_jsonb_data": has_jsonb_data or False,
        }
        logger.debug(
            f"Registered metadata for {view_name}: {len(table_columns or set())} columns, "
            f"jsonb={has_jsonb_data}"
        )


class FraiseQLRepository(IntelligentPassthroughMixin, PassthroughMixin):
    """Asynchronous repository for executing SQL queries via a pooled psycopg connection."""

    def __init__(self, pool: AsyncConnectionPool, context: Optional[dict[str, Any]] = None) -> None:
        """Initialize with an async connection pool and optional context."""
        self._pool = pool
        self.context = context or {}
        self.mode = self._determine_mode()
        # Get query timeout from context or use default (30 seconds)
        self.query_timeout = self.context.get("query_timeout", 30)

    async def _set_session_variables(self, cursor_or_conn) -> None:
        """Set PostgreSQL session variables from context.

        Sets app.tenant_id and app.contact_id session variables if present in context.
        Uses SET LOCAL to scope variables to the current transaction.

        Args:
            cursor_or_conn: Either a psycopg cursor or an asyncpg connection
        """
        from psycopg.sql import SQL, Literal

        # Check if this is a cursor (psycopg) or connection (asyncpg)
        is_cursor = hasattr(cursor_or_conn, "execute") and hasattr(cursor_or_conn, "fetchone")

        if "tenant_id" in self.context:
            if is_cursor:
                await cursor_or_conn.execute(
                    SQL("SET LOCAL app.tenant_id = {}").format(
                        Literal(str(self.context["tenant_id"]))
                    )
                )
            else:
                # asyncpg connection
                await cursor_or_conn.execute(
                    "SET LOCAL app.tenant_id = $1", str(self.context["tenant_id"])
                )

        if "contact_id" in self.context:
            if is_cursor:
                await cursor_or_conn.execute(
                    SQL("SET LOCAL app.contact_id = {}").format(
                        Literal(str(self.context["contact_id"]))
                    )
                )
            else:
                # asyncpg connection
                await cursor_or_conn.execute(
                    "SET LOCAL app.contact_id = $1", str(self.context["contact_id"])
                )
        elif "user" in self.context:
            # Fallback to 'user' if 'contact_id' not set
            if is_cursor:
                await cursor_or_conn.execute(
                    SQL("SET LOCAL app.contact_id = {}").format(Literal(str(self.context["user"])))
                )
            else:
                # asyncpg connection
                await cursor_or_conn.execute(
                    "SET LOCAL app.contact_id = $1", str(self.context["user"])
                )

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

                # Set session variables from context
                await self._set_session_variables(cursor)

                # Handle statement execution based on type and parameter presence
                if isinstance(query.statement, Composed) and not query.params:
                    # Composed objects without params have only embedded literals
                    # This fixes the "%r" placeholder bug from WHERE clause generation
                    await cursor.execute(query.statement)
                elif isinstance(query.statement, (Composed, SQL)) and query.params:
                    # Composed/SQL objects with params - pass parameters normally
                    # This handles legitimate cases like SQL.format() with remaining placeholders
                    await cursor.execute(query.statement, query.params)
                elif isinstance(query.statement, SQL):
                    # SQL objects without params execute directly
                    await cursor.execute(query.statement)
                else:
                    # String statements use parameters normally
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

                # Set session variables from context
                await self._set_session_variables(cursor)

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
            if context_args:
                placeholders = ", ".join(["%s"] * len(context_args)) + ", %s::jsonb"
            else:
                placeholders = "%s::jsonb"
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

                # Set session variables from context
                await self._set_session_variables(cursor)

                await cursor.execute(
                    f"SELECT * FROM {function_name}({placeholders})",
                    tuple(params),
                )
                result = await cursor.fetchone()
                return result if result else {}
        else:
            # asyncpg pool
            if context_args:
                placeholders = (
                    ", ".join([f"${i + 1}" for i in range(len(context_args))])
                    + f", ${param_count}::jsonb"
                )
            else:
                placeholders = "$1::jsonb"
            params = [*list(context_args), input_data]

            async with self._pool.acquire() as conn:
                # Set up JSON codec for asyncpg
                await conn.set_type_codec(
                    "jsonb",
                    encoder=json.dumps,
                    decoder=json.loads,
                    schema="pg_catalog",
                )

                # Set session variables from context
                await self._set_session_variables(conn)

                result = await conn.fetchrow(
                    f"SELECT * FROM {function_name}({placeholders})",
                    *params,
                )
                return dict(result) if result else {}

    def _determine_mode(self) -> str:
        """Determine if we're in dev or production mode."""
        # Check if JSON passthrough is explicitly enabled
        if self.context.get("json_passthrough"):
            return "production"  # Use production mode (no instantiation)

        # Check context first (allows per-request override)
        if "mode" in self.context:
            return self.context["mode"]

        # Then environment
        env = os.getenv("FRAISEQL_ENV", "production")
        return "development" if env == "development" else "production"

    async def _ensure_table_columns_cached(self, view_name: str) -> None:
        """Ensure table columns are cached for hybrid table detection.

        PERFORMANCE OPTIMIZATION:
        - Only introspect once per table per repository instance
        - Cache both successes and failures to avoid repeated queries
        - Use connection pool efficiently
        """
        if not hasattr(self, "_introspected_columns"):
            self._introspected_columns = {}
            self._introspection_in_progress = set()

        # Skip if already cached or being introspected (avoid race conditions)
        if view_name in self._introspected_columns or view_name in self._introspection_in_progress:
            return

        # Mark as in progress to prevent concurrent introspections
        self._introspection_in_progress.add(view_name)

        try:
            await self._introspect_table_columns(view_name)
        except Exception:
            # Cache failure to avoid repeated attempts
            self._introspected_columns[view_name] = set()
        finally:
            self._introspection_in_progress.discard(view_name)

    async def find(self, view_name: str, **kwargs) -> list[dict[str, Any]]:
        """Find records and return as list of dicts.

        In production mode, uses raw JSON internally for field mapping
        but returns parsed dicts for GraphQL compatibility.
        """
        # Pre-fetch table columns for hybrid table detection if there's a where clause
        if "where" in kwargs:
            await self._ensure_table_columns_cached(view_name)

        # Log current mode and context
        logger.info(
            f"Repository find(): mode={self.mode}, context_mode={self.context.get('mode')}, "
            f"json_passthrough={self.context.get('json_passthrough')}"
        )

        # Production mode: Use raw JSON internally but return dicts
        if self.mode == "production":
            # Get GraphQL info from context if available
            info = self.context.get("graphql_info")

            # Extract field paths if we have GraphQL info
            field_paths = None
            if info:
                from fraiseql.core.ast_parser import extract_field_paths_from_info
                from fraiseql.utils.casing import to_snake_case

                field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

            # Check if JSONB extraction is enabled and we don't have field paths
            config = self.context.get("config")
            jsonb_extraction_enabled = (
                config.jsonb_extraction_enabled
                if config and hasattr(config, "jsonb_extraction_enabled")
                else False
            )

            jsonb_column = None
            if jsonb_extraction_enabled and not field_paths:
                # First, get sample rows to determine JSONB column
                sample_query = self._build_find_query(view_name, limit=1, **kwargs)

                async with (
                    self._pool.connection() as conn,
                    conn.cursor(row_factory=dict_row) as cursor,
                ):
                    await cursor.execute(sample_query.statement, sample_query.params)
                    sample_rows = await cursor.fetchall()

                if sample_rows:
                    # Determine which JSONB column to use
                    jsonb_column = self._determine_jsonb_column(view_name, sample_rows)

                    # If no JSONB column found, we need to return full rows
                    if jsonb_column:
                        # Build optimized query with JSONB column
                        query = self._build_find_query(
                            view_name,
                            raw_json=True,
                            field_paths=field_paths,
                            info=info,
                            jsonb_column=jsonb_column,
                            **kwargs,
                        )
                    else:
                        # No JSONB column found, return full rows
                        query = self._build_find_query(view_name, **kwargs)
                        rows = await self.run(query)
                        return rows
                else:
                    # No rows found, just return empty list
                    return []
            elif field_paths:
                # Build optimized query with field mapping
                query = self._build_find_query(
                    view_name, raw_json=True, field_paths=field_paths, info=info, **kwargs
                )
            else:
                # JSONB extraction disabled, return full rows
                query = self._build_find_query(view_name, **kwargs)
                rows = await self.run(query)
                return rows

            # Execute and parse JSON results
            import json

            async with self._pool.connection() as conn:
                result = await execute_raw_json_list_query(
                    conn, query.statement, query.params, None
                )
                # Parse the raw JSON to get list of dicts
                data = json.loads(result.json_string)
                # Extract the data array (it's wrapped in {"data": [...]})
                return data.get("data", [])

        # Development: Full instantiation
        query = self._build_find_query(view_name, **kwargs)
        rows = await self.run(query)
        type_class = self._get_type_for_view(view_name)
        return [self._instantiate_from_row(type_class, row) for row in rows]

    async def find_one(self, view_name: str, **kwargs) -> Optional[dict[str, Any]]:
        """Find single record and return as dict.

        In production mode, uses raw JSON internally for field mapping
        but returns parsed dict for GraphQL compatibility.
        """
        # Log current mode and context
        logger.info(
            f"Repository find_one(): mode={self.mode}, context_mode={self.context.get('mode')}, "
            f"json_passthrough={self.context.get('json_passthrough')}"
        )

        # Production mode: Use raw JSON internally but return dict
        if self.mode == "production":
            # Get GraphQL info from context if available
            info = self.context.get("graphql_info")

            # Extract field paths if we have GraphQL info
            field_paths = None
            if info:
                from fraiseql.core.ast_parser import extract_field_paths_from_info
                from fraiseql.utils.casing import to_snake_case

                field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

            # Check if JSONB extraction is enabled and we don't have field paths
            config = self.context.get("config")
            jsonb_extraction_enabled = (
                config.jsonb_extraction_enabled
                if config and hasattr(config, "jsonb_extraction_enabled")
                else False
            )

            jsonb_column = None
            if jsonb_extraction_enabled and not field_paths:
                # First, get sample row to determine JSONB column
                sample_query = self._build_find_one_query(view_name, **kwargs)

                async with (
                    self._pool.connection() as conn,
                    conn.cursor(row_factory=dict_row) as cursor,
                ):
                    if (
                        isinstance(sample_query.statement, (Composed, SQL))
                        and not sample_query.params
                    ):
                        await cursor.execute(sample_query.statement)
                    else:
                        await cursor.execute(sample_query.statement, sample_query.params)
                    sample_row = await cursor.fetchone()

                if sample_row:
                    # Determine which JSONB column to use
                    jsonb_column = self._determine_jsonb_column(view_name, [sample_row])

                    # If no JSONB column found, we need to return full row
                    if jsonb_column:
                        # Build optimized query with JSONB column
                        query = self._build_find_one_query(
                            view_name,
                            raw_json=True,
                            field_paths=field_paths,
                            info=info,
                            jsonb_column=jsonb_column,
                            **kwargs,
                        )
                    else:
                        # No JSONB column found, return full row
                        return sample_row
                else:
                    # No row found
                    return None
            elif field_paths:
                # Build optimized query with field mapping
                query = self._build_find_one_query(
                    view_name, raw_json=True, field_paths=field_paths, info=info, **kwargs
                )
            else:
                # JSONB extraction disabled, return full row
                query = self._build_find_one_query(view_name, **kwargs)

                # Execute query to get single row
                async with (
                    self._pool.connection() as conn,
                    conn.cursor(row_factory=dict_row) as cursor,
                ):
                    # Set statement timeout for this query
                    if self.query_timeout:
                        timeout_ms = int(self.query_timeout * 1000)
                        await cursor.execute(
                            f"SET LOCAL statement_timeout = '{timeout_ms}ms'",
                        )

                    # Set session variables from context
                    await self._set_session_variables(cursor)

                    # If we have a Composed statement with embedded Literals, execute without params
                    if isinstance(query.statement, (Composed, SQL)) and not query.params:
                        await cursor.execute(query.statement)
                    else:
                        await cursor.execute(query.statement, query.params)
                    row = await cursor.fetchone()

                return row

            # Execute and parse JSON result
            import json

            async with self._pool.connection() as conn:
                result = await execute_raw_json_query(conn, query.statement, query.params, None)
                # Parse the raw JSON to get dict
                data = json.loads(result.json_string)
                # Extract the data object (it's wrapped in {"data": {...}})
                return data.get("data")

        # Development: Full instantiation
        query = self._build_find_one_query(view_name, **kwargs)

        # Execute query to get single row
        async with (
            self._pool.connection() as conn,
            conn.cursor(row_factory=dict_row) as cursor,
        ):
            # Set statement timeout for this query
            if self.query_timeout:
                timeout_ms = int(self.query_timeout * 1000)
                await cursor.execute(
                    f"SET LOCAL statement_timeout = '{timeout_ms}ms'",
                )

            # Set session variables from context
            await self._set_session_variables(cursor)

            # If we have a Composed statement with embedded Literals, execute without params
            if isinstance(query.statement, (Composed, SQL)) and not query.params:
                await cursor.execute(query.statement)
            else:
                await cursor.execute(query.statement, query.params)
            row = await cursor.fetchone()

        if not row:
            return None

        type_class = self._get_type_for_view(view_name)
        return self._instantiate_from_row(type_class, row)

    async def find_raw_json(
        self, view_name: str, field_name: str, info: Any = None, **kwargs
    ) -> RawJSONResult:
        """Find records and return as raw JSON for direct passthrough.

        This method executes a query and returns the result as a raw JSON string,
        bypassing all Python object creation and dict parsing. Use this only for
        special passthrough scenarios. For normal resolvers, use find() instead.

        Args:
            view_name: The database view name
            field_name: The GraphQL field name for response wrapping
            info: Optional GraphQL resolve info for field selection
            **kwargs: Query parameters (where, limit, offset, etc.)

        Returns:
            RawJSONResult containing the raw JSON response
        """
        # Extract field paths from GraphQL info if available
        field_paths = None
        if info:
            from fraiseql.core.ast_parser import extract_field_paths_from_info
            from fraiseql.utils.casing import to_snake_case

            field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

        # Build query with raw JSON output and field paths
        query = self._build_find_query(
            view_name, raw_json=True, field_paths=field_paths, info=info, **kwargs
        )

        # Execute and return raw JSON
        async with self._pool.connection() as conn:
            return await execute_raw_json_list_query(
                conn, query.statement, query.params, field_name
            )

    async def find_one_raw_json(
        self, view_name: str, field_name: str, info: Any = None, **kwargs
    ) -> RawJSONResult:
        """Find a single record and return as raw JSON for direct passthrough.

        This method returns RawJSONResult which cannot be used in normal resolvers.
        Use this only for special passthrough scenarios. For normal resolvers,
        use find_one() instead.

        Args:
            view_name: The database view name
            field_name: The GraphQL field name for response wrapping
            info: Optional GraphQL resolve info for field selection
            **kwargs: Query parameters (id, where, etc.)

        Returns:
            RawJSONResult containing the raw JSON response
        """
        # Extract field paths from GraphQL info if available
        field_paths = None
        if info:
            from fraiseql.core.ast_parser import extract_field_paths_from_info
            from fraiseql.utils.casing import to_snake_case

            field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)

        # Build query with raw JSON output and field paths
        query = self._build_find_one_query(
            view_name, raw_json=True, field_paths=field_paths, info=info, **kwargs
        )

        # Execute and return raw JSON
        async with self._pool.connection() as conn:
            return await execute_raw_json_query(conn, query.statement, query.params, field_name)

    def _instantiate_from_row(self, type_class: type, row: dict[str, Any]) -> Any:
        """Instantiate a type from the row data.

        Handles three scenarios:
        1. Regular tables with only columns (no JSONB)
        2. Pure JSONB tables (all data in JSONB column)
        3. Hybrid tables (both regular columns AND JSONB data)
        """
        # Check if this is a hybrid table (has both regular columns and JSONB data)
        has_data_column = "data" in row and isinstance(row.get("data"), dict)

        # Check if this type uses JSONB data column or regular columns
        if hasattr(type_class, "__fraiseql_definition__"):
            jsonb_column = type_class.__fraiseql_definition__.jsonb_column

            if jsonb_column is None and not has_data_column:
                # Regular table columns only - instantiate from the full row
                return self._instantiate_recursive(type_class, row)
            if jsonb_column is None and has_data_column:
                # Hybrid table: merge regular columns with JSONB data
                # Start with regular columns
                merged_data = {k: v for k, v in row.items() if k != "data"}
                # Override/add fields from JSONB data
                if row["data"]:
                    merged_data.update(row["data"])
                return self._instantiate_recursive(type_class, merged_data)
            # JSONB data column specified - instantiate from the jsonb_column
            column_to_use = jsonb_column or "data"
            if column_to_use not in row:
                raise KeyError(column_to_use)
            return self._instantiate_recursive(type_class, row[column_to_use])

        # No definition - try to detect the structure
        if has_data_column:
            # If we have a data column, it's likely a hybrid or JSONB table
            # For hybrid tables, merge the data
            merged_data = {k: v for k, v in row.items() if k != "data"}
            if row["data"]:
                merged_data.update(row["data"])
            return self._instantiate_recursive(type_class, merged_data)
        # Regular table with no JSONB
        return self._instantiate_recursive(type_class, row)

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

    def _derive_entity_type(self, view_name: str, typename: str | None = None) -> str | None:
        """Derive entity type for CamelForge from view name or GraphQL typename."""
        # Only derive entity type if CamelForge is enabled
        if not self.context.get("camelforge_enabled", False):
            return None

        # First try to use GraphQL typename
        if typename:
            # Convert PascalCase to snake_case (e.g., DnsServer -> dns_server)
            from fraiseql.utils.casing import to_snake_case

            return to_snake_case(typename)

        # Fallback to view name (remove v_, tv_, mv_ prefixes)
        if view_name:
            for prefix in ["v_", "tv_", "mv_"]:
                if view_name.startswith(prefix):
                    return view_name[len(prefix) :]
            return view_name

        return None

    def _determine_jsonb_column(self, view_name: str, rows: list[dict[str, Any]]) -> str | None:
        """Determine which JSONB column to extract data from.

        Args:
            view_name: Name of the database view
            rows: Sample rows to inspect for JSONB columns

        Returns:
            Name of the JSONB column to extract, or None if no suitable column found
        """
        # Check if JSONB extraction is enabled
        config = self.context.get("config")
        if (
            config
            and hasattr(config, "jsonb_extraction_enabled")
            and not config.jsonb_extraction_enabled
        ):
            logger.debug(f"JSONB extraction disabled by config for view '{view_name}'")
            return None
        # Strategy 1: Check if a type is registered for this view and has explicit JSONB column
        if view_name in _type_registry:
            type_class = _type_registry[view_name]
            if hasattr(type_class, "__fraiseql_definition__"):
                definition = type_class.__fraiseql_definition__
                if definition.jsonb_column:
                    # Verify the column exists in the data
                    if rows and definition.jsonb_column in rows[0]:
                        logger.debug(
                            f"Using explicit JSONB column '{definition.jsonb_column}' "
                            f"for view '{view_name}'"
                        )
                        return definition.jsonb_column
                    logger.warning(
                        f"Explicit JSONB column '{definition.jsonb_column}' not found "
                        f"in data for view '{view_name}'. Available columns: "
                        f"{list(rows[0].keys()) if rows else 'None'}"
                    )

        # Strategy 2: Default column names to try
        # Get default columns from config if available, otherwise use hardcoded defaults
        config = self.context.get("config")
        if config and hasattr(config, "jsonb_default_columns"):
            default_columns = config.jsonb_default_columns
        else:
            default_columns = ["data", "json_data", "jsonb_data"]

        if rows:
            for col_name in default_columns:
                if col_name in rows[0]:
                    # Verify it contains dict-like data (not just a primitive)
                    value = rows[0][col_name]
                    if isinstance(value, dict) and value:
                        logger.debug(
                            f"Using default JSONB column '{col_name}' for view '{view_name}'"
                        )
                        return col_name

        # Strategy 3: Auto-detect JSONB columns by content (if enabled)
        config = self.context.get("config")
        auto_detect_enabled = True
        if config and hasattr(config, "jsonb_auto_detect"):
            auto_detect_enabled = config.jsonb_auto_detect

        if auto_detect_enabled and rows:
            for key, value in rows[0].items():
                # Look for columns with dict content that might be JSONB
                if (
                    isinstance(value, dict)
                    and value
                    and key not in ["metadata", "context", "config"]  # Skip common metadata columns
                    and not key.endswith("_id")
                ):  # Skip foreign key columns
                    logger.debug(f"Auto-detected JSONB column '{key}' for view '{view_name}'")
                    return key

        logger.debug(f"No JSONB column found for view '{view_name}', returning raw rows")
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
        logger.error(f"Type registry state: {_type_registry}")
        raise NotImplementedError(
            f"Type registry lookup for {view_name} not implemented. "
            f"Available views: {available_views}. Registry size: {len(_type_registry)}",
        )

    def _build_find_query(
        self,
        view_name: str,
        raw_json: bool = False,
        field_paths: list[Any] | None = None,
        info: Any = None,
        jsonb_column: str | None = None,
        **kwargs,
    ) -> DatabaseQuery:
        """Build a SELECT query for finding multiple records.

        Supports both simple key-value filters and where types with to_sql() methods.

        Args:
            view_name: Name of the view to query
            raw_json: Whether to return raw JSON text for passthrough
            field_paths: Optional list of FieldPath objects for field selection
            info: Optional GraphQL resolve info
            jsonb_column: Optional JSONB column name to use
            **kwargs: Query parameters
        """
        from psycopg.sql import SQL, Composed, Identifier, Literal

        where_parts = []

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
                # HYBRID TABLE FIX (v0.9.5): Handle nested object filters in hybrid tables
                # When a table has both SQL columns (e.g., machine_id) and JSONB data
                # (e.g., data->'machine'->>'id'), nested object filters like
                # {machine: {id: {eq: value}}} should use the SQL column for performance.
                #
                # Without this fix, FraiseQL generates JSONB paths which:
                # 1. Fail with type mismatches (text = uuid)
                # 2. Are slower than direct column access
                # 3. Return incorrect results
                if view_name and hasattr(self, "_introspected_columns"):
                    table_columns = self._introspected_columns.get(view_name)
                    if table_columns:
                        # Convert WHERE object to dict to detect nested object filters
                        where_dict = self._where_obj_to_dict(where_obj, table_columns)
                        if where_dict:
                            # Use dict-based processing which handles hybrid tables correctly
                            where_composed = self._convert_dict_where_to_sql(
                                where_dict, view_name, table_columns
                            )
                            if where_composed:
                                where_parts.append(where_composed)
                        else:
                            # Fallback to standard processing if conversion fails
                            where_composed = where_obj.to_sql()
                            if where_composed:
                                where_parts.append(where_composed)
                    else:
                        # No table columns info, use standard processing
                        where_composed = where_obj.to_sql()
                        if where_composed:
                            where_parts.append(where_composed)
                else:
                    # No view name or introspection, use standard processing
                    where_composed = where_obj.to_sql()
                    if where_composed:
                        where_parts.append(where_composed)
            # Handle plain dictionary where clauses (used in dynamic filter construction)
            # These use regular column names, not JSONB paths
            elif isinstance(where_obj, dict):
                # Try to get actual table columns for accurate field detection
                # This is synchronous context, so we'll rely on cached info if available
                table_columns = None
                if (
                    hasattr(self, "_introspected_columns")
                    and view_name in self._introspected_columns
                ):
                    table_columns = self._introspected_columns[view_name]

                # Convert dictionary where clause to SQL conditions
                where_composed = self._convert_dict_where_to_sql(
                    where_obj, view_name, table_columns
                )
                if where_composed:
                    where_parts.append(where_composed)

        # Process remaining kwargs as simple equality filters
        # Use Composed SQL with Literal values to avoid parameter mixing with WHERE clauses
        for key, value in kwargs.items():
            # Use SQL composition with Literal instead of parameter placeholders
            # This prevents mixing parameter styles when WHERE clauses use Composed objects
            where_condition = Composed([Identifier(key), SQL(" = "), Literal(value)])
            where_parts.append(where_condition)

        # Build SQL using proper composition
        if raw_json and field_paths is not None and len(field_paths) > 0:
            # Use SQL generator for proper field mapping with camelCase aliases
            from fraiseql.sql.sql_generator import build_sql_query

            # Get typename from registry if available
            typename = None
            if view_name in _type_registry:
                type_class = _type_registry[view_name]
                if hasattr(type_class, "__gql_type_name__"):
                    typename = type_class.__gql_type_name__
                elif hasattr(type_class, "__name__"):
                    typename = type_class.__name__

            # Build WHERE clause from parts
            where_composed = None
            if where_parts:
                # Combine SQL/Composed objects
                where_sql_parts = []
                for part in where_parts:
                    if isinstance(part, (SQL, Composed)):
                        where_sql_parts.append(part)
                    else:
                        where_sql_parts.append(SQL(part))
                if where_sql_parts:
                    where_composed = SQL(" AND ").join(where_sql_parts)

            # Process order_by for SQL generator compatibility
            order_by_tuples = None
            if order_by:
                # Check if this is a GraphQL order by input that needs conversion
                if hasattr(order_by, "_to_sql_order_by"):
                    order_by_set = order_by._to_sql_order_by()
                    if order_by_set:
                        # Convert OrderBySet to list of tuples for build_sql_query
                        order_by_tuples = [
                            (instr.field, instr.direction) for instr in order_by_set.instructions
                        ]
                # Check if it's already an OrderBySet
                elif hasattr(order_by, "instructions"):
                    # Convert OrderBySet to list of tuples for build_sql_query
                    order_by_tuples = [
                        (instr.field, instr.direction) for instr in order_by.instructions
                    ]
                # Check if it's a dict representing GraphQL OrderBy input
                elif isinstance(order_by, dict):
                    # Convert dict to SQL ORDER BY
                    from fraiseql.sql.graphql_order_by_generator import (
                        _convert_order_by_input_to_sql,
                    )

                    order_by_set = _convert_order_by_input_to_sql(order_by)
                    if order_by_set:
                        order_by_tuples = [
                            (instr.field, instr.direction) for instr in order_by_set.instructions
                        ]
                # Check if it's a list representing GraphQL OrderBy input
                elif isinstance(order_by, list):
                    # Check if it's already a list of tuples
                    if order_by and isinstance(order_by[0], tuple) and len(order_by[0]) == 2:
                        # Already in the correct format
                        order_by_tuples = order_by
                    else:
                        # Convert list to SQL ORDER BY
                        from fraiseql.sql.graphql_order_by_generator import (
                            _convert_order_by_input_to_sql,
                        )

                        order_by_set = _convert_order_by_input_to_sql(order_by)
                        if order_by_set:
                            order_by_tuples = [
                                (instr.field, instr.direction)
                                for instr in order_by_set.instructions
                            ]

            # Use SQL generator with field paths
            statement = build_sql_query(
                table=view_name,
                field_paths=field_paths,
                where_clause=where_composed,
                json_output=True,
                typename=typename,
                raw_json_output=True,
                auto_camel_case=True,
                order_by=order_by_tuples,
                field_limit_threshold=self.context.get("camelforge_field_threshold")
                or self.context.get("jsonb_field_limit_threshold"),
                camelforge_enabled=self.context.get("camelforge_enabled", False),
                camelforge_function=self.context.get("camelforge_function", "turbo.fn_camelforge"),
                entity_type=self._derive_entity_type(view_name, typename),
            )

            # Handle limit and offset
            if limit is not None:
                statement = statement + SQL(" LIMIT ") + Literal(limit)
                if offset is not None:
                    statement = statement + SQL(" OFFSET ") + Literal(offset)

            return DatabaseQuery(statement=statement, params={}, fetch_result=True)
        if raw_json:
            # For raw JSON without field paths, select the JSONB column as JSON text
            if jsonb_column:
                # Use the determined JSONB column
                query_parts = [
                    SQL("SELECT ")
                    + Identifier(jsonb_column)
                    + SQL("::text FROM ")
                    + Identifier(view_name)
                ]
            else:
                # Check if the type explicitly has no JSONB column
                type_class = None
                if view_name in _type_registry:
                    type_class = _type_registry[view_name]

                if (
                    type_class
                    and hasattr(type_class, "__fraiseql_definition__")
                    and type_class.__fraiseql_definition__.jsonb_column is None
                ):
                    # Type explicitly uses regular columns, not JSONB - fall back to SELECT *
                    query_parts = [SQL("SELECT * FROM ") + Identifier(view_name)]
                else:
                    # Default to 'data' column for backward compatibility
                    query_parts = [SQL("SELECT data::text FROM ") + Identifier(view_name)]
        else:
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
            # Check if this is a GraphQL order by input that needs conversion
            if hasattr(order_by, "_to_sql_order_by"):
                order_by_set = order_by._to_sql_order_by()
                if order_by_set:
                    query_parts.append(SQL(" ") + order_by_set.to_sql())
            # Check if it's already an OrderBySet
            elif hasattr(order_by, "to_sql"):
                query_parts.append(SQL(" ") + order_by.to_sql())
            # Check if it's a dict representing GraphQL OrderBy input
            elif isinstance(order_by, dict):
                # Convert dict to SQL ORDER BY
                from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

                order_by_set = _convert_order_by_input_to_sql(order_by)
                if order_by_set:
                    query_parts.append(SQL(" ") + order_by_set.to_sql())
            # Check if it's a list representing GraphQL OrderBy input (e.g., [{'ipAddress': 'asc'}])
            elif isinstance(order_by, list):
                # Convert list to SQL ORDER BY
                from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

                order_by_set = _convert_order_by_input_to_sql(order_by)
                if order_by_set:
                    query_parts.append(SQL(" ") + order_by_set.to_sql())
            # Otherwise treat as a simple string
            else:
                query_parts.append(SQL(" ORDER BY ") + SQL(order_by))

        # Handle limit and offset
        if limit is not None:
            query_parts.append(SQL(" LIMIT ") + Literal(limit))
            if offset is not None:
                query_parts.append(SQL(" OFFSET ") + Literal(offset))

        statement = SQL("").join(query_parts)
        # Since we now use Composed SQL with embedded Literals for all conditions,
        # params should be empty to avoid parameter mixing
        return DatabaseQuery(statement=statement, params={}, fetch_result=True)

    def _build_find_one_query(
        self,
        view_name: str,
        raw_json: bool = False,
        field_paths: list[Any] | None = None,
        info: Any = None,
        jsonb_column: str | None = None,
        **kwargs,
    ) -> DatabaseQuery:
        """Build a SELECT query for finding a single record."""
        # Force limit=1 for find_one
        kwargs["limit"] = 1
        return self._build_find_query(
            view_name,
            raw_json=raw_json,
            field_paths=field_paths,
            info=info,
            jsonb_column=jsonb_column,
            **kwargs,
        )

    async def _get_table_columns_cached(self, view_name: str) -> set[str] | None:
        """Get table columns with caching.

        Returns set of column names or None if unable to retrieve.
        """
        if not hasattr(self, "_introspected_columns"):
            self._introspected_columns = {}

        if view_name in self._introspected_columns:
            return self._introspected_columns[view_name]

        try:
            columns = await self._introspect_table_columns(view_name)
            self._introspected_columns[view_name] = columns
            return columns
        except Exception:
            return None

    def _convert_dict_where_to_sql(
        self,
        where_dict: dict[str, Any],
        view_name: str | None = None,
        table_columns: set[str] | None = None,
    ) -> Composed | None:
        """Convert a dictionary WHERE clause to SQL conditions.

        This method handles dynamically constructed where clauses used in GraphQL resolvers.
        Unlike WhereInput types (which use JSONB paths), dictionary filters use direct
        column names for regular tables.

        Args:
            where_dict: Dictionary with field names as keys and operator dictionaries as values
                       e.g., {'name': {'contains': 'router'}, 'port': {'gt': 20}}
            view_name: Optional view/table name for hybrid table detection
            table_columns: Optional set of actual table columns for accurate detection

        Returns:
            A Composed SQL object with parameterized conditions, or None if no valid conditions
        """
        from psycopg.sql import SQL, Composed

        conditions = []

        for field_name, field_filter in where_dict.items():
            if field_filter is None:
                continue

            # Convert GraphQL field names to database field names
            db_field_name = self._convert_field_name_to_database(field_name)

            if isinstance(field_filter, dict):
                # Check if this might be a nested object filter (e.g., {machine: {id: {eq: value}}})
                # Nested object filters have 'id' as a key with a dict value containing operators
                is_nested_object = False
                if "id" in field_filter and isinstance(field_filter["id"], dict):
                    # This looks like a nested object filter
                    # Check if we have a corresponding SQL column for this relationship
                    potential_fk_column = f"{db_field_name}_id"
                    if table_columns and potential_fk_column in table_columns:
                        # We have a SQL column for this relationship, use it directly
                        is_nested_object = True
                        # Extract the filter value from the nested structure
                        id_filter = field_filter["id"]
                        for operator, value in id_filter.items():
                            if value is None:
                                continue
                            # Build condition using the FK column directly
                            condition_sql = self._build_dict_where_condition(
                                potential_fk_column, operator, value, view_name, table_columns
                            )
                            if condition_sql:
                                conditions.append(condition_sql)

                if not is_nested_object:
                    # Handle regular operator-based filtering: {'contains': 'router', 'gt': 10}
                    field_conditions = []

                    for operator, value in field_filter.items():
                        if value is None:
                            continue

                        # Build SQL condition using converted database field name
                        condition_sql = self._build_dict_where_condition(
                            db_field_name, operator, value, view_name, table_columns
                        )
                        if condition_sql:
                            field_conditions.append(condition_sql)

                    # Combine multiple conditions for the same field with AND
                    if field_conditions:
                        if len(field_conditions) == 1:
                            conditions.append(field_conditions[0])
                        else:
                            # Multiple conditions for same field: (cond1 AND cond2 AND ...)
                            combined_parts = []
                            for i, cond in enumerate(field_conditions):
                                if i > 0:
                                    combined_parts.append(SQL(" AND "))
                                combined_parts.append(cond)
                            conditions.append(Composed([SQL("("), *combined_parts, SQL(")")]))

            else:
                # Handle simple equality: {'status': 'active'}
                condition_sql = self._build_dict_where_condition(db_field_name, "eq", field_filter)
                if condition_sql:
                    conditions.append(condition_sql)

        # Combine all field conditions with AND
        if not conditions:
            return None

        if len(conditions) == 1:
            return conditions[0]

        # Multiple field conditions: (field1_cond AND field2_cond AND ...)
        result_parts = []
        for i, condition in enumerate(conditions):
            if i > 0:
                result_parts.append(SQL(" AND "))
            result_parts.append(condition)

        return Composed(result_parts)

    def _build_dict_where_condition(
        self,
        field_name: str,
        operator: str,
        value: Any,
        view_name: str | None = None,
        table_columns: set[str] | None = None,
    ) -> Composed | None:
        """Build a single WHERE condition using FraiseQL's operator strategy system.

        This method now uses the sophisticated operator strategy system instead of
        primitive SQL templates, enabling features like IP address type casting,
        MAC address handling, and other advanced field type detection.

        For hybrid tables (with both regular columns and JSONB data), it determines
        whether to use direct column access or JSONB path based on the actual table structure.

        Args:
            field_name: Database field name (e.g., 'ip_address', 'port', 'status')
            operator: Filter operator (eq, contains, gt, in, etc.)
            value: Filter value
            view_name: Optional view/table name for hybrid table detection
            table_columns: Optional set of actual table columns (for accurate detection)

        Returns:
            Composed SQL condition with intelligent type casting, or None if operator not supported
        """
        from psycopg.sql import SQL, Composed, Identifier, Literal

        from fraiseql.sql.operator_strategies import get_operator_registry

        try:
            # Get the operator strategy registry (contains the v0.7.1 IP filtering fixes)
            registry = get_operator_registry()

            # Determine if this field is a regular column or needs JSONB path
            use_jsonb_path = False

            if table_columns is not None:
                # We have actual column info - use it!
                # Field is JSONB if: table has 'data' column AND field is NOT a regular column
                has_data_column = "data" in table_columns
                is_regular_column = field_name in table_columns
                use_jsonb_path = has_data_column and not is_regular_column
            elif view_name:
                # Fall back to heuristic-based detection
                use_jsonb_path = self._should_use_jsonb_path_sync(view_name, field_name)

            if use_jsonb_path:
                # Field is in JSONB data column, use JSONB path
                path_sql = Composed([SQL("data"), SQL(" ->> "), Literal(field_name)])
            else:
                # Field is a regular column, use direct column name
                path_sql = Identifier(field_name)

            # Get the appropriate strategy for this operator
            # field_type=None triggers fallback detection (IP addresses, MAC addresses, etc.)
            strategy = registry.get_strategy(operator, field_type=None)

            if strategy is None:
                # Operator not supported by strategy system, fall back to basic handling
                return self._build_basic_dict_condition(
                    field_name, operator, value, use_jsonb_path=use_jsonb_path
                )

            # Use the strategy to build intelligent SQL with type detection
            # This is where the IP filtering fixes from v0.7.1 are applied
            sql_condition = strategy.build_sql(path_sql, operator, value, field_type=None)

            return sql_condition

        except Exception as e:
            # If strategy system fails, fall back to basic condition building
            logger.warning(f"Operator strategy failed for {field_name} {operator} {value}: {e}")
            return self._build_basic_dict_condition(field_name, operator, value)

    def _build_basic_dict_condition(
        self, field_name: str, operator: str, value: Any, use_jsonb_path: bool = False
    ) -> Composed | None:
        """Fallback method for basic WHERE condition building.

        This provides basic SQL generation when the operator strategy system
        is not available or fails. Used as a safety fallback.
        """
        from psycopg.sql import SQL, Composed, Identifier, Literal

        # Basic operator templates for fallback scenarios
        basic_operators = {
            "eq": lambda path, val: Composed([path, SQL(" = "), Literal(val)]),
            "neq": lambda path, val: Composed([path, SQL(" != "), Literal(val)]),
            "gt": lambda path, val: Composed([path, SQL(" > "), Literal(val)]),
            "gte": lambda path, val: Composed([path, SQL(" >= "), Literal(val)]),
            "lt": lambda path, val: Composed([path, SQL(" < "), Literal(val)]),
            "lte": lambda path, val: Composed([path, SQL(" <= "), Literal(val)]),
            "ilike": lambda path, val: Composed([path, SQL(" ILIKE "), Literal(val)]),
            "like": lambda path, val: Composed([path, SQL(" LIKE "), Literal(val)]),
            "isnull": lambda path, val: Composed(
                [path, SQL(" IS NULL" if val else " IS NOT NULL")]
            ),
        }

        if operator not in basic_operators:
            return None

        # Build path based on whether this is a JSONB field or regular column
        if use_jsonb_path:
            # Use JSONB path for fields in data column
            path_sql = Composed([SQL("data"), SQL(" ->> "), Literal(field_name)])
        else:
            # Use direct column name for regular columns
            path_sql = Identifier(field_name)

        # Generate basic condition
        return basic_operators[operator](path_sql, value)

    async def _introspect_table_columns(self, view_name: str) -> set[str]:
        """Introspect actual table columns from database information_schema.

        This provides accurate column information for hybrid tables.
        Results are cached for performance.
        """
        if not hasattr(self, "_introspected_columns"):
            self._introspected_columns = {}

        if view_name in self._introspected_columns:
            return self._introspected_columns[view_name]

        try:
            # Query information_schema to get actual columns
            # PERFORMANCE: Use a single query to get all we need
            query = """
                SELECT
                    column_name,
                    data_type,
                    udt_name
                FROM information_schema.columns
                WHERE table_name = %s
                AND table_schema = 'public'
                ORDER BY ordinal_position
            """

            async with self._pool.connection() as conn, conn.cursor() as cursor:
                await cursor.execute(query, (view_name,))
                rows = await cursor.fetchall()

                # Extract column names and identify if JSONB exists
                columns = set()
                has_jsonb_data = False

                for row in rows:
                    # Handle both dict and tuple cursor results
                    if isinstance(row, dict):
                        col_name = row.get("column_name")
                        udt_name = row.get("udt_name", "")
                    else:
                        # Tuple-based result (column_name, data_type, udt_name)
                        col_name = row[0] if row else None
                        udt_name = row[2] if len(row) > 2 else ""

                    if col_name:
                        columns.add(col_name)

                        # Check if this is a JSONB data column
                        if col_name == "data" and udt_name == "jsonb":
                            has_jsonb_data = True

                # Cache the result
                self._introspected_columns[view_name] = columns

                # Also cache whether this table has JSONB data column
                if not hasattr(self, "_table_has_jsonb"):
                    self._table_has_jsonb = {}
                self._table_has_jsonb[view_name] = has_jsonb_data

                return columns

        except Exception as e:
            logger.warning(f"Failed to introspect table {view_name}: {e}")
            # Cache empty set to avoid repeated failures
            self._introspected_columns[view_name] = set()
            return set()

    def _should_use_jsonb_path_sync(self, view_name: str, field_name: str) -> bool:
        """Check if a field should use JSONB path or direct column access.

        PERFORMANCE OPTIMIZED:
        - Uses metadata from registration time (no DB queries)
        - Single cache lookup per field
        - Fast path for registered tables
        """
        # Fast path: use cached decision if available
        if not hasattr(self, "_field_path_cache"):
            self._field_path_cache = {}

        cache_key = f"{view_name}:{field_name}"
        cached_result = self._field_path_cache.get(cache_key)
        if cached_result is not None:
            return cached_result

        # BEST CASE: Check registration-time metadata first (no DB query needed)
        if view_name in _table_metadata:
            metadata = _table_metadata[view_name]
            columns = metadata.get("columns", set())
            has_jsonb = metadata.get("has_jsonb_data", False)

            # Use JSONB path only if: has data column AND field is not a regular column
            use_jsonb = has_jsonb and field_name not in columns
            self._field_path_cache[cache_key] = use_jsonb
            return use_jsonb

        # SECOND BEST: Check if we have runtime introspected columns
        if hasattr(self, "_introspected_columns") and view_name in self._introspected_columns:
            columns = self._introspected_columns[view_name]
            has_data_column = "data" in columns
            is_regular_column = field_name in columns

            # Use JSONB path only if: has data column AND field is not a regular column
            use_jsonb = has_data_column and not is_regular_column
            self._field_path_cache[cache_key] = use_jsonb
            return use_jsonb

        # Fallback: Use fast heuristic for known patterns
        # PERFORMANCE: This avoids DB queries for common cases
        if not hasattr(self, "_table_has_jsonb"):
            self._table_has_jsonb = {}

        if view_name not in self._table_has_jsonb:
            # Quick pattern matching for known table types
            known_hybrid_patterns = ("jsonb", "hybrid")
            known_regular_patterns = ("test_product", "test_item", "users", "companies", "orders")

            view_lower = view_name.lower()
            if any(p in view_lower for p in known_regular_patterns):
                self._table_has_jsonb[view_name] = False
            elif any(p in view_lower for p in known_hybrid_patterns):
                self._table_has_jsonb[view_name] = True
            else:
                # Conservative default: assume regular table
                self._table_has_jsonb[view_name] = False

        # If no JSONB data column, always use direct access
        if not self._table_has_jsonb[view_name]:
            self._field_path_cache[cache_key] = False
            return False

        # For hybrid tables, use a small set of known regular columns
        # PERFORMANCE: Using frozenset for O(1) lookup
        REGULAR_COLUMNS = frozenset(
            {
                "id",
                "tenant_id",
                "created_at",
                "updated_at",
                "name",
                "status",
                "type",
                "category_id",
                "identifier",
                "is_active",
                "is_featured",
                "is_available",
                "is_deleted",
                "start_date",
                "end_date",
                "created_date",
                "modified_date",
            }
        )

        use_jsonb = field_name not in REGULAR_COLUMNS
        self._field_path_cache[cache_key] = use_jsonb
        return use_jsonb

    def _where_obj_to_dict(self, where_obj: Any, table_columns: set[str]) -> dict[str, Any] | None:
        """Convert a WHERE object to a dictionary for hybrid table processing.

        This method examines a WHERE object and converts it to a dictionary format
        that can be processed by our dict-based WHERE handler, which knows how to
        handle nested objects in hybrid tables correctly.

        Args:
            where_obj: The WHERE object with to_sql() method
            table_columns: Set of actual table column names

        Returns:
            Dictionary representation of the WHERE clause, or None if conversion fails
        """
        result = {}

        # Iterate through attributes of the where object
        if hasattr(where_obj, "__dict__"):
            for field_name, field_value in where_obj.__dict__.items():
                if field_value is None:
                    continue

                # Skip special fields
                if field_name.startswith("_"):
                    continue

                # Check if this is a nested object filter
                if hasattr(field_value, "__dict__"):
                    # Check if it has an 'id' field with filter operators
                    id_value = getattr(field_value, "id", None)
                    if hasattr(field_value, "id") and isinstance(id_value, dict):
                        # This is a nested object filter, convert to dict format
                        result[field_name] = {"id": id_value}
                    else:
                        # Try to convert recursively
                        nested_dict = {
                            nested_field: nested_value
                            for nested_field, nested_value in field_value.__dict__.items()
                            if nested_value is not None and not nested_field.startswith("_")
                        }
                        if nested_dict:
                            result[field_name] = nested_dict
                elif isinstance(field_value, dict):
                    # Direct dict value, use as-is
                    result[field_name] = field_value
                elif isinstance(field_value, (str, int, float, bool)):
                    # Scalar value, wrap in eq operator
                    result[field_name] = {"eq": field_value}

        return result if result else None

    def _convert_field_name_to_database(self, field_name: str) -> str:
        """Convert GraphQL field name to database field name.

        Automatically converts camelCase to snake_case while preserving
        existing snake_case names for backward compatibility.

        Args:
            field_name: GraphQL field name (camelCase or snake_case)

        Returns:
            Database field name in snake_case

        Examples:
            'ipAddress' -> 'ip_address'
            'status' -> 'status' (unchanged)
        """
        if not field_name or not isinstance(field_name, str):
            return field_name or ""

        # Preserve existing snake_case for backward compatibility
        if "_" in field_name:
            return field_name

        # Convert camelCase to snake_case
        return to_snake_case(field_name)
