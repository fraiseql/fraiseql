"""Analytics decorators for FraiseQL fact tables and aggregate queries."""

from collections.abc import Callable
from types import FunctionType
from typing import TypeVar

from fraiseql.registry import SchemaRegistry
from fraiseql.types import extract_field_info

F = TypeVar("F", bound=FunctionType)
T = TypeVar("T")


def fact_table(
    *,
    table_name: str,
    measures: list[str],
    dimension_column: str = "data",
    dimension_paths: list[dict[str, str]] | None = None,
) -> Callable[[type[T]], type[T]]:
    """Decorator to mark a Python class as a fact table type.

    Fact tables are special analytics tables that follow FraiseQL's pattern:
    - Table name starts with `tf_` (e.g., "tf_sales")
    - Measures: SQL columns with numeric types (for aggregation)
    - Dimensions: JSONB column (for GROUP BY)
    - Denormalized filters: Indexed columns (for fast WHERE)

    Args:
        table_name: SQL table name (must start with "tf_")
        measures: List of field names that are measures (numeric columns)
        dimension_column: JSONB column name (default: "data")
        dimension_paths: Optional list of dimension paths with keys:
            - name: Path name (e.g., "category")
            - json_path: JSON path expression (e.g., "data->>'category'")
            - data_type: Data type hint (e.g., "text")

    Returns:
        The original class (unmodified)

    Examples:
        >>> @fraiseql.fact_table(
        ...     table_name="tf_sales",
        ...     measures=["revenue", "quantity", "cost"],
        ...     dimension_paths=[
        ...         {
        ...             "name": "category",
        ...             "json_path": "data->>'category'",
        ...             "data_type": "text"
        ...         },
        ...         {
        ...             "name": "product_name",
        ...             "json_path": "data->>'product_name'",
        ...             "data_type": "text"
        ...         }
        ...     ]
        ... )
        ... @fraiseql.type
        ... class Sale:
        ...     id: int
        ...     revenue: float
        ...     quantity: int
        ...     cost: float
        ...     customer_id: str
        ...     occurred_at: str

        This generates metadata:
        {
            "table_name": "tf_sales",
            "measures": [
                {"name": "revenue", "sql_type": "Float", "nullable": false},
                {"name": "quantity", "sql_type": "Int", "nullable": false},
                {"name": "cost", "sql_type": "Float", "nullable": false}
            ],
            "dimensions": {
                "name": "data",
                "paths": [
                    {
                        "name": "category",
                        "json_path": "data->>'category'",
                        "data_type": "text"
                    },
                    {
                        "name": "product_name",
                        "json_path": "data->>'product_name'",
                        "data_type": "text"
                    }
                ]
            },
            "denormalized_filters": [
                {"name": "customer_id", "sql_type": "Text", "indexed": true},
                {"name": "occurred_at", "sql_type": "Timestamp", "indexed": true}
            ]
        }

    Notes:
        - Table name must start with "tf_" prefix
        - Measures must be numeric types (int, float)
        - Dimension paths are optional (can be introspected at runtime)
        - This decorator should be combined with @fraiseql.type
    """
    if not table_name.startswith("tf_"):
        msg = f"Fact table name must start with 'tf_', got: {table_name}"
        raise ValueError(msg)

    def decorator(cls: type[T]) -> type[T]:
        # Extract field information from class annotations
        fields = extract_field_info(cls)

        # Separate measures from filters
        measure_fields = []
        filter_fields = []

        for measure_name in measures:
            if measure_name not in fields:
                msg = f"Measure '{measure_name}' not found in class fields"
                raise ValueError(msg)

            field = fields[measure_name]
            # Validate measure is numeric
            if field["type"] not in ("Int", "Float"):
                msg = f"Measure '{measure_name}' must be Int or Float, got: {field['type']}"
                raise ValueError(msg)

            measure_fields.append({
                "name": measure_name,
                "sql_type": field["type"],
                "nullable": field["nullable"],
            })

        # Non-measure fields are denormalized filters
        for field_name, field_info in fields.items():
            if field_name not in measures and field_name != "id":
                # Map GraphQL types to SQL types
                sql_type = field_info["type"]
                if sql_type == "String":
                    sql_type = "Text"
                elif sql_type == "ID":
                    sql_type = "Uuid"

                filter_fields.append({
                    "name": field_name,
                    "sql_type": sql_type,
                    "indexed": True,  # Assume all filters are indexed
                })

        # Build dimension metadata
        dimensions = {
            "name": dimension_column,
            "paths": dimension_paths or [],
        }

        # Register fact table with schema registry
        SchemaRegistry.register_fact_table(
            table_name=table_name,
            measures=measure_fields,
            dimensions=dimensions,
            denormalized_filters=filter_fields,
        )

        # Return original class unmodified
        return cls

    return decorator


def aggregate_query(
    *,
    fact_table: str,
    auto_group_by: bool = True,
    auto_aggregates: bool = True,
) -> Callable[[F], F]:
    """Decorator to mark a function as an aggregate query.

    Aggregate queries run GROUP BY operations on fact tables with:
    - GROUP BY: Dimensions and temporal buckets
    - SELECT: Aggregate functions (COUNT, SUM, AVG, etc.)
    - WHERE: Pre-aggregation filters
    - HAVING: Post-aggregation filters

    Args:
        fact_table: Fact table name (e.g., "tf_sales")
        auto_group_by: Automatically generate groupBy fields (default: True)
        auto_aggregates: Automatically generate aggregate fields (default: True)

    Returns:
        The original function (unmodified)

    Examples:
        >>> @fraiseql.aggregate_query(
        ...     fact_table="tf_sales",
        ...     auto_group_by=True,
        ...     auto_aggregates=True
        ... )
        ... @fraiseql.query
        ... def sales_aggregate() -> list[dict[str, Any]]:
        ...     '''Aggregate sales data with flexible grouping and aggregation.'''
        ...     pass

        This generates a query that accepts:
        - groupBy: { category: true, occurred_at_day: true }
        - aggregates: { count: true, revenue_sum: true, revenue_avg: true }
        - where: { customer_id: { _eq: "uuid-123" } }
        - having: { revenue_sum_gt: 1000 }
        - orderBy: [{ field: "revenue_sum", direction: DESC }]
        - limit: 100
        - offset: 0

    Notes:
        - Must be used with @fraiseql.query
        - Fact table must be registered with @fraiseql.fact_table
        - Return type should be list[dict[str, Any]] for flexibility
    """

    def decorator(f: F) -> F:
        # Extract function metadata
        function_name = f.__name__
        description = f.__doc__

        # Register aggregate query with schema registry
        SchemaRegistry.register_aggregate_query(
            name=function_name,
            fact_table=fact_table,
            auto_group_by=auto_group_by,
            auto_aggregates=auto_aggregates,
            description=description,
        )

        # Return original function unmodified
        return f

    return decorator
