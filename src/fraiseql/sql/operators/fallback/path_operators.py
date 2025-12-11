"""Path/tree operator strategy for generic hierarchical operations."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class PathOperatorStrategy(BaseOperatorStrategy):
    """Strategy for generic path/tree operators.

    Supports:
        - depth_eq: Path depth equals value
        - depth_gt: Path depth greater than value
        - depth_lt: Path depth less than value
        - isdescendant: Is descendant of path
    """

    SUPPORTED_OPERATORS = {"depth_eq", "depth_gt", "depth_lt", "isdescendant"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a path operator."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for path operators."""
        if operator == "depth_eq":
            return SQL("nlevel({}) = {}").format(path_sql, Literal(value))

        if operator == "depth_gt":
            return SQL("nlevel({}) > {}").format(path_sql, Literal(value))

        if operator == "depth_lt":
            return SQL("nlevel({}) < {}").format(path_sql, Literal(value))

        if operator == "isdescendant":
            return SQL("{} <@ {}").format(path_sql, Literal(value))

        return None
