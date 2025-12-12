"""Fallback comparison operator strategy."""

import re
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Optional
from uuid import UUID

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy

# IP address pattern with validation (matches valid IPv4 and basic IPv6)
# IPv4: octets must be 0-255
# IPv6: simplified pattern for hex groups separated by colons
_IP_PATTERN = re.compile(
    r"^(?:"
    # IPv4 with octet validation (0-255)
    r"(?:(?:25[0-5]|2[0-4]\d|1\d{2}|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d{2}|[1-9]?\d)|"
    r"(?:[0-9a-fA-F]{0,4}:){2,7}[0-9a-fA-F]{0,4}"  # IPv6
    r")$"
)


class ComparisonOperatorStrategy(BaseOperatorStrategy):
    """Fallback strategy for comparison operators (=, !=, <, >, <=, >=).

    This strategy handles comparison operators that weren't caught by
    more specific strategies (like NumericOperatorStrategy, StringOperatorStrategy, etc.).

    Supports:
        - eq, neq: Equality/inequality
        - gt, gte, lt, lte: Comparison operators
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "gt", "gte", "lt", "lte"}

    OPERATOR_MAP = {
        "eq": " = ",
        "neq": " != ",
        "gt": " > ",
        "gte": " >= ",
        "lt": " < ",
        "lte": " <= ",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a comparison operator (fallback - always handles these)."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for comparison operators with proper type casting."""
        # Apply type casting based on value type
        # For JSONB, this is required; for regular columns, this adds safety
        if jsonb_column:
            casted_path = self._apply_type_cast(path_sql, value)
        # Even for regular columns, detect IP addresses for auto-casting
        elif isinstance(value, str) and _IP_PATTERN.match(value):
            casted_path = SQL("({})::inet").format(path_sql)
        else:
            casted_path = path_sql

        sql_op = self.OPERATOR_MAP.get(operator)
        if not sql_op:
            return None

        # Handle boolean values specially for JSONB text comparison
        if isinstance(value, bool) and jsonb_column:
            # JSONB stores booleans as "true"/"false" text when extracted with ->>
            string_val = "true" if value else "false"
            return SQL("{}{}{}").format(casted_path, SQL(sql_op), Literal(string_val))

        # Standard comparison
        return SQL("{}{}{}").format(casted_path, SQL(sql_op), Literal(value))

    def _apply_type_cast(self, path_sql: SQL, value: Any) -> Composable:
        """Apply appropriate type casting to the JSONB path based on value type."""
        # Check bool BEFORE int since bool is subclass of int in Python
        if isinstance(value, bool):
            # For booleans, don't cast - will handle value conversion in build_sql
            return path_sql
        if isinstance(value, (int, float, Decimal)):
            # All numeric operations need numeric casting
            return SQL("({})::numeric").format(path_sql)
        if isinstance(value, datetime):
            return SQL("({})::timestamp").format(path_sql)
        if isinstance(value, date):
            return SQL("({})::date").format(path_sql)
        if isinstance(value, UUID):
            return SQL("({})::uuid").format(path_sql)
        # Check for IP address patterns (auto-detection)
        if isinstance(value, str) and _IP_PATTERN.match(value):
            return SQL("({})::inet").format(path_sql)
        # Default: no casting (treat as text)
        return path_sql
