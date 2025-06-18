"""SQL generator for building SELECT queries with GraphQL-aware JSONB projection.

Supports JSONB path extraction and camelCase aliasing for GraphQL field names.
Integrates deeply with FraiseQL's AST selection and schema mappings to construct
optimized PostgreSQL queries using `jsonb_build_object` for minimal post-processing.
"""

from collections.abc import Sequence

from psycopg import sql
from psycopg.sql import SQL, Composed, Identifier

from fraiseql.core.ast_parser import FieldPath


def build_sql_query(
    table: str,
    field_paths: Sequence[FieldPath],
    where_clause: SQL | None = None,
    *,
    json_output: bool = False,
    typename: str | None = None,
    order_by: Sequence[tuple[str, str]] | None = None,
    group_by: Sequence[str] | None = None,
    auto_camel_case: bool = False,
) -> Composed:
    """Build a SELECT SQL query using jsonb path extraction and optional WHERE/ORDER BY/GROUP BY.

    If `json_output` is True, wraps the result in jsonb_build_object(...)
    and aliases it as `result`. Adds '__typename' if `typename` is provided.

    Args:
        table: Table name to query
        field_paths: Sequence of field paths to extract
        where_clause: Optional WHERE clause
        json_output: Whether to wrap output in jsonb_build_object
        typename: Optional GraphQL typename to include
        order_by: Optional list of (field_path, direction) tuples for ORDER BY
        group_by: Optional list of field paths for GROUP BY
        auto_camel_case: Whether to convert field paths to snake_case (when DB uses snake_case)
    """
    object_pairs: list[sql.Composable] = []

    for field in field_paths:
        json_path_parts = [sql.Literal(part) for part in field.path[:-1]]
        last_part = field.path[-1]

        expr = sql.SQL("data")
        for key in json_path_parts:
            expr = sql.SQL("{}->{}").format(expr, key)

        expr = sql.SQL("{}->>{}").format(expr, sql.Literal(last_part))
        object_pairs.append(sql.Literal(field.alias))
        object_pairs.append(expr)

    if typename is not None:
        object_pairs.append(sql.Literal("__typename"))
        object_pairs.append(sql.Literal(typename))

    if json_output:
        select_clause = SQL("jsonb_build_object({}) AS result").format(SQL(", ").join(object_pairs))
    else:
        select_items = [
            SQL("{} AS {}{}").format(expr, Identifier(field.alias), SQL(""))
            for field, expr in zip(field_paths, object_pairs[1::2], strict=False)
        ]
        select_clause = SQL(", ").join(select_items)

    base = SQL("SELECT {} FROM {}").format(select_clause, Identifier(table))

    # Build query with clauses in correct SQL order
    query_parts = [base]

    if where_clause:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(where_clause)

    if group_by:
        group_by_parts = []
        for field_path in group_by:
            # Convert field path (e.g., "profile.age") to JSONB expression
            path_parts = field_path.split(".")

            # Apply snake_case transformation if auto_camel_case is enabled
            if auto_camel_case:
                from fraiseql.utils.casing import to_snake_case

                path_parts = [to_snake_case(part) for part in path_parts]

            if len(path_parts) == 1:
                # Top-level field
                expr = SQL("data->>{}").format(sql.Literal(path_parts[0]))
            else:
                # Nested field
                json_path = [sql.Literal(part) for part in path_parts[:-1]]
                expr = SQL("data")
                for part in json_path:
                    expr = SQL("{}->{}").format(expr, part)
                expr = SQL("{}->>{}").format(expr, sql.Literal(path_parts[-1]))
            group_by_parts.append(expr)

        query_parts.append(SQL(" GROUP BY "))
        query_parts.append(SQL(", ").join(group_by_parts))

    if order_by:
        order_by_parts = []
        for field_path, direction in order_by:
            # Convert field path to JSONB expression
            path_parts = field_path.split(".")

            # Apply snake_case transformation if auto_camel_case is enabled
            if auto_camel_case:
                from fraiseql.utils.casing import to_snake_case

                path_parts = [to_snake_case(part) for part in path_parts]

            if len(path_parts) == 1:
                # Top-level field
                expr = SQL("data->>{}").format(sql.Literal(path_parts[0]))
            else:
                # Nested field
                json_path = [sql.Literal(part) for part in path_parts[:-1]]
                expr = SQL("data")
                for part in json_path:
                    expr = SQL("{}->{}").format(expr, part)
                expr = SQL("{}->>{}").format(expr, sql.Literal(path_parts[-1]))

            # Add direction (ASC/DESC)
            order_expr = expr + SQL(" ") + SQL(direction.upper())
            order_by_parts.append(order_expr)

        query_parts.append(SQL(" ORDER BY "))
        query_parts.append(SQL(", ").join(order_by_parts))

    return sql.Composed(query_parts)
