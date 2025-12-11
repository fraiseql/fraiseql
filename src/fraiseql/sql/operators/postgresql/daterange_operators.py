"""DateRange operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class DateRangeOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL daterange operators.

    Supports range operators:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - contains_date: Range contains specific date
        - overlaps: Ranges overlap
        - adjacent: Ranges are adjacent
        - strictly_left: Range is strictly left of another
        - strictly_right: Range is strictly right of another
        - not_left: Range does not extend left
        - not_right: Range does not extend right
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {
        "eq",
        "neq",
        "in",
        "nin",
        "contains_date",
        "overlaps",
        "adjacent",
        "strictly_left",
        "strictly_right",
        "not_left",
        "not_right",
        "isnull",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a daterange operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # DateRange-specific operators
        if operator in {
            "contains_date",
            "overlaps",
            "adjacent",
            "strictly_left",
            "strictly_right",
            "not_left",
            "not_right",
        }:
            return True

        # With type hint
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if "DateRange" in type_name or "daterange" in type_name.lower():
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
        """Build SQL for daterange operators."""
        # Comparison operators
        if operator in ("eq", "neq"):
            casted_path = self._cast_path(
                path_sql, "daterange", jsonb_column, use_postgres_cast=True
            )
            return self._build_comparison(operator, casted_path, str(value))

        # Cast to daterange for range operators
        casted_path = self._cast_path(path_sql, "daterange", jsonb_column, use_postgres_cast=True)

        # Range operators
        if operator == "contains_date":
            return SQL("{} @> {}::date").format(casted_path, Literal(str(value)))

        if operator == "overlaps":
            return SQL("{} && {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "adjacent":
            return SQL("{} -|- {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "strictly_left":
            return SQL("{} << {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "strictly_right":
            return SQL("{} >> {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "not_left":
            return SQL("{} &> {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "not_right":
            return SQL("{} &< {}::daterange").format(casted_path, Literal(str(value)))

        # List operators (check if range is in list)
        if operator == "in":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                cast_values="daterange",
            )

        if operator == "nin":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                negate=True,
                cast_values="daterange",
            )

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
