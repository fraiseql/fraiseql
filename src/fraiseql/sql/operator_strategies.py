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


class NetworkOperatorStrategy(BaseOperatorStrategy):
    """Strategy for network-specific operators (v0.3.8+)."""

    def __init__(self) -> None:
        super().__init__(["inSubnet", "inRange", "isPrivate", "isPublic", "isIPv4", "isIPv6"])

    def can_handle(self, op: str) -> bool:
        """Check if this strategy can handle the given operator."""
        # Only handle network operators for IP address types
        return op in self.operators

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for network operators."""
        # Build SQL directly for network operators
        if op == "inSubnet":
            # PostgreSQL subnet matching using <<= operator
            return Composed([SQL("("), path_sql, SQL(")::inet <<= "), Literal(val), SQL("::inet")])

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
                    SQL("("),
                    path_sql,
                    SQL(")::inet >= "),
                    Literal(from_ip),
                    SQL("::inet"),
                    SQL(" AND ("),
                    path_sql,
                    SQL(")::inet <= "),
                    Literal(to_ip),
                    SQL("::inet"),
                ]
            )

        if op == "isPrivate":
            # RFC 1918 private network ranges
            if val:
                return Composed(
                    [
                        SQL("(("),
                        path_sql,
                        SQL(")::inet <<= '10.0.0.0/8'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '172.16.0.0/12'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '192.168.0.0/16'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '127.0.0.0/8'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '169.254.0.0/16'::inet)"),
                    ]
                )
            return Composed(
                [
                    SQL("NOT (("),
                    path_sql,
                    SQL(")::inet <<= '10.0.0.0/8'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '172.16.0.0/12'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '192.168.0.0/16'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '127.0.0.0/8'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '169.254.0.0/16'::inet)"),
                ]
            )

        if op == "isPublic":
            # Invert private logic
            if val:
                # Public means NOT private
                return Composed(
                    [
                        SQL("NOT (("),
                        path_sql,
                        SQL(")::inet <<= '10.0.0.0/8'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '172.16.0.0/12'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '192.168.0.0/16'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '127.0.0.0/8'::inet OR "),
                        SQL("("),
                        path_sql,
                        SQL(")::inet <<= '169.254.0.0/16'::inet)"),
                    ]
                )
            # NOT public means private
            return Composed(
                [
                    SQL("(("),
                    path_sql,
                    SQL(")::inet <<= '10.0.0.0/8'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '172.16.0.0/12'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '192.168.0.0/16'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '127.0.0.0/8'::inet OR "),
                    SQL("("),
                    path_sql,
                    SQL(")::inet <<= '169.254.0.0/16'::inet)"),
                ]
            )

        if op == "isIPv4":
            # Check IP version using family() function
            if val:
                return Composed([SQL("family(("), path_sql, SQL(")::inet) = 4")])
            return Composed([SQL("family(("), path_sql, SQL(")::inet) != 4")])

        if op == "isIPv6":
            # Check IP version using family() function
            if val:
                return Composed([SQL("family(("), path_sql, SQL(")::inet) = 6")])
            return Composed([SQL("family(("), path_sql, SQL(")::inet) != 6")])

        raise ValueError(f"Unsupported network operator: {op}")


class OperatorRegistry:
    """Registry for operator strategies."""

    def __init__(self) -> None:
        """Initialize the registry with all available strategies."""
        self.strategies: list[OperatorStrategy] = [
            NullOperatorStrategy(),
            ComparisonOperatorStrategy(),
            PatternMatchingStrategy(),  # Move before JsonOperatorStrategy
            JsonOperatorStrategy(),
            ListOperatorStrategy(),
            PathOperatorStrategy(),
            NetworkOperatorStrategy(),  # Network-specific operators (v0.3.8+)
        ]

    def get_strategy(self, op: str) -> OperatorStrategy:
        """Get the appropriate strategy for an operator."""
        for strategy in self.strategies:
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
        strategy = self.get_strategy(op)
        return strategy.build_sql(path_sql, op, val, field_type)


# Global registry instance
_operator_registry = OperatorRegistry()


def get_operator_registry() -> OperatorRegistry:
    """Get the global operator registry."""
    return _operator_registry
