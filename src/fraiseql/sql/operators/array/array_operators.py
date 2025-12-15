"""Array operator strategies for JSONB array fields."""

import json
from typing import Any, Optional, get_origin

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class ArrayOperatorStrategy(BaseOperatorStrategy):
    """Strategy for JSONB array field operators.

    Supports:
        - eq, neq: Array equality/inequality
        - contains: Array contains elements (@> operator)
        - contained_by: Array is contained by another (<@ operator)
        - overlaps: Arrays have common elements (?| operator)
        - len_eq, len_neq, len_gt, len_gte, len_lt, len_lte: Length operations
        - any_eq: Any element equals value
        - all_eq: All elements equal value
    """

    SUPPORTED_OPERATORS = {
        "eq",
        "neq",
        "contains",
        "contained_by",
        "overlaps",
        "len_eq",
        "len_neq",
        "len_gt",
        "len_gte",
        "len_lt",
        "len_lte",
        "any_eq",
        "all_eq",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is an array operator on an array field."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Only handle array operations when field_type indicates array
        if field_type is None:
            return False

        # Check if field_type is a list type (e.g., list[str], List[int])
        return get_origin(field_type) is list

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for array operators."""
        # Array operations work directly on JSONB arrays
        # path_sql is already the JSONB path (e.g., data->'tags')

        # Equality/inequality - compare JSONB arrays directly
        if operator == "eq":
            json_str = json.dumps(value)
            return SQL("{} = {}::jsonb").format(path_sql, Literal(json_str))

        if operator == "neq":
            json_str = json.dumps(value)
            return SQL("{} != {}::jsonb").format(path_sql, Literal(json_str))

        # Containment operators
        if operator == "contains":
            # @> operator: left_array @> right_array means left contains right
            json_str = json.dumps(value)
            return SQL("{} @> {}::jsonb").format(path_sql, Literal(json_str))

        if operator == "contained_by":
            # <@ operator: left_array <@ right_array means left is contained by right
            json_str = json.dumps(value)
            return SQL("{} <@ {}::jsonb").format(path_sql, Literal(json_str))

        if operator == "overlaps":
            # ?| operator: check if arrays have any elements in common
            if isinstance(value, list):
                # Build ARRAY['item1', 'item2', ...] syntax
                array_elements = [str(item) for item in value]
                array_str = "{" + ",".join(f'"{elem}"' for elem in array_elements) + "}"
                return SQL("{} ?| {}").format(path_sql, Literal(array_str))
            json_str = json.dumps(value)
            return SQL("{} ?| {}").format(path_sql, Literal(json_str))

        # Length operations using jsonb_array_length()
        if operator == "len_eq":
            return SQL("jsonb_array_length({}) = {}").format(path_sql, Literal(value))

        if operator == "len_neq":
            return SQL("jsonb_array_length({}) != {}").format(path_sql, Literal(value))

        if operator == "len_gt":
            return SQL("jsonb_array_length({}) > {}").format(path_sql, Literal(value))

        if operator == "len_gte":
            return SQL("jsonb_array_length({}) >= {}").format(path_sql, Literal(value))

        if operator == "len_lt":
            return SQL("jsonb_array_length({}) < {}").format(path_sql, Literal(value))

        if operator == "len_lte":
            return SQL("jsonb_array_length({}) <= {}").format(path_sql, Literal(value))

        # Element query operations using jsonb_array_elements_text
        if operator == "any_eq":
            # Check if any element in the array equals the value
            return SQL(
                "EXISTS (SELECT 1 FROM jsonb_array_elements_text({}) AS elem WHERE elem = {})"
            ).format(path_sql, Literal(value))

        if operator == "all_eq":
            # Check if all elements in the array equal the value
            # This means: array_length = count of elements that equal the value
            return SQL(
                "jsonb_array_length({}) = "
                "(SELECT COUNT(*) FROM jsonb_array_elements_text({}) AS elem WHERE elem = {})"
            ).format(path_sql, path_sql, Literal(value))

        return None
