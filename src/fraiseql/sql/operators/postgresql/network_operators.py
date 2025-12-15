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
        - inrange: IP in CIDR range (alias for insubnet)
        - isipv4: Check if IPv4 address
        - isipv6: Check if IPv6 address
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
        "inrange",
        "isipv4",
        "isipv6",
        "overlaps",
        "strictleft",
        "strictright",
        "isnull",
        # CamelCase versions used by tests
        "isPrivate",
        "isPublic",
        "inSubnet",
        "inRange",
        "isIPv4",
        "isIPv6",
    }

    NETWORK_TYPES = {"IPv4Address", "IPv6Address", "IPv4Network", "IPv6Network", "IpAddress"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a network operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Network-specific operators - support even without field_type
        # The operator name itself is a strong signal this is a network operation
        if operator in {
            "isprivate",
            "ispublic",
            "insubnet",
            "inrange",
            "isipv4",
            "isipv6",
            "overlaps",
            "strictleft",
            "strictright",
            # CamelCase versions
            "isPrivate",
            "isPublic",
            "inSubnet",
            "inRange",
            "isIPv4",
            "isIPv6",
        }:
            # Accept these operators even without field_type
            # If field_type is provided, verify it's a network type
            if field_type is not None:
                type_name = (
                    field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
                )
                # Only accept if it's actually a network type
                if not any(net_type in type_name for net_type in self.NETWORK_TYPES):
                    return False
            return True

        # Generic operators (eq, neq, in, nin) - require field type verification
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if any(net_type in type_name for net_type in self.NETWORK_TYPES):
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
            # Private IP check using CIDR containment operators
            # Check if IP is in any private range (RFC 1918 + special use)
            casted_path = SQL("({})::inet").format(path_sql)
            return SQL(
                "({} << inet '10.0.0.0/8' OR "
                "{} << inet '172.16.0.0/12' OR "
                "{} << inet '192.168.0.0/16' OR "
                "{} << inet '127.0.0.0/8' OR "
                "{} << inet '169.254.0.0/16' OR "
                "{} << inet 'fc00::/7' OR "
                "{} << inet 'fe80::/10')"
            ).format(
                casted_path,
                casted_path,
                casted_path,
                casted_path,
                casted_path,
                casted_path,
                casted_path,
            )

        if operator in {"ispublic", "isPublic"}:
            # Public IP check: NOT in any private range
            casted_path = SQL("({})::inet").format(path_sql)
            return SQL(
                "NOT ({} << inet '10.0.0.0/8' OR "
                "{} << inet '172.16.0.0/12' OR "
                "{} << inet '192.168.0.0/16' OR "
                "{} << inet '127.0.0.0/8' OR "
                "{} << inet '169.254.0.0/16' OR "
                "{} << inet 'fc00::/7' OR "
                "{} << inet 'fe80::/10')"
            ).format(
                casted_path,
                casted_path,
                casted_path,
                casted_path,
                casted_path,
                casted_path,
                casted_path,
            )

        if operator in {"insubnet", "inSubnet"}:
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} <<= {}").format(casted_path, casted_value)

        if operator in {"inrange", "inRange"}:
            # inRange is an alias for inSubnet - check if IP is in CIDR range
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "inet")
            return SQL("{} <<= {}").format(casted_path, casted_value)

        if operator in {"isipv4", "isIPv4"}:
            casted_path = SQL("({})::inet").format(path_sql)
            return SQL("family({}) = 4").format(casted_path)

        if operator in {"isipv6", "isIPv6"}:
            casted_path = SQL("({})::inet").format(path_sql)
            return SQL("family({}) = 6").format(casted_path)

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
