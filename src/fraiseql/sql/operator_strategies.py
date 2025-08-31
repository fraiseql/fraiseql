"""Operator strategies for SQL WHERE clause generation.

This module implements the strategy pattern for different SQL operators,
making the where clause generation more maintainable and extensible.
"""

from abc import ABC, abstractmethod
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Protocol

from psycopg.sql import SQL, Composed, Literal


class OperatorStrategy(Protocol):
    """Protocol for operator strategies."""

    def can_handle(self, op: str) -> bool:
        """Check if this strategy can handle the given operator."""
        ...

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build the SQL for this operator."""
        ...


class BaseOperatorStrategy(ABC):
    """Base class for operator strategies with common functionality."""

    def __init__(self, operators: list[str]) -> None:
        """Initialize with the list of operators this strategy handles."""
        self.operators = operators

    def can_handle(self, op: str) -> bool:
        """Check if this strategy can handle the given operator."""
        return op in self.operators

    @abstractmethod
    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build the SQL for this operator."""

    def _apply_type_cast(
        self, path_sql: SQL, val: Any, op: str, field_type: type | None = None
    ) -> SQL | Composed:
        """Apply appropriate type casting to the JSONB path."""
        # Handle IP address types specially
        if (
            field_type
            and self._is_ip_address_type(field_type)
            and op in ("eq", "neq", "contains", "startswith", "endswith", "in", "notin")
        ):
            # For IP addresses, use host() to strip CIDR notation
            return Composed([SQL("host("), path_sql, SQL("::inet)")])

        # Handle booleans first
        if isinstance(val, bool):
            return Composed([path_sql, SQL("::boolean")])

        # For comparison operators, apply type casting
        if op in ("gt", "gte", "lt", "lte") or (op in ("eq", "neq") and not isinstance(val, str)):
            if isinstance(val, (int, float, Decimal)):
                return Composed([path_sql, SQL("::numeric")])
            if isinstance(val, datetime):
                return Composed([path_sql, SQL("::timestamp")])
            if isinstance(val, date):
                return Composed([path_sql, SQL("::date")])

        return path_sql

    def _is_ip_address_type(self, field_type: type) -> bool:
        """Check if field_type is an IP address type."""
        # Import here to avoid circular imports
        try:
            from fraiseql.types.scalars.ip_address import IpAddressField

            return field_type == IpAddressField or (
                isinstance(field_type, type) and issubclass(field_type, IpAddressField)
            )
        except ImportError:
            return False


class NullOperatorStrategy(BaseOperatorStrategy):
    """Strategy for null/not null operators."""

    def __init__(self) -> None:
        super().__init__(["isnull"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for null checks."""
        if val:
            return Composed([path_sql, SQL(" IS NULL")])
        return Composed([path_sql, SQL(" IS NOT NULL")])


class ComparisonOperatorStrategy(BaseOperatorStrategy):
    """Strategy for comparison operators (=, !=, <, >, <=, >=)."""

    def __init__(self) -> None:
        super().__init__(["eq", "neq", "gt", "gte", "lt", "lte"])
        self.operator_map = {
            "eq": " = ",
            "neq": " != ",
            "gt": " > ",
            "gte": " >= ",
            "lt": " < ",
            "lte": " <= ",
        }

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for comparison operators."""
        path_sql = self._apply_type_cast(path_sql, val, op, field_type)
        sql_op = self.operator_map[op]
        return Composed([path_sql, SQL(sql_op), Literal(val)])


class JsonOperatorStrategy(BaseOperatorStrategy):
    """Strategy for JSONB-specific operators."""

    def __init__(self) -> None:
        super().__init__(["overlaps", "strictly_contains"])  # Removed "contains"

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for JSONB operators."""
        if op == "overlaps":
            return Composed([path_sql, SQL(" && "), Literal(val)])
        if op == "strictly_contains":
            return Composed(
                [
                    path_sql,
                    SQL(" @> "),
                    Literal(val),
                    SQL(" AND "),
                    path_sql,
                    SQL(" != "),
                    Literal(val),
                ],
            )
        raise ValueError(f"Unsupported JSON operator: {op}")


class PatternMatchingStrategy(BaseOperatorStrategy):
    """Strategy for pattern matching operators."""

    def __init__(self) -> None:
        super().__init__(["matches", "startswith", "contains", "endswith"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for pattern matching."""
        # Apply type-specific casting (including IP address handling)
        casted_path = self._apply_type_cast(path_sql, val, op, field_type)

        if op == "matches":
            return Composed([casted_path, SQL(" ~ "), Literal(val)])
        if op == "startswith":
            if isinstance(val, str):
                # Use LIKE for better performance
                return Composed([casted_path, SQL(" LIKE "), Literal(val + "%")])
            return Composed([casted_path, SQL(" ~ "), Literal(str(val) + ".*")])
        if op == "contains":
            if isinstance(val, str):
                # Use LIKE for substring matching
                # Note: % is already properly handled by psycopg's Literal
                like_val = f"%{val}%"
                return Composed([casted_path, SQL(" LIKE "), Literal(like_val)])
            return Composed([casted_path, SQL(" ~ "), Literal(f".*{val}.*")])
        if op == "endswith":
            if isinstance(val, str):
                # Use LIKE for suffix matching
                like_val = f"%{val}"
                return Composed([casted_path, SQL(" LIKE "), Literal(like_val)])
            return Composed([casted_path, SQL(" ~ "), Literal(f".*{val}$")])
        raise ValueError(f"Unsupported pattern operator: {op}")


class ListOperatorStrategy(BaseOperatorStrategy):
    """Strategy for list-based operators (IN, NOT IN)."""

    def __init__(self) -> None:
        super().__init__(["in", "notin"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for list operators."""
        if not isinstance(val, list):
            msg = f"'{op}' operator requires a list, got {type(val)}"
            raise TypeError(msg)

        # Apply type-specific casting (including IP address handling)
        casted_path = self._apply_type_cast(path_sql, val[0] if val else None, op, field_type)

        # Check if we need numeric casting (but not for IP addresses)
        if not (field_type and self._is_ip_address_type(field_type)):
            if val and all(isinstance(v, (int, float, Decimal)) for v in val):
                casted_path = Composed([casted_path, SQL("::numeric")])
                literals = [Literal(v) for v in val]
            else:
                # Convert booleans to strings for JSONB text comparison
                converted_vals = [str(v).lower() if isinstance(v, bool) else v for v in val]
                literals = [Literal(v) for v in converted_vals]
        else:
            # For IP addresses, use string literals
            literals = [Literal(str(v)) for v in val]

        # Build the IN/NOT IN clause
        parts = [casted_path]
        parts.append(SQL(" IN (" if op == "in" else " NOT IN ("))

        for i, lit in enumerate(literals):
            if i > 0:
                parts.append(SQL(", "))
            parts.append(lit)

        parts.append(SQL(")"))
        return Composed(parts)


class PathOperatorStrategy(BaseOperatorStrategy):
    """Strategy for path/tree operators."""

    def __init__(self) -> None:
        super().__init__(["depth_eq", "depth_gt", "depth_lt", "isdescendant"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for path operators."""
        if op == "depth_eq":
            return Composed([SQL("nlevel("), path_sql, SQL(") = "), Literal(val)])
        if op == "depth_gt":
            return Composed([SQL("nlevel("), path_sql, SQL(") > "), Literal(val)])
        if op == "depth_lt":
            return Composed([SQL("nlevel("), path_sql, SQL(") < "), Literal(val)])
        if op == "isdescendant":
            return Composed([path_sql, SQL(" <@ "), Literal(val)])
        raise ValueError(f"Unsupported path operator: {op}")


class DateRangeOperatorStrategy(BaseOperatorStrategy):
    """Strategy for DateRange operators with PostgreSQL daterange type casting."""

    def __init__(self) -> None:
        # Include range operators and basic operations, restrict problematic patterns
        super().__init__(
            [
                "eq",
                "neq",
                "in",
                "notin",  # Basic operations
                "contains_date",  # Range contains date (@>)
                "overlaps",  # Ranges overlap (&&) - handled by existing JsonOperatorStrategy
                "adjacent",  # Ranges are adjacent (-|-)
                "strictly_left",  # Range is strictly left (<<)
                "strictly_right",  # Range is strictly right (>>)
                "not_left",  # Range does not extend left (&>)
                "not_right",  # Range does not extend right (&<)
                "contains",
                "startswith",
                "endswith",  # Generic patterns (to restrict)
            ]
        )

    def can_handle(self, op: str, field_type: type | None = None) -> bool:
        """Check if this strategy can handle the given operator.

        DateRange operators should only be used with DateRange field types.
        For DateRange types, we handle ALL operators to properly restrict unsupported ones.
        """
        # If no field type provided, we can't determine if this is appropriate
        if field_type is None:
            return False

        # For DateRange types, handle ALL the operators we're configured for
        # This ensures we can properly restrict the problematic ones
        return self._is_daterange_type(field_type) and op in self.operators

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for DateRange operators with proper daterange casting."""
        # For basic operations, cast both sides to daterange for proper PostgreSQL handling
        if op in ("eq", "neq", "in", "notin"):
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])

            if op == "eq":
                return Composed([casted_path, SQL(" = "), Literal(val), SQL("::daterange")])
            if op == "neq":
                return Composed([casted_path, SQL(" != "), Literal(val), SQL("::daterange")])
            if op == "in":
                if not isinstance(val, list):
                    msg = f"'in' operator requires a list, got {type(val)}"
                    raise TypeError(msg)

                parts = [casted_path, SQL(" IN (")]
                for i, range_val in enumerate(val):
                    if i > 0:
                        parts.append(SQL(", "))
                    parts.extend([Literal(range_val), SQL("::daterange")])
                parts.append(SQL(")"))
                return Composed(parts)
            if op == "notin":
                if not isinstance(val, list):
                    msg = f"'notin' operator requires a list, got {type(val)}"
                    raise TypeError(msg)

                parts = [casted_path, SQL(" NOT IN (")]
                for i, range_val in enumerate(val):
                    if i > 0:
                        parts.append(SQL(", "))
                    parts.extend([Literal(range_val), SQL("::daterange")])
                parts.append(SQL(")"))
                return Composed(parts)

        # For range-specific operators
        elif op == "contains_date":
            # range @> date - range contains date
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" @> "), Literal(val), SQL("::date")])

        elif op == "overlaps":
            # range1 && range2 - ranges overlap
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" && "), Literal(val), SQL("::daterange")])

        elif op == "adjacent":
            # range1 -|- range2 - ranges are adjacent
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" -|- "), Literal(val), SQL("::daterange")])

        elif op == "strictly_left":
            # range1 << range2 - range1 is strictly left of range2
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" << "), Literal(val), SQL("::daterange")])

        elif op == "strictly_right":
            # range1 >> range2 - range1 is strictly right of range2
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" >> "), Literal(val), SQL("::daterange")])

        elif op == "not_left":
            # range1 &> range2 - range1 does not extend left of range2
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" &> "), Literal(val), SQL("::daterange")])

        elif op == "not_right":
            # range1 &< range2 - range1 does not extend right of range2
            casted_path = Composed([SQL("("), path_sql, SQL(")::daterange")])
            return Composed([casted_path, SQL(" &< "), Literal(val), SQL("::daterange")])

        # For pattern operators (contains, startswith, endswith), explicitly reject them
        elif op in ("contains", "startswith", "endswith"):
            raise ValueError(
                f"Pattern operator '{op}' is not supported for DateRange fields. "
                f"Use range operators: contains_date, overlaps, adjacent, strictly_left, "
                f"strictly_right, not_left, not_right, or basic: eq, neq, in, notin, isnull."
            )

        raise ValueError(f"Unsupported DateRange operator: {op}")

    def _is_daterange_type(self, field_type: type) -> bool:
        """Check if field_type is a DateRange type."""
        # Import here to avoid circular imports
        try:
            from fraiseql.types.scalars.daterange import DateRangeField

            return field_type == DateRangeField or (
                isinstance(field_type, type) and issubclass(field_type, DateRangeField)
            )
        except ImportError:
            return False


class LTreeOperatorStrategy(BaseOperatorStrategy):
    """Strategy for LTree hierarchical path operators with PostgreSQL ltree type casting."""

    def __init__(self) -> None:
        # Include hierarchical operators and basic operations, restrict problematic patterns
        super().__init__(
            [
                "eq",
                "neq",
                "in",
                "notin",  # Basic operations
                "ancestor_of",
                "descendant_of",  # Hierarchical relationships
                "matches_lquery",
                "matches_ltxtquery",  # Pattern matching
                "contains",
                "startswith",
                "endswith",  # Generic patterns (to restrict)
            ]
        )

    def can_handle(self, op: str, field_type: type | None = None) -> bool:
        """Check if this strategy can handle the given operator.

        LTree operators should only be used with LTree field types.
        For LTree types, we handle ALL operators to properly restrict unsupported ones.
        """
        # If no field type provided, we can't determine if this is appropriate
        if field_type is None:
            return False

        # For LTree types, handle ALL the operators we're configured for
        # This ensures we can properly restrict the problematic ones
        return self._is_ltree_type(field_type) and op in self.operators

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for LTree operators with proper ltree casting."""
        # For basic operations, cast both sides to ltree for proper PostgreSQL handling
        if op in ("eq", "neq", "in", "notin"):
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])

            if op == "eq":
                return Composed([casted_path, SQL(" = "), Literal(val), SQL("::ltree")])
            if op == "neq":
                return Composed([casted_path, SQL(" != "), Literal(val), SQL("::ltree")])
            if op == "in":
                if not isinstance(val, list):
                    msg = f"'in' operator requires a list, got {type(val)}"
                    raise TypeError(msg)

                parts = [casted_path, SQL(" IN (")]
                for i, path in enumerate(val):
                    if i > 0:
                        parts.append(SQL(", "))
                    parts.extend([Literal(path), SQL("::ltree")])
                parts.append(SQL(")"))
                return Composed(parts)
            if op == "notin":
                if not isinstance(val, list):
                    msg = f"'notin' operator requires a list, got {type(val)}"
                    raise TypeError(msg)

                parts = [casted_path, SQL(" NOT IN (")]
                for i, path in enumerate(val):
                    if i > 0:
                        parts.append(SQL(", "))
                    parts.extend([Literal(path), SQL("::ltree")])
                parts.append(SQL(")"))
                return Composed(parts)

        # For hierarchical operators, use proper ltree operators
        elif op == "ancestor_of":
            # path1 @> path2 means path1 is ancestor of path2
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            return Composed([casted_path, SQL(" @> "), Literal(val), SQL("::ltree")])

        elif op == "descendant_of":
            # path1 <@ path2 means path1 is descendant of path2
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            return Composed([casted_path, SQL(" <@ "), Literal(val), SQL("::ltree")])

        elif op == "matches_lquery":
            # path ~ lquery means path matches lquery pattern
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            return Composed([casted_path, SQL(" ~ "), Literal(val), SQL("::lquery")])

        elif op == "matches_ltxtquery":
            # path ? ltxtquery means path matches ltxtquery text query
            casted_path = Composed([SQL("("), path_sql, SQL(")::ltree")])
            return Composed([casted_path, SQL(" ? "), Literal(val), SQL("::ltxtquery")])

        # For pattern operators (contains, startswith, endswith), explicitly reject them
        elif op in ("contains", "startswith", "endswith"):
            raise ValueError(
                f"Pattern operator '{op}' is not supported for LTree fields. "
                f"Use hierarchical operators: ancestor_of, descendant_of, matches_lquery, "
                f"matches_ltxtquery, or basic: eq, neq, in, notin, isnull."
            )

        raise ValueError(f"Unsupported LTree operator: {op}")

    def _is_ltree_type(self, field_type: type) -> bool:
        """Check if field_type is an LTree type."""
        # Import here to avoid circular imports
        try:
            from fraiseql.types import LTree
            from fraiseql.types.scalars.ltree import LTreeField

            return field_type in (LTree, LTreeField) or (
                isinstance(field_type, type) and issubclass(field_type, LTreeField)
            )
        except ImportError:
            return False


class MacAddressOperatorStrategy(BaseOperatorStrategy):
    """Strategy for MAC address-specific operators with PostgreSQL macaddr type casting."""

    def __init__(self) -> None:
        # Include ALL operators to properly restrict unsupported ones
        super().__init__(["eq", "neq", "in", "notin", "contains", "startswith", "endswith"])

    def can_handle(self, op: str, field_type: type | None = None) -> bool:
        """Check if this strategy can handle the given operator.

        MAC address operators should only be used with MAC address field types.
        For MAC address types, we handle ALL operators to properly restrict unsupported ones.
        """
        # If no field type provided, we can't determine if this is appropriate
        if field_type is None:
            return False

        # For MAC address types, handle ALL the operators we're configured for
        # This ensures we can properly restrict the problematic ones
        return self._is_mac_address_type(field_type) and op in self.operators

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for MAC address operators with proper macaddr casting."""
        # For supported operators, cast the JSONB field to macaddr for proper PostgreSQL handling
        if op in ("eq", "neq", "in", "notin"):
            casted_path = Composed([SQL("("), path_sql, SQL(")::macaddr")])

            if op == "eq":
                return Composed([casted_path, SQL(" = "), Literal(val), SQL("::macaddr")])
            if op == "neq":
                return Composed([casted_path, SQL(" != "), Literal(val), SQL("::macaddr")])
            if op == "in":
                if not isinstance(val, list):
                    msg = f"'in' operator requires a list, got {type(val)}"
                    raise TypeError(msg)

                parts = [casted_path, SQL(" IN (")]
                for i, mac in enumerate(val):
                    if i > 0:
                        parts.append(SQL(", "))
                    parts.extend([Literal(mac), SQL("::macaddr")])
                parts.append(SQL(")"))
                return Composed(parts)
            if op == "notin":
                if not isinstance(val, list):
                    msg = f"'notin' operator requires a list, got {type(val)}"
                    raise TypeError(msg)

                parts = [casted_path, SQL(" NOT IN (")]
                for i, mac in enumerate(val):
                    if i > 0:
                        parts.append(SQL(", "))
                    parts.extend([Literal(mac), SQL("::macaddr")])
                parts.append(SQL(")"))
                return Composed(parts)

        # For pattern operators (contains, startswith, endswith), explicitly reject them
        elif op in ("contains", "startswith", "endswith"):
            raise ValueError(
                f"Pattern operator '{op}' is not supported for MAC address fields. "
                f"Use only: eq, neq, in, notin, isnull for MAC address filtering."
            )

        raise ValueError(f"Unsupported MAC address operator: {op}")

    def _is_mac_address_type(self, field_type: type) -> bool:
        """Check if field_type is a MAC address type."""
        # Import here to avoid circular imports
        try:
            from fraiseql.types import MacAddress
            from fraiseql.types.scalars.mac_address import MacAddressField

            return field_type in (MacAddress, MacAddressField) or (
                isinstance(field_type, type) and issubclass(field_type, MacAddressField)
            )
        except ImportError:
            return False


class NetworkOperatorStrategy(BaseOperatorStrategy):
    """Strategy for network-specific operators (v0.3.8+)."""

    def __init__(self) -> None:
        super().__init__(["inSubnet", "inRange", "isPrivate", "isPublic", "isIPv4", "isIPv6"])

    def can_handle(self, op: str, field_type: type | None = None) -> bool:
        """Check if this strategy can handle the given operator.

        Network operators should only be used with IP address field types.
        """
        if op not in self.operators:
            return False

        # If no field type provided, we can't determine if this is appropriate
        # but we'll allow it for backward compatibility
        if field_type is None:
            return True

        # Only handle network operators for IP address types
        return self._is_ip_address_type(field_type)

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for network operators."""
        # Apply consistent type casting (same as ComparisonOperatorStrategy)
        # For IP addresses, we should use the same casting approach for consistency
        if field_type and self._is_ip_address_type(field_type):
            # Use direct ::inet casting for network operations (more appropriate than host())
            # Network operators work with full CIDR notation, so we don't strip with host()
            casted_path = Composed([SQL("("), path_sql, SQL(")::inet")])
        else:
            # Fallback to direct path for non-IP types
            casted_path = path_sql

        if op == "inSubnet":
            # PostgreSQL subnet matching using <<= operator
            return Composed([casted_path, SQL(" <<= "), Literal(val), SQL("::inet")])

        if op == "inRange":
            # IP range comparison
            if not isinstance(val, dict) or "from_" not in val or "to" not in val:
                # Try alternative field names
                if not isinstance(val, dict) or "from" not in val or "to" not in val:
                    raise ValueError(f"inRange requires dict with 'from' and 'to' keys, got {val}")
                from_ip = val["from"]
                to_ip = val["to"]
            else:
                from_ip = val["from_"]
                to_ip = val["to"]

            return Composed(
                [
                    casted_path,
                    SQL(" >= "),
                    Literal(from_ip),
                    SQL("::inet"),
                    SQL(" AND "),
                    casted_path,
                    SQL(" <= "),
                    Literal(to_ip),
                    SQL("::inet"),
                ]
            )

        if op == "isPrivate":
            # RFC 1918 private network ranges
            if val:
                return Composed(
                    [
                        SQL("("),
                        casted_path,
                        SQL(" <<= '10.0.0.0/8'::inet OR "),
                        casted_path,
                        SQL(" <<= '172.16.0.0/12'::inet OR "),
                        casted_path,
                        SQL(" <<= '192.168.0.0/16'::inet OR "),
                        casted_path,
                        SQL(" <<= '127.0.0.0/8'::inet OR "),
                        casted_path,
                        SQL(" <<= '169.254.0.0/16'::inet)"),
                    ]
                )
            return Composed(
                [
                    SQL("NOT ("),
                    casted_path,
                    SQL(" <<= '10.0.0.0/8'::inet OR "),
                    casted_path,
                    SQL(" <<= '172.16.0.0/12'::inet OR "),
                    casted_path,
                    SQL(" <<= '192.168.0.0/16'::inet OR "),
                    casted_path,
                    SQL(" <<= '127.0.0.0/8'::inet OR "),
                    casted_path,
                    SQL(" <<= '169.254.0.0/16'::inet)"),
                ]
            )

        if op == "isPublic":
            # Invert private logic
            if val:
                # Public means NOT private
                return Composed(
                    [
                        SQL("NOT ("),
                        casted_path,
                        SQL(" <<= '10.0.0.0/8'::inet OR "),
                        casted_path,
                        SQL(" <<= '172.16.0.0/12'::inet OR "),
                        casted_path,
                        SQL(" <<= '192.168.0.0/16'::inet OR "),
                        casted_path,
                        SQL(" <<= '127.0.0.0/8'::inet OR "),
                        casted_path,
                        SQL(" <<= '169.254.0.0/16'::inet)"),
                    ]
                )
            # NOT public means private
            return Composed(
                [
                    SQL("("),
                    casted_path,
                    SQL(" <<= '10.0.0.0/8'::inet OR "),
                    casted_path,
                    SQL(" <<= '172.16.0.0/12'::inet OR "),
                    casted_path,
                    SQL(" <<= '192.168.0.0/16'::inet OR "),
                    casted_path,
                    SQL(" <<= '127.0.0.0/8'::inet OR "),
                    casted_path,
                    SQL(" <<= '169.254.0.0/16'::inet)"),
                ]
            )

        if op == "isIPv4":
            # Check IP version using family() function
            if val:
                return Composed([SQL("family("), casted_path, SQL(") = 4")])
            return Composed([SQL("family("), casted_path, SQL(") != 4")])

        if op == "isIPv6":
            # Check IP version using family() function
            if val:
                return Composed([SQL("family("), casted_path, SQL(") = 6")])
            return Composed([SQL("family("), casted_path, SQL(") != 6")])

        raise ValueError(f"Unsupported network operator: {op}")


class OperatorRegistry:
    """Registry for operator strategies."""

    def __init__(self) -> None:
        """Initialize the registry with all available strategies."""
        self.strategies: list[OperatorStrategy] = [
            NullOperatorStrategy(),
            DateRangeOperatorStrategy(),  # Must come before ComparisonOperatorStrategy
            LTreeOperatorStrategy(),  # Must come before ComparisonOperatorStrategy
            MacAddressOperatorStrategy(),  # Must come before ComparisonOperatorStrategy
            NetworkOperatorStrategy(),  # Must come before ComparisonOperatorStrategy
            ComparisonOperatorStrategy(),
            PatternMatchingStrategy(),  # Move before JsonOperatorStrategy
            JsonOperatorStrategy(),
            ListOperatorStrategy(),
            PathOperatorStrategy(),
        ]

    def get_strategy(self, op: str, field_type: type | None = None) -> OperatorStrategy:
        """Get the appropriate strategy for an operator."""
        for strategy in self.strategies:
            # Try to pass field_type if the strategy supports it
            try:
                if hasattr(strategy, "can_handle"):
                    # Check if can_handle accepts field_type parameter
                    import inspect

                    sig = inspect.signature(strategy.can_handle)
                    if "field_type" in sig.parameters:
                        if strategy.can_handle(op, field_type):
                            return strategy
                    elif strategy.can_handle(op):
                        return strategy
            except Exception:
                # Fallback to basic can_handle
                if strategy.can_handle(op):
                    return strategy
        raise ValueError(f"Unsupported operator: {op}")

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for the given operator."""
        strategy = self.get_strategy(op, field_type)
        return strategy.build_sql(path_sql, op, val, field_type)


# Global registry instance
_operator_registry = OperatorRegistry()


def get_operator_registry() -> OperatorRegistry:
    """Get the global operator registry."""
    return _operator_registry
