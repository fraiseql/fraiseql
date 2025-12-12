"""Numeric operator strategies."""

from typing import Any, Optional

from psycopg.sql import Composable

from fraiseql.sql.operators.base import BaseOperatorStrategy


class NumericOperatorStrategy(BaseOperatorStrategy):
    """Strategy for numeric field operators (int, float, Decimal).

    Supports:
        - eq, neq: Equality/inequality
        - gt, gte, lt, lte: Comparison operators
        - in, nin: List membership
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "gt", "gte", "lt", "lte", "in", "nin", "isnull"}

    NUMERIC_TYPES = (int, float)

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a numeric operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is None:
            # Only handle if operator is clearly numeric-specific
            return operator in {"gt", "gte", "lt", "lte"}

        # Check for numeric types (exclude bool which has its own strategy)
        if field_type is bool:
            return False

        try:
            return issubclass(field_type, self.NUMERIC_TYPES)
        except TypeError:
            return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for numeric operators."""
        # Determine numeric cast type
        if jsonb_column:
            # JSONB numeric values need casting
            cast_type = "integer" if (field_type is int or isinstance(value, int)) else "numeric"
            casted_path = self._cast_path(path_sql, cast_type, jsonb_column, use_postgres_cast=True)
        else:
            casted_path = path_sql

        # Comparison operators
        comparison_sql = self._build_comparison(operator, casted_path, value)
        if comparison_sql is not None:
            return comparison_sql

        # List operators
        if operator == "in":
            return self._build_in_operator(casted_path, value)

        if operator == "nin":
            return self._build_in_operator(casted_path, value, negate=True)

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
