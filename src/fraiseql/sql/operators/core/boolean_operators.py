"""Boolean operator strategies."""

from typing import Any, Optional

from psycopg.sql import Composable

from fraiseql.sql.operators.base import BaseOperatorStrategy


class BooleanOperatorStrategy(BaseOperatorStrategy):
    """Strategy for boolean field operators.

    Supports:
        - eq, neq: Equality/inequality
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a boolean operator on a boolean field."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is None:
            return False

        return field_type is bool

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for boolean operators."""
        # Comparison operators
        if operator in ("eq", "neq"):
            casted_path = self._cast_path(path_sql, "boolean", jsonb_column, use_postgres_cast=True)
            return self._build_comparison(operator, casted_path, bool(value))

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
