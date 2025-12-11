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
        # Cast to ltree for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::ltree").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS ltree)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}::ltree").format(casted_path, Literal(str(value)))

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
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(SQL("{}::ltree").format(Literal(str(v))) for v in value)
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(SQL("{}::ltree").format(Literal(str(v))) for v in value)
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
