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
        "notin",
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
        # For DateRange fields, we support all operators (to reject unsupported ones)
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if "DateRange" in type_name or "daterange" in type_name.lower():
                return True

        # For non-DateRange fields, only support our specialized operators
        specialized_operators = {
            "contains_date",
            "overlaps",
            "adjacent",
            "strictly_left",
            "strictly_right",
            "not_left",
            "not_right",
        }
        return operator in specialized_operators

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for daterange operators."""
        # Validate that pattern operators are not used with DateRange
        pattern_operators = {"contains", "startswith", "endswith"}
        if operator in pattern_operators and field_type and hasattr(field_type, "__name__"):
            type_name = field_type.__name__.lower()
            if "daterange" in type_name:
                raise ValueError(
                    f"Pattern operator '{operator}' is not supported for DateRange fields. "
                    "DateRange only supports range-specific operators."
                )

        # Comparison operators
        if operator == "eq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} = {}").format(casted_path, casted_value)

        if operator == "neq":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} != {}").format(casted_path, casted_value)

        # Range operators
        if operator == "contains_date":
            casted_path = SQL("({})::daterange").format(path_sql)
            casted_value = SQL("{}::date").format(Literal(str(value)))
            return SQL("{} @> {}").format(casted_path, casted_value)

        if operator == "overlaps":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} && {}").format(casted_path, casted_value)

        if operator == "adjacent":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} -|- {}").format(casted_path, casted_value)

        if operator == "strictly_left":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} << {}").format(casted_path, casted_value)

        if operator == "strictly_right":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} >> {}").format(casted_path, casted_value)

        if operator == "not_left":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} &> {}").format(casted_path, casted_value)

        if operator == "not_right":
            casted_path, casted_value = self._cast_both_sides(path_sql, str(value), "daterange")
            return SQL("{} &< {}").format(casted_path, casted_value)

        # List operators (check if range is in list)
        if operator == "in":
            # Cast field path
            casted_path = SQL("({})::daterange").format(path_sql)

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values([str(v) for v in value_list], "daterange")

            # Build IN clause: field IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} IN ({})").format(casted_path, values_sql)

        if operator in ("nin", "notin"):
            # Cast field path
            casted_path = SQL("({})::daterange").format(path_sql)

            # Cast each value in list
            value_list = value if isinstance(value, (list, tuple)) else [value]
            casted_values = self._cast_list_values([str(v) for v in value_list], "daterange")

            # Build NOT IN clause: field NOT IN (val1, val2, ...)
            values_sql = SQL(", ").join(casted_values)
            return SQL("{} NOT IN ({})").format(casted_path, values_sql)

        # NULL checking (don't cast for NULL checks)
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
