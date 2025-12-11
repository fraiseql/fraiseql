"""Null checking operator strategy."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable

from fraiseql.sql.operators.base import BaseOperatorStrategy


class NullOperatorStrategy(BaseOperatorStrategy):
    """Strategy for NULL checking operators.

    Supports:
        - isnull: Check if field is NULL or NOT NULL
    """

    SUPPORTED_OPERATORS = {"isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is the isnull operator."""
        return operator == "isnull"

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for null checks."""
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
