"""Fallback list operator strategy."""

from decimal import Decimal
from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class ListOperatorStrategy(BaseOperatorStrategy):
    """Fallback strategy for list-based operators (IN, NOT IN).

    This strategy handles list operators that weren't caught by
    more specific strategies.

    Supports:
        - in: Value in list
        - notin: Value not in list
    """

    SUPPORTED_OPERATORS = {"in", "notin"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a list operator (fallback - always handles these)."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for list operators."""
        if not isinstance(value, list):
            raise TypeError(f"'{operator}' operator requires a list, got {type(value)}")

        # Apply type casting for JSONB fields based on first value type
        if jsonb_column and value:
            casted_path = self._apply_type_cast(path_sql, value[0])
        else:
            casted_path = path_sql

        # Handle value conversion based on type
        if value and all(isinstance(v, bool) for v in value):
            # For boolean lists, use text comparison with converted values
            literals = [Literal("true" if v else "false") for v in value]
        elif value and all(isinstance(v, (int, float, Decimal)) for v in value):
            # For numeric lists
            literals = [Literal(v) for v in value]
        else:
            # For other types (strings, etc.)
            literals = [Literal(v) for v in value]

        # Build the IN/NOT IN clause
        parts = [casted_path]
        if operator == "in":
            parts.append(SQL(" IN ("))
        else:  # notin
            parts.append(SQL(" NOT IN ("))

        for i, lit in enumerate(literals):
            if i > 0:
                parts.append(SQL(", "))
            parts.append(lit)

        parts.append(SQL(")"))
        return Composable(parts)

    def _apply_type_cast(self, path_sql: SQL, value: Any) -> Composable:
        """Apply appropriate type casting to the JSONB path based on value type."""
        # Check bool BEFORE int since bool is subclass of int in Python
        if isinstance(value, bool):
            return path_sql  # No casting for booleans
        if isinstance(value, (int, float, Decimal)):
            return SQL("({})::numeric").format(path_sql)
        return path_sql
