"""Decorator for hybrid table types with both regular columns and JSONB data."""

from typing import Callable, Optional, Type

from fraiseql.db import register_type_for_view


def hybrid_type(
    sql_source: str,
    regular_columns: Optional[set[str]] = None,
    has_jsonb_data: bool = True,
) -> Callable:
    """Decorator for types backed by hybrid tables.

    Hybrid tables have both regular SQL columns and JSONB data columns.
    This decorator registers the type with metadata to avoid runtime introspection.

    Example:
        @fraiseql.type
        @hybrid_type(
            sql_source="tv_allocation",
            regular_columns={'id', 'is_current', 'is_past', 'start_date'},
            has_jsonb_data=True
        )
        class Allocation:
            id: UUID
            is_current: bool
            machine_id: str  # From JSONB data

    Args:
        sql_source: The database table/view name
        regular_columns: Set of column names that exist as regular SQL columns
        has_jsonb_data: Whether the table has a JSONB 'data' column

    Performance:
        By providing column metadata at decoration time, we avoid expensive
        runtime queries to information_schema, making WHERE clause generation
        much faster.
    """

    def decorator(cls: Type) -> Type:
        # Store metadata on the class for introspection
        cls.__hybrid_metadata__ = {
            "sql_source": sql_source,
            "regular_columns": regular_columns or set(),
            "has_jsonb_data": has_jsonb_data,
        }

        # Auto-register with the repository when class is defined
        # This happens at import time, not query time
        if regular_columns or has_jsonb_data:
            register_type_for_view(
                sql_source, cls, table_columns=regular_columns, has_jsonb_data=has_jsonb_data
            )

        return cls

    return decorator
