"""Network type operator strategies (INET, CIDR, IPv4, IPv6)."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class NetworkOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL network type operators.

    Supports INET, CIDR types with operators:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - isprivate: Is private network
        - ispublic: Is public network
        - insubnet: Network contains address
        - overlaps: Networks overlap
        - strictleft, strictright: Ordering
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {
        "eq",
        "neq",
        "in",
        "nin",
        "isprivate",
        "ispublic",
        "insubnet",
        "overlaps",
        "strictleft",
        "strictright",
        "isnull",
    }

    NETWORK_TYPES = {"IPv4Address", "IPv6Address", "IPv4Network", "IPv6Network"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a network operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Check field type
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if any(net_type in type_name for net_type in self.NETWORK_TYPES):
                return True

        # Network-specific operators
        if operator in {
            "isprivate",
            "ispublic",
            "insubnet",
            "overlaps",
            "strictleft",
            "strictright",
        }:
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
        """Build SQL for network operators."""
        # Comparison operators
        if operator in ("eq", "neq"):
            casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
            return self._build_comparison(operator, casted_path, str(value))

        # List operators
        if operator == "in":
            casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                cast_values="inet",
            )

        if operator == "nin":
            casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                negate=True,
                cast_values="inet",
            )

        # Network-specific operators
        casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
        if operator == "isprivate":
            return SQL("NOT inet_public({})").format(casted_path)

        if operator == "ispublic":
            return SQL("inet_public({})").format(casted_path)

        if operator == "insubnet":
            return SQL("{} <<= {}::inet").format(casted_path, Literal(str(value)))

        if operator == "overlaps":
            return SQL("{} && {}::inet").format(casted_path, Literal(str(value)))

        if operator == "strictleft":
            return SQL("{} << {}::inet").format(casted_path, Literal(str(value)))

        if operator == "strictright":
            return SQL("{} >> {}::inet").format(casted_path, Literal(str(value)))

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
