"""MAC address operator strategies."""

from typing import Any, Optional

from psycopg.sql import Composable

from fraiseql.sql.operators.base import BaseOperatorStrategy


class MacAddressOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL macaddr/macaddr8 operators.

    Supports:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "in", "nin", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a MAC address operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if "MacAddr" in type_name or "macaddr" in type_name.lower():
                return True

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for MAC address operators."""
        # Comparison operators
        if operator in ("eq", "neq"):
            casted_path = self._cast_path(path_sql, "macaddr", jsonb_column, use_postgres_cast=True)
            return self._build_comparison(operator, casted_path, str(value))

        # List operators
        if operator == "in":
            casted_path = self._cast_path(path_sql, "macaddr", jsonb_column, use_postgres_cast=True)
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                cast_values="macaddr",
            )

        if operator == "nin":
            casted_path = self._cast_path(path_sql, "macaddr", jsonb_column, use_postgres_cast=True)
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                negate=True,
                cast_values="macaddr",
            )

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
