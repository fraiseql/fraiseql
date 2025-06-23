"""Translate GraphQL queries into PostgreSQL SQL statements."""

from graphql import GraphQLError
from psycopg.sql import SQL, Composed

from fraiseql.core.ast_parser import extract_flat_paths, parse_query_ast
from fraiseql.sql.sql_generator import build_sql_query


def translate_query(
    query: str,
    table: str,
    typename: str,
    where_clause: SQL | None = None,
    order_by: list[tuple[str, str]] | None = None,
    group_by: list[str] | None = None,
    auto_camel_case: bool = False,
) -> Composed | SQL:
    """Translate a GraphQL query string into a PostgreSQL SQL query.

    This function:
    - Parses the GraphQL query AST
    - Resolves fragments and flattens field paths
    - Builds a SQL statement returning JSONB objects according to the query

    Args:
        query: GraphQL query string.
        table: Database table to query.
        typename: GraphQL type name for the results.
        where_clause: Optional SQL WHERE clause to filter results.
        order_by: Optional list of (field_path, direction) tuples for ORDER BY.
        group_by: Optional list of field paths for GROUP BY.
        auto_camel_case: Whether to automatically convert snake_case DB fields to
            camelCase GraphQL fields.

    Returns:
        A psycopg `Composed` or `SQL` object representing the SQL query.

    Raises:
        GraphQLError: If the GraphQL query parsing fails.
    """
    try:
        op, fragments = parse_query_ast(query)
    except Exception as exc:
        msg = f"Failed to parse query: {exc}"
        raise GraphQLError(msg) from exc

    # Apply snake_case transformation if auto_camel_case is enabled
    transform_fn = None
    if auto_camel_case:
        from fraiseql.utils.casing import to_snake_case

        transform_fn = to_snake_case

    field_paths = extract_flat_paths(op.selection_set, fragments, transform_path=transform_fn)

    return build_sql_query(
        table=table,
        field_paths=field_paths,
        where_clause=where_clause,
        json_output=True,
        typename=typename,
        order_by=order_by,
        group_by=group_by,
        auto_camel_case=auto_camel_case,
    )
