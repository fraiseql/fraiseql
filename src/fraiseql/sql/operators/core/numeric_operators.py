"""Numeric operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

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

        # Check for numeric types
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
        # Cast to appropriate numeric type for JSONB
        if jsonb_column:
            # JSONB numeric values stored as text, need casting
            if field_type is int or isinstance(value, int):
                casted_path = SQL("({})::integer").format(path_sql)
            else:
                casted_path = SQL("({})::numeric").format(path_sql)
        else:
            casted_path = path_sql

        # Comparison operators
        if operator == "eq":
            return SQL("{} = {}").format(casted_path, Literal(value))

        if operator == "neq":
            return SQL("{} != {}").format(casted_path, Literal(value))

        if operator == "gt":
            return SQL("{} > {}").format(casted_path, Literal(value))

        if operator == "gte":
            return SQL("{} >= {}").format(casted_path, Literal(value))

        if operator == "lt":
            return SQL("{} < {}").format(casted_path, Literal(value))

        if operator == "lte":
            return SQL("{} <= {}").format(casted_path, Literal(value))

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(v) for v in value)
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(v) for v in value)
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
