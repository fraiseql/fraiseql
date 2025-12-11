"""Fallback pattern matching operator strategy."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class PatternOperatorStrategy(BaseOperatorStrategy):
    """Fallback strategy for pattern matching operators.

    This strategy handles pattern operators that weren't caught by
    more specific strategies (like StringOperatorStrategy).

    Supports:
        - matches: Regex match (~)
        - imatches: Case-insensitive regex match (~*)
        - not_matches: Negated regex match (!~)
        - startswith: Prefix matching (LIKE)
        - endswith: Suffix matching (LIKE)
        - contains: Substring matching (LIKE)
        - ilike: Case-insensitive substring (ILIKE)
    """

    SUPPORTED_OPERATORS = {
        "matches",
        "imatches",
        "not_matches",
        "startswith",
        "endswith",
        "contains",
        "ilike",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a pattern matching operator (fallback - always handles these)."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for pattern matching operators."""
        # No special type casting needed for pattern matching
        # Text operations work naturally

        if operator == "matches":
            return SQL("{} ~ {}").format(path_sql, Literal(value))

        if operator == "imatches":
            return SQL("{} ~* {}").format(path_sql, Literal(value))

        if operator == "not_matches":
            return SQL("{} !~ {}").format(path_sql, Literal(value))

        if operator == "startswith":
            if isinstance(value, str):
                like_val = f"{value}%"
                return SQL("{} LIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~ {}").format(path_sql, Literal(f"^{value}.*"))

        if operator == "endswith":
            if isinstance(value, str):
                like_val = f"%{value}"
                return SQL("{} LIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~ {}").format(path_sql, Literal(f".*{value}$"))

        if operator == "contains":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} LIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~ {}").format(path_sql, Literal(f".*{value}.*"))

        if operator == "ilike":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} ILIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~* {}").format(path_sql, Literal(value))

        return None
