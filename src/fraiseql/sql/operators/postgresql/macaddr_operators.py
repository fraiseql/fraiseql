"""MAC address operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable

from fraiseql.sql.operators.base import BaseOperatorStrategy


class MacAddressOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL macaddr/macaddr8 operators.

    Supports:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "in", "nin", "notin", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a MAC address operator."""
        # Only support operators for MAC address fields
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if (
                "MacAddr" in type_name
                or "macaddr" in type_name.lower()
                or "MacAddress" in type_name
            ):
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
        """Build SQL for MAC address operators with proper casting.

        Always casts both field and value to ::macaddr for type-safe comparisons.

        Supported operators:
            - eq, neq: Equality/inequality with ::macaddr casting
            - in, nin: List membership with ::macaddr casting
            - isnull: NULL checking (no casting needed)
        """
        # Validate that pattern operators are not used with MAC addresses
        pattern_operators = {"contains", "startswith", "endswith"}
        if operator in pattern_operators and field_type and hasattr(field_type, "__name__"):
            type_name = field_type.__name__.lower()
            if "mac" in type_name or "macaddr" in type_name:
                raise ValueError(
                    f"Pattern operator '{operator}' is not supported for MAC address fields. "
                    "MAC addresses only support equality, list, and null operators."
                )

        # Comparison operators (eq, neq)
        if operator == "eq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "macaddr")
            return SQL("{} = {}").format(casted_path, casted_value)

        if operator == "neq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "macaddr")
            return SQL("{} != {}").format(casted_path, casted_value)

        # List operators (in, nin)
        if operator == "in":
            # Cast field path
            casted_path = SQL("({})::{}").format(path_sql, SQL("macaddr"))

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values([str(v) for v in value_list], "macaddr")

            # Build IN clause: field IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} IN ({})").format(casted_path, values_sql)

        if operator in ("nin", "notin"):
            # Cast field path
            casted_path = SQL("({})::{}").format(path_sql, SQL("macaddr"))

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values([str(v) for v in value_list], "macaddr")

            # Build NOT IN clause: field NOT IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} NOT IN ({})").format(casted_path, values_sql)

        # NULL checking (no casting needed)
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
