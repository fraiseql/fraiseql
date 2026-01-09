"""Pure Python query building for FraiseQL database operations.

This module builds SQL queries from high-level specifications without executing them.
All query building is independent of the database execution layer.

Query Types:
- find: SELECT with WHERE, ORDER BY, LIMIT, OFFSET
- find_one: Single record SELECT (LIMIT 1)
- aggregations: Built via _build_where_clause for count, sum, avg, etc.

Architecture:
- WHERE normalization via fraiseql.where_normalization
- Operator strategy system for intelligent SQL generation
- Supports both SQL columns and JSONB paths for hybrid tables
- Handles schema-qualified table names (schema.table)
"""

import logging
from dataclasses import dataclass
from typing import Any

from psycopg.sql import SQL, Composed, Identifier, Literal

from fraiseql.db.registry import _table_metadata
from fraiseql.sql.operators import get_default_registry as get_operator_registry
from fraiseql.utils.casing import to_snake_case
from fraiseql.where_clause import WhereClause
from fraiseql.where_normalization import normalize_dict_where, normalize_whereinput

logger = logging.getLogger(__name__)


@dataclass
class DatabaseQuery:
    """Encapsulates a SQL query, parameters, and fetch flag."""

    statement: Composed | SQL
    params: list[Any] | dict[str, Any] | None = None
    fetch_result: bool = True


def build_find_query(
    view_name: str,
    field_paths: list[Any] | None = None,
    info: Any = None,
    jsonb_column: str | None = None,
    table_columns: set[str] | None = None,
    where_parts: list[Any] | None = None,
    where_params: dict[str, Any] | None = None,
    limit: int | None = None,
    offset: int | None = None,
    order_by: Any = None,
) -> DatabaseQuery:
    """Build a SELECT query for finding multiple records.

    Unified Rust-first: always SELECT jsonb_column::text or row_to_json()
    Rust handles field projection, not PostgreSQL!

    Args:
        view_name: Name of the view to query
        field_paths: Optional field paths for projection (passed to Rust)
        info: Optional GraphQL resolve info
        jsonb_column: JSONB column name to use
        table_columns: Optional set of actual table columns (for hybrid detection)
        where_parts: List of SQL WHERE conditions (from _build_where_clause)
        where_params: Parameters for WHERE conditions
        limit: Maximum number of rows to return
        offset: Number of rows to skip
        order_by: ORDER BY specification (OrderBySet, dict, list, or string)

    Returns:
        DatabaseQuery with statement, params, and fetch_result=True
    """
    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    if jsonb_column is None:
        # For tables with jsonb_column=None, select all columns as JSON
        # This allows the Rust pipeline to extract individual fields
        query_parts = [
            SQL("SELECT row_to_json(t)::text FROM "),
            table_identifier,
            SQL(" AS t"),
        ]
    else:
        # For JSONB tables, select the JSONB column as text
        target_jsonb_column = jsonb_column or "data"
        query_parts = [
            SQL("SELECT "),
            Identifier(target_jsonb_column),
            SQL("::text FROM "),
            table_identifier,
        ]

    # Add WHERE clause
    if where_parts:
        where_sql_parts = []
        for part in where_parts:
            if isinstance(part, (SQL, Composed)):
                where_sql_parts.append(part)
            else:
                where_sql_parts.append(SQL(part))
        if where_sql_parts:
            query_parts.extend([SQL(" WHERE "), SQL(" AND ").join(where_sql_parts)])

    # Determine table reference for ORDER BY
    # For JSONB tables, use the column name; for non-JSONB tables, use table alias "t"
    table_ref = jsonb_column if jsonb_column is not None else "t"

    # Add ORDER BY
    if order_by:
        if hasattr(order_by, "to_sql"):
            order_sql = order_by.to_sql(table_ref)
            if order_sql:
                # OrderBySet.to_sql() already includes "ORDER BY " prefix
                query_parts.append(SQL(" "))
                query_parts.append(order_sql)
        elif hasattr(order_by, "_to_sql_order_by"):
            # Convert GraphQL OrderByInput to SQL OrderBySet, then get SQL
            sql_order_by_obj = order_by._to_sql_order_by()
            if sql_order_by_obj and hasattr(sql_order_by_obj, "to_sql"):
                order_sql = sql_order_by_obj.to_sql(table_ref)
                if order_sql:
                    query_parts.append(SQL(" "))
                    query_parts.append(order_sql)
        elif isinstance(order_by, (dict, list)):
            # Convert dict or list-style order by input to SQL OrderBySet
            from fraiseql.sql.graphql_order_by_generator import (
                _convert_order_by_input_to_sql,
            )

            sql_order_by_obj = _convert_order_by_input_to_sql(order_by)
            if sql_order_by_obj and hasattr(sql_order_by_obj, "to_sql"):
                order_sql = sql_order_by_obj.to_sql(table_ref)
                if order_sql:
                    query_parts.append(SQL(" "))
                    query_parts.append(order_sql)
        elif isinstance(order_by, str):
            query_parts.extend([SQL(" ORDER BY "), SQL(order_by)])

    # Add LIMIT
    if limit is not None:
        query_parts.extend([SQL(" LIMIT "), Literal(limit)])

    # Add OFFSET
    if offset is not None:
        query_parts.extend([SQL(" OFFSET "), Literal(offset)])

    statement = SQL("").join(query_parts)
    return DatabaseQuery(statement=statement, params=where_params or {}, fetch_result=True)


def build_find_one_query(
    view_name: str,
    field_paths: list[Any] | None = None,
    info: Any = None,
    jsonb_column: str | None = None,
    table_columns: set[str] | None = None,
    where_parts: list[Any] | None = None,
    where_params: dict[str, Any] | None = None,
    order_by: Any = None,
) -> DatabaseQuery:
    """Build a SELECT query for finding a single record.

    Wrapper that forces LIMIT 1 for find_one queries.
    """
    return build_find_query(
        view_name,
        field_paths=field_paths,
        info=info,
        jsonb_column=jsonb_column,
        table_columns=table_columns,
        where_parts=where_parts,
        where_params=where_params,
        limit=1,
        order_by=order_by,
    )


def build_where_clause(
    view_name: str,
    table_columns: set[str] | None = None,
    jsonb_column: str | None = None,
    where: Any = None,
    **kwargs: Any,
) -> tuple[list[Any], dict[str, Any]]:
    """Build WHERE clause from kwargs.

    Unified WHERE clause building from kwargs - single code path for all query types.
    Used by count(), exists(), sum(), avg(), min(), max(), distinct(), pluck(), aggregate().

    Args:
        view_name: View/table name for metadata lookup
        table_columns: Optional set of actual table columns (for hybrid table detection)
        jsonb_column: Optional JSONB column name
        where: Optional WHERE clause (WhereClause, dict, or WhereInput)
        **kwargs: Remaining kwargs treated as equality filters

    Returns:
        Tuple of (where_parts, params) where:
        - where_parts: List of SQL/Composed conditions
        - params: Dict of parameters for SQL placeholders
    """
    where_parts: list[Any] = []
    params = {}

    if where is not None:
        # Normalize WHERE to WhereClause (single code path)
        where_clause = normalize_where(where, view_name, table_columns)

        # Convert WhereClause to SQL
        try:
            where_sql = where_clause.to_sql(
                table_columns=table_columns,
                jsonb_column=jsonb_column,
                registry=get_operator_registry(),
            )
            if where_sql:
                where_parts.append(where_sql)
        except Exception as e:
            logger.error(
                f"WHERE clause building failed for {view_name}: {e}",
                exc_info=False,
            )
            raise

    # Process remaining kwargs as simple equality conditions
    for key, value in kwargs.items():
        if key not in ("limit", "offset", "order_by"):
            # Convert camelCase to snake_case for GraphQL compatibility
            db_field = to_snake_case(key)

            # Check if field uses JSONB path or direct column access
            use_jsonb_path = _should_use_jsonb_path(
                view_name, db_field, table_columns, jsonb_column
            )

            if use_jsonb_path:
                # Use JSONB path for fields in data column
                jsonb_col = jsonb_column or "data"
                condition = Composed(
                    [
                        Identifier(jsonb_col),
                        SQL(" ->> "),
                        Literal(db_field),
                        SQL(" = "),
                        Literal(value),
                    ]
                )
            else:
                # Use direct column reference
                condition = Composed([Identifier(db_field), SQL(" = "), Literal(value)])

            where_parts.append(condition)

    return (where_parts, params)


def normalize_where(
    where: Any,
    view_name: str,
    table_columns: set[str] | None = None,
) -> WhereClause:
    """Single entry point for WHERE normalization.

    Converts dict and WhereInput to canonical WhereClause.

    Args:
        where: WHERE specification (WhereClause, dict, or WhereInput)
        view_name: View/table name for metadata lookup
        table_columns: Optional set of actual table columns

    Returns:
        Normalized WhereClause object
    """
    # Already normalized
    if isinstance(where, WhereClause):
        return where

    # Dict-based WHERE (e.g., {"email": {"eq": "test@example.com"}})
    if isinstance(where, dict):
        return normalize_dict_where(where, view_name, table_columns)

    # WhereInput objects (GraphQL input types)
    if hasattr(where, "__dataclass_fields__"):
        # Convert dataclass WhereInput to dict, then normalize
        where_dict = {k: getattr(where, k, None) for k in where.__dataclass_fields__}
        return normalize_dict_where(where_dict, view_name, table_columns)

    # Try WhereInput interface
    if hasattr(where, "to_dict"):
        return normalize_dict_where(where.to_dict(), view_name, table_columns)

    # Fallback: try as WhereInput directly
    return normalize_whereinput(where, view_name, table_columns)


def build_dict_where_condition(
    field_name: str,
    operator: str,
    value: Any,
    view_name: str | None = None,
    table_columns: set[str] | None = None,
    jsonb_column: str | None = None,
) -> Composed | None:
    """Build a single WHERE condition using operator strategy system.

    Uses sophisticated operator strategy system for intelligent SQL generation
    with type detection (IP addresses, MAC addresses, etc.).

    For hybrid tables (with both regular columns and JSONB data), determines
    whether to use direct column access or JSONB path based on table structure.

    Args:
        field_name: Database field name (e.g., 'ip_address', 'port', 'status')
        operator: Filter operator (eq, contains, gt, in, etc.)
        value: Filter value
        view_name: Optional view/table name for hybrid table detection
        table_columns: Optional set of actual table columns (for accurate detection)
        jsonb_column: Optional JSONB column name (if set, use JSONB paths for all non-id fields)

    Returns:
        Composed SQL condition with intelligent type casting, or None if operator not supported
    """
    try:
        # Get the operator strategy registry
        registry = get_operator_registry()

        # Determine if this field is a regular column or needs JSONB path
        use_jsonb_path = False

        # IMPORTANT: Check table_columns FIRST for hybrid tables (Issue #124)
        # For hybrid tables with FK columns, we must use the SQL FK column, not JSONB path
        if table_columns is not None and field_name in table_columns:
            # This field is a real SQL column - never use JSONB path for it
            use_jsonb_path = False
        elif jsonb_column:
            # Explicit JSONB column specified - use JSONB paths for non-id fields
            use_jsonb_path = field_name != "id"
        elif table_columns is not None:
            # We have column info, but field is not in columns - check if it's in JSONB
            has_data_column = "data" in table_columns
            use_jsonb_path = has_data_column
        elif view_name:
            # Fall back to heuristic-based detection
            use_jsonb_path = _should_use_jsonb_path(view_name, field_name)

        if use_jsonb_path:
            # Field is in JSONB data column, use JSONB path
            jsonb_col = jsonb_column or "data"
            path_sql = Composed([Identifier(jsonb_col), SQL(" ->> "), Literal(field_name)])
        else:
            # Field is a regular column, use direct column name
            path_sql = Identifier(field_name)

        # Get the appropriate strategy for this operator
        # field_type=None triggers fallback detection (IP addresses, MAC addresses, etc.)
        strategy = registry.get_strategy(operator, field_type=None)

        if strategy is None:
            # Operator not supported by strategy system, fall back to basic handling
            return build_basic_dict_condition(
                field_name,
                operator,
                value,
                use_jsonb_path=use_jsonb_path,
            )

        # Use the strategy to build intelligent SQL with type detection
        return strategy.build_sql(
            operator=operator,
            value=value,
            path_sql=path_sql,
            field_type=None,
            jsonb_column=jsonb_column if use_jsonb_path else None,
        )

    except Exception as e:
        # If strategy system fails, fall back to basic condition building
        logger.warning(f"Operator strategy failed for {field_name} {operator} {value}: {e}")
        return build_basic_dict_condition(field_name, operator, value)


def build_basic_dict_condition(
    field_name: str,
    operator: str,
    value: Any,
    use_jsonb_path: bool = False,
) -> Composed | None:
    """Fallback method for basic WHERE condition building.

    Provides basic SQL generation when the operator strategy system
    is not available or fails. Used as a safety fallback.
    """
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
            [path, SQL(" IS NULL" if val else " IS NOT NULL")],
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


def _should_use_jsonb_path(
    view_name: str,
    field_name: str,
    table_columns: set[str] | None = None,
    jsonb_column: str | None = None,
) -> bool:
    """Determine if field should use JSONB path or direct column access.

    Uses heuristic-based detection for known patterns.

    Args:
        view_name: View/table name
        field_name: Field name to check
        table_columns: Optional actual table columns (overrides heuristics)
        jsonb_column: Optional JSONB column name

    Returns:
        True if field should use JSONB path, False for direct column access
    """
    # If explicit table columns provided, use them
    if table_columns is not None:
        return "data" in table_columns and field_name not in table_columns

    # If explicit JSONB column specified, use it for non-id fields
    if jsonb_column is not None:
        return field_name != "id"

    # Check metadata from registration time
    if view_name in _table_metadata:
        metadata = _table_metadata[view_name]
        columns = metadata.get("columns", set())
        has_jsonb = metadata.get("has_jsonb_data", False)

        # Use JSONB path only if: has data column AND field is not a regular column
        return has_jsonb and field_name not in columns

    # Heuristic-based detection for known patterns
    known_hybrid_patterns = ("jsonb", "hybrid")
    known_regular_patterns = ("test_product", "test_item", "users", "companies", "orders")

    view_lower = view_name.lower()
    if any(p in view_lower for p in known_regular_patterns):
        return False
    if any(p in view_lower for p in known_hybrid_patterns):
        return True

    # Conservative default: assume regular table
    return False
