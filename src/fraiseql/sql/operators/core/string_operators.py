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
        # Cast to text for JSONB columns
        if jsonb_column:
            casted_path = path_sql
        else:
            casted_path = SQL("CAST({} AS TEXT)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}").format(casted_path, Literal(str(value)))

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
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(str(v)) for v in value)
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(str(v)) for v in value)
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
