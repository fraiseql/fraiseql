"""String operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class StringOperatorStrategy(BaseOperatorStrategy):
    """Strategy for string field operators.

    Supports:
        - eq, neq: Equality/inequality
        - contains: Case-sensitive substring (uses LIKE)
        - icontains: Case-insensitive substring (uses ILIKE)
        - startswith, istartswith: Prefix matching
        - endswith, iendswith: Suffix matching
        - in, nin: List membership
        - isnull: NULL checking
        - like, ilike: Explicit LIKE with user-provided wildcards
        - matches, imatches: Regex matching
        - not_matches: Negated regex
    """

    SUPPORTED_OPERATORS = {
        "eq",
        "neq",
        "contains",
        "icontains",
        "startswith",
        "istartswith",
        "endswith",
        "iendswith",
        "in",
        "nin",
        "isnull",
        "like",
        "ilike",
        "matches",
        "imatches",
        "not_matches",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a string operator."""
        if field_type is None:
            # Conservative: only handle if operator is clearly string-specific
            return operator in {
                "contains",
                "icontains",
                "startswith",
                "istartswith",
                "endswith",
                "iendswith",
                "like",
                "ilike",
                "matches",
                "imatches",
                "not_matches",
            }

        # With type hint, check if it's a string type
        if field_type is str:
            return operator in self.SUPPORTED_OPERATORS

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for string operators."""
        # Comparison operators (eq, neq)
        if operator in ("eq", "neq"):
            casted_path = self._cast_path(path_sql, "text", jsonb_column)
            return self._build_comparison(operator, casted_path, str(value))

        # Cast to text for pattern matching operators
        casted_path = self._cast_path(path_sql, "text", jsonb_column)

        # Pattern matching with automatic wildcards
        if operator == "contains":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} LIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~ {}").format(casted_path, Literal(f".*{value}.*"))

        if operator == "icontains":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} ILIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~* {}").format(casted_path, Literal(value))

        if operator == "startswith":
            if isinstance(value, str):
                like_val = f"{value}%"
                return SQL("{} LIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~ {}").format(casted_path, Literal(f"^{value}.*"))

        if operator == "istartswith":
            if isinstance(value, str):
                like_val = f"{value}%"
                return SQL("{} ILIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~* {}").format(casted_path, Literal(f"^{value}"))

        if operator == "endswith":
            if isinstance(value, str):
                like_val = f"%{value}"
                return SQL("{} LIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~ {}").format(casted_path, Literal(f".*{value}$"))

        if operator == "iendswith":
            if isinstance(value, str):
                like_val = f"%{value}"
                return SQL("{} ILIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~* {}").format(casted_path, Literal(f"{value}$"))

        # Explicit LIKE/ILIKE (user provides wildcards)
        if operator == "like":
            return SQL("{} LIKE {}").format(casted_path, Literal(str(value)))

        if operator == "ilike":
            return SQL("{} ILIKE {}").format(casted_path, Literal(str(value)))

        # Regex operators
        if operator == "matches":
            return SQL("{} ~ {}").format(casted_path, Literal(value))

        if operator == "imatches":
            return SQL("{} ~* {}").format(casted_path, Literal(value))

        if operator == "not_matches":
            return SQL("{} !~ {}").format(casted_path, Literal(value))

        # List operators
        if operator == "in":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
            )

        if operator == "nin":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                negate=True,
            )

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
