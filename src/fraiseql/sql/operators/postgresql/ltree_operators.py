"""LTree hierarchical path operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class LTreeOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL ltree (label tree) operators.

    Supports hierarchical path operators:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - ancestor_of: Is ancestor of path
        - descendant_of: Is descendant of path
        - matches_lquery: Matches lquery pattern
        - matches_ltxtquery: Matches ltxtquery pattern
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {
        "eq",
        "neq",
        "in",
        "nin",
        "ancestor_of",
        "descendant_of",
        "matches_lquery",
        "matches_ltxtquery",
        "isnull",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is an ltree operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # LTree-specific operators always handled by this strategy
        if operator in {
            "ancestor_of",
            "descendant_of",
            "matches_lquery",
            "matches_ltxtquery",
        }:
            return True

        # With type hint, check if it's an LTree type
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if "LTree" in type_name or "ltree" in type_name.lower():
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
        """Build SQL for ltree operators."""
        # Comparison operators
        if operator in ("eq", "neq"):
            casted_path = self._cast_path(path_sql, "ltree", jsonb_column, use_postgres_cast=True)
            return self._build_comparison(operator, casted_path, str(value))

        # Cast to ltree for hierarchical operators
        casted_path = self._cast_path(path_sql, "ltree", jsonb_column, use_postgres_cast=True)

        # Hierarchical operators
        if operator == "ancestor_of":
            return SQL("{} @> {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "descendant_of":
            return SQL("{} <@ {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "matches_lquery":
            return SQL("{} ~ {}::lquery").format(casted_path, Literal(str(value)))

        if operator == "matches_ltxtquery":
            return SQL("{} @ {}::ltxtquery").format(casted_path, Literal(str(value)))

        # List operators
        if operator == "in":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                cast_values="ltree",
            )

        if operator == "nin":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                negate=True,
                cast_values="ltree",
            )

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
