"""Enhanced raw JSON query handler with advanced detection.

This module detects simple queries that can be executed directly
without going through GraphQL validation and serialization.
"""

import logging
import re
from typing import TYPE_CHECKING, Any, Dict, Optional

from graphql import GraphQLSchema

if TYPE_CHECKING:
    from fraiseql.analysis.query_analyzer import QueryAnalyzer
from fraiseql.core.ast_parser import extract_field_paths_from_info, parse_query_ast
from fraiseql.core.raw_json_executor import RawJSONResult

logger = logging.getLogger(__name__)


async def try_raw_json_execution(
    schema: GraphQLSchema,
    query: str,
    variables: dict[str, Any] | None,
    context: dict[str, Any],
) -> Optional[RawJSONResult]:
    """Try to execute a query using raw JSON passthrough.

    This function checks if a query is simple enough to bypass GraphQL
    execution and use raw JSON passthrough directly.

    Args:
        schema: The GraphQL schema
        query: The GraphQL query string
        variables: Query variables
        context: The request context

    Returns:
        RawJSONResult if the query was executed via raw JSON, None otherwise
    """
    logger.info(
        f"try_raw_json_execution called: mode={context.get('mode')}, query_preview={query[:100]}..."
    )

    # Only in production mode
    if context.get("mode") != "production":
        logger.info("Raw JSON execution skipped: not in production mode")
        return None

    # Check if we have a database with raw JSON support
    db = context.get("db")
    logger.info(
        f"Database check: db_exists={db is not None}, db_type={type(db).__name__ if db else None}"
    )
    if db:
        has_find_raw_json = hasattr(db, "find_raw_json")
        available_methods = [m for m in dir(db) if "find" in m.lower()]
        logger.info(
            f"Database methods: find_raw_json={has_find_raw_json}, "
            f"available_methods={available_methods}"
        )

    if not db or not hasattr(db, "find_raw_json"):
        logger.info(
            f"Raw JSON execution skipped: db={db is not None}, "
            f"has_find_raw_json={hasattr(db, 'find_raw_json') if db else False}"
        )
        return None

    # Parse the query
    try:
        op, fragments = parse_query_ast(query)
        logger.info(f"Query parsed successfully: operation={op.operation if op else 'None'}")
    except Exception as e:
        logger.info(f"Failed to parse query for raw JSON: {e}")
        return None

    # Only handle simple queries (not mutations or subscriptions)
    from graphql import OperationType

    if op.operation != OperationType.QUERY:
        logger.info(f"Raw JSON execution skipped: operation type is {op.operation}, not QUERY")
        return None

    # Get the selections
    selections = op.selection_set.selections
    logger.info(f"Query selections count: {len(selections)}")
    if len(selections) != 1:
        logger.info("Raw JSON execution skipped: multiple root fields")
        return None

    # Get the single field
    field = selections[0]
    if field.kind != "field":
        logger.info(f"Raw JSON execution skipped: field kind is {field.kind}, not field")
        return None

    field_name = field.name.value
    logger.info(f"Processing field: {field_name}")

    # Check if this field exists in the schema
    query_type = schema.type_map.get("Query")
    if not query_type or field_name not in query_type.fields:
        logger.info(f"Raw JSON execution skipped: field {field_name} not found in schema")
        return None

    # Get the field definition
    field_def = query_type.fields[field_name]

    # Check if the resolver is simple (this is a heuristic)
    resolver = field_def.resolve
    if not resolver:
        return None

    # Try to get the original function (before wrapping)
    original_fn = resolver
    unwrap_count = 0
    while hasattr(original_fn, "__wrapped__") and unwrap_count < 10:
        original_fn = original_fn.__wrapped__
        unwrap_count += 1
        func_name = original_fn.__name__ if hasattr(original_fn, "__name__") else "unknown"
        logger.info(f"Unwrapped {unwrap_count} time(s), current function: {func_name}")

    # Check if it's a simple db query
    is_simple = _is_simple_db_query(original_fn)
    logger.info(f"Raw JSON execution: field={field_name}, is_simple_db_query={is_simple}")

    # TEMPORARY: Force raw JSON for specific fields we know are safe
    force_raw_json_fields = ["user", "allocations"]  # Add fields that should use raw JSON
    if field_name in force_raw_json_fields:
        logger.info(
            f"Forcing raw JSON execution for field '{field_name}' (bypass complexity check)"
        )
        # Continue with raw JSON execution
    elif not is_simple:
        logger.info("Raw JSON execution skipped: resolver is not a simple db query")
        return None

    # Extract the view name from the function
    view_name = _extract_view_name(original_fn)

    logger.info(f"Raw JSON execution: view_name={view_name}")
    if not view_name:
        logger.info("Raw JSON execution skipped: could not extract view name from resolver")
        return None

    # Build the arguments
    args = {}
    if field.arguments:
        for arg in field.arguments:
            arg_name = arg.name.value
            arg_value = arg.value

            # Handle variable references
            if hasattr(arg_value, "kind") and arg_value.kind == "variable":
                var_name = arg_value.name.value
                if variables and var_name in variables:
                    args[arg_name] = variables[var_name]
            else:
                # Handle literal values
                args[arg_name] = _get_literal_value(arg_value)

    # Create a mock info object for field path extraction
    class MockInfo:
        def __init__(self, field_nodes, fragments):
            self.field_nodes = field_nodes
            self.fragments = fragments

    mock_info = MockInfo([field], fragments)

    # Extract field paths
    from fraiseql.utils.casing import to_snake_case

    extract_field_paths_from_info(mock_info, transform_path=to_snake_case)

    # Store field name in db context
    db.context["graphql_field_name"] = field_name

    # Determine if it's find or find_one
    is_find_one = "find_one" in str(original_fn.__code__.co_names)

    # Add context filters that resolvers typically use
    # Check if tenant_id is in context and not already in args
    if "tenant_id" in context and "tenant_id" not in args:
        args["tenant_id"] = context["tenant_id"]

    # For user query specifically, add the contact_id as id filter
    if field_name == "user" and "id" not in args and "contact_id" in context:
        args["id"] = context["contact_id"]

    # Note: allocations only needs tenant_id filtering, which is already handled above

    logger.info(f"Raw JSON execution with context filters: {args}")

    try:
        if is_find_one:
            # Execute find_one
            result = await db.find_one_raw_json(view_name, field_name, mock_info, **args)
        else:
            # Execute find
            result = await db.find_raw_json(view_name, field_name, mock_info, **args)

        logger.debug(f"Successfully executed query '{field_name}' via raw JSON")
        return result

    except Exception as e:
        logger.error(f"Failed to execute raw JSON query: {e}")
        return None


def _is_simple_db_query(fn) -> bool:
    """Check if a function is a simple database query."""
    try:
        import inspect

        source = inspect.getsource(fn)
        logger.info(f"Checking resolver source (first 300 chars): {source[:300]}...")

        # Check for simple patterns
        has_db_query = "db.find_one(" in source or "db.find(" in source
        logger.info(f"Has db query: {has_db_query}")

        if has_db_query:
            # Check for complex patterns
            complex_patterns = ["for ", "if ", "filter(", "map(", "lambda", "await (?!db.find)"]
            found_patterns = [pattern for pattern in complex_patterns if pattern in source]
            logger.info(f"Found complex patterns: {found_patterns}")

            if found_patterns:
                logger.info("Resolver rejected: contains complex patterns")
                return False
            return True
    except Exception as e:
        logger.info(f"Error checking resolver: {e}")

    return False


def _extract_view_name(fn) -> Optional[str]:
    """Extract the view name from a resolver function."""
    try:
        import inspect

        source = inspect.getsource(fn)

        # Look for db.find or db.find_one calls with various quote styles
        # Matches: db.find("view_name"), db.find('view_name'), db.find_one("view_name"), etc.
        patterns = [
            r'db\.find(?:_one)?\s*\(\s*["\']([^"\']+)["\']',  # quoted strings
            r'await\s+db\.find(?:_one)?\s*\(\s*["\']([^"\']+)["\']',  # async with await
            # return await pattern
            r'return\s+await\s+db\.find(?:_one)?\s*\(\s*["\']([^"\']+)["\']',
        ]

        for pattern in patterns:
            match = re.search(pattern, source)
            if match:
                view_name = match.group(1)
                logger.info(
                    f"Extracted view name '{view_name}' from resolver using pattern: {pattern}"
                )
                return view_name

        logger.debug(f"Could not extract view name from source: {source[:200]}...")
    except Exception as e:
        logger.debug(f"Error extracting view name: {e}")

    return None


def _get_literal_value(value_node) -> Any:
    """Extract literal value from GraphQL AST node."""
    if hasattr(value_node, "value"):
        return value_node.value
    # Handle other types as needed
    return None


async def try_raw_json_execution_enhanced(
    schema: GraphQLSchema,
    query: str,
    variables: dict[str, Any] | None,
    context: dict[str, Any],
    analyzer: "QueryAnalyzer",
) -> Optional[RawJSONResult]:
    """Enhanced raw JSON execution with comprehensive detection.

    Args:
        schema: GraphQL schema
        query: GraphQL query string
        variables: Query variables
        context: Request context
        analyzer: Query analyzer instance

    Returns:
        RawJSONResult if executed via raw JSON, None otherwise
    """
    # Only in production mode
    if context.get("mode") != "production":
        return None

    # Check if we have a database with raw JSON support
    db = context.get("db")
    if not db or not hasattr(db, "find_raw_json"):
        return None

    # Analyze query for passthrough eligibility
    analysis = analyzer.analyze_for_passthrough(query, variables)

    if not analysis.eligible:
        logger.debug(
            "Query not eligible for raw JSON passthrough",
            reason=analysis.reason,
            complexity=analysis.complexity_score,
        )
        return None

    # Build SQL from analysis
    try:
        sql = build_passthrough_sql(
            analysis.view_name, analysis.field_paths, analysis.where_conditions
        )

        # Execute raw SQL
        result = await db.execute_raw_json(sql, analysis.where_conditions)

        logger.info(
            "Successfully executed query via raw JSON passthrough",
            view=analysis.view_name,
            complexity=analysis.complexity_score,
        )

        return result

    except Exception as e:
        logger.error("Failed to execute raw JSON query", error=str(e), view=analysis.view_name)
        return None


def build_passthrough_sql(
    view_name: str, field_paths: Dict[str, str], where_conditions: Dict[str, Any]
) -> str:
    """Build SQL for JSON passthrough execution.

    Args:
        view_name: Database view name
        field_paths: Mapping of GraphQL fields to JSONB paths
        where_conditions: WHERE clause conditions

    Returns:
        SQL query string
    """
    # Build field selection
    if field_paths:
        # Build JSONB object with selected fields
        field_parts = []
        for graphql_field, jsonb_path in field_paths.items():
            field_parts.append(f"'{graphql_field}', {jsonb_path}")

        fields = f"jsonb_build_object({', '.join(field_parts)})"
    else:
        # Select all fields
        fields = "to_jsonb(t.*)"

    # Check if we're selecting a single record or multiple
    is_single = any(key in where_conditions for key in ["id", "uuid", "pk"])

    if is_single:
        # Single object
        sql = f"""
        SELECT {fields}::text as result
        FROM {view_name} t
        WHERE {build_where_clause(where_conditions)}
        LIMIT 1
        """
    else:
        # Array of objects
        sql = f"""
        SELECT COALESCE(
            json_agg({fields} ORDER BY t.id),
            '[]'::json
        )::text as result
        FROM {view_name} t
        WHERE {build_where_clause(where_conditions)}
        """

    # Add limit if specified
    if "limit" in where_conditions:
        sql += " LIMIT %(limit)s"

    # Add offset if specified
    if "offset" in where_conditions:
        sql += " OFFSET %(offset)s"

    return sql


def build_where_clause(conditions: Dict[str, Any]) -> str:
    """Build WHERE clause from conditions.

    Args:
        conditions: Dictionary of conditions

    Returns:
        WHERE clause string
    """
    if not conditions:
        return "1=1"

    clauses = []

    for field, value in conditions.items():
        # Skip non-filter fields
        if field in ["limit", "offset", "orderBy"]:
            continue

        # Handle different condition types
        if isinstance(value, dict):
            # Complex condition (e.g., {gte: 10, lte: 20})
            for op in value:
                sql_op = map_graphql_op_to_sql(op)
                clauses.append(f"{field} {sql_op} %({field}_{op})s")
        else:
            # Simple equality
            clauses.append(f"{field} = %({field})s")

    return " AND ".join(clauses) if clauses else "1=1"


def map_graphql_op_to_sql(op: str) -> str:
    """Map GraphQL operator to SQL operator.

    Args:
        op: GraphQL operator

    Returns:
        SQL operator
    """
    mapping = {
        "eq": "=",
        "ne": "!=",
        "gt": ">",
        "gte": ">=",
        "lt": "<",
        "lte": "<=",
        "in": "IN",
        "contains": "LIKE",
        "startsWith": "LIKE",
        "endsWith": "LIKE",
    }

    return mapping.get(op, "=")
