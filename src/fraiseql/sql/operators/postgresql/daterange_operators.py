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
        # Cast to daterange for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::daterange").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS daterange)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}::daterange").format(casted_path, Literal(str(value)))

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
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::daterange").format(Literal(str(v))) for v in value
            )
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::daterange").format(Literal(str(v))) for v in value
            )
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
