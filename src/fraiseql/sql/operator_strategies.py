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

    def _apply_type_cast(self, path_sql: SQL, val: Any, op: str) -> SQL:
        """Apply appropriate type casting to the JSONB path."""
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
        path_sql = self._apply_type_cast(path_sql, val, op)
        sql_op = self.operator_map[op]
        return Composed([path_sql, SQL(sql_op), Literal(val)])


class JsonOperatorStrategy(BaseOperatorStrategy):
    """Strategy for JSONB-specific operators."""

    def __init__(self) -> None:
        super().__init__(["contains", "overlaps", "strictly_contains"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for JSONB operators."""
        if op == "contains":
            return Composed([path_sql, SQL(" @> "), Literal(val)])
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
        super().__init__(["matches", "startswith"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build SQL for pattern matching."""
        if op == "matches":
            return Composed([path_sql, SQL(" ~ "), Literal(val)])
        if op == "startswith":
            if isinstance(val, str):
                # Use LIKE for better performance
                return Composed([path_sql, SQL(" LIKE "), Literal(val + "%")])
            return Composed([path_sql, SQL(" ~ "), Literal(str(val) + ".*")])
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

        # Check if we need numeric casting
        if val and all(isinstance(v, (int, float, Decimal)) for v in val):
            path_sql = Composed([path_sql, SQL("::numeric")])
            literals = [Literal(v) for v in val]
        else:
            # Convert booleans to strings for JSONB text comparison
            converted_vals = [str(v).lower() if isinstance(v, bool) else v for v in val]
            literals = [Literal(v) for v in converted_vals]

        # Build the IN/NOT IN clause
        parts = [path_sql]
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


class OperatorRegistry:
    """Registry for operator strategies."""

    def __init__(self) -> None:
        """Initialize the registry with all available strategies."""
        self.strategies: list[OperatorStrategy] = [
            NullOperatorStrategy(),
            ComparisonOperatorStrategy(),
            JsonOperatorStrategy(),
            PatternMatchingStrategy(),
            ListOperatorStrategy(),
            PathOperatorStrategy(),
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
