"""JSONB-specific operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class JsonbOperatorStrategy(BaseOperatorStrategy):
    """Strategy for JSONB-specific operators.

    Supports:
        - overlaps: JSONB objects/arrays overlap (&&)
        - strictly_contains: JSONB strictly contains (@> but not equal)
    """

    SUPPORTED_OPERATORS = {"overlaps", "strictly_contains"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a JSONB-specific operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # These are JSONB-specific operators - handle only when we know it's JSONB
        # In practice, these operators are only used with JSONB fields
        return True

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for JSONB operators."""
        if operator == "overlaps":
            # && operator: check if JSONB objects/arrays overlap
            return SQL("{} && {}").format(path_sql, Literal(value))

        if operator == "strictly_contains":
            # @> operator but exclude exact equality
            # Means: contains the value AND is not equal to the value
            return SQL("{} @> {} AND {} != {}").format(
                path_sql, Literal(value), path_sql, Literal(value)
            )

        return None
