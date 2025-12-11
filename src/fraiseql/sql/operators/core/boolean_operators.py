"""Boolean operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class BooleanOperatorStrategy(BaseOperatorStrategy):
    """Strategy for boolean field operators.

    Supports:
        - eq, neq: Equality/inequality
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a boolean operator on a boolean field."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is None:
            return False

        return field_type is bool

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for boolean operators."""
        # Cast to boolean for JSONB
        if jsonb_column:
            casted_path = SQL("({})::boolean").format(path_sql)
        else:
            casted_path = path_sql

        if operator == "eq":
            return SQL("{} = {}").format(casted_path, Literal(bool(value)))

        if operator == "neq":
            return SQL("{} != {}").format(casted_path, Literal(bool(value)))

        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
