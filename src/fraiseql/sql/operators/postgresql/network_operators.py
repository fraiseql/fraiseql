"""Network type operator strategies (INET, CIDR, IPv4, IPv6)."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable

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
        # CamelCase versions used by tests
        "isPrivate",
        "isPublic",
        "inSubnet",
    }

    NETWORK_TYPES = {"IPv4Address", "IPv6Address", "IPv4Network", "IPv6Network", "IpAddress"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a network operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Check field type for network fields
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if any(net_type in type_name for net_type in self.NETWORK_TYPES):
                return True

        # Network-specific operators only for network field types
        # (Don't claim them for other types like DateRange)
        if operator in {
            "isprivate",
            "ispublic",
            "insubnet",
            "overlaps",
            "strictleft",
            "strictright",
            # CamelCase versions
            "isPrivate",
            "isPublic",
            "inSubnet",
        }:
            # Only support these for network field types
            if field_type is not None:
                type_name = (
                    field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
                )
                if any(net_type in type_name for net_type in self.NETWORK_TYPES):
                    return True
            return False

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
        if operator == "eq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} = {}").format(casted_path, casted_value)

        if operator == "neq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} != {}").format(casted_path, casted_value)

        # List operators
        if operator == "in":
            # Cast field path
            casted_path = SQL("({})::inet").format(path_sql)

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values([str(v) for v in value_list], "inet")

            # Build IN clause: field IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} IN ({})").format(casted_path, values_sql)

        if operator == "nin":
            # Cast field path
            casted_path = SQL("({})::inet").format(path_sql)

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values([str(v) for v in value_list], "inet")

            # Build NOT IN clause: field NOT IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} NOT IN ({})").format(casted_path, values_sql)

        # Network-specific operators
        if operator in {"isprivate", "isPrivate"}:
            casted_path = SQL("({})::inet").format(path_sql)
            return SQL("NOT inet_public({})").format(casted_path)

        if operator in {"ispublic", "isPublic"}:
            casted_path = SQL("({})::inet").format(path_sql)
            return SQL("inet_public({})").format(casted_path)

        if operator in {"insubnet", "inSubnet"}:
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} <<= {}").format(casted_path, casted_value)

        if operator == "overlaps":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} && {}").format(casted_path, casted_value)

        if operator == "strictleft":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} << {}").format(casted_path, casted_value)

        if operator == "strictright":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} >> {}").format(casted_path, casted_value)

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
