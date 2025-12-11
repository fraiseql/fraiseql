"""MAC address operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class MacAddressOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL macaddr/macaddr8 operators.

    Supports:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "in", "nin", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a MAC address operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
            if "MacAddr" in type_name or "macaddr" in type_name.lower():
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
        """Build SQL for MAC address operators."""
        # Cast to macaddr for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::macaddr").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS macaddr)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}::macaddr").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}::macaddr").format(casted_path, Literal(str(value)))

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(SQL("{}::macaddr").format(Literal(str(v))) for v in value)
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(SQL("{}::macaddr").format(Literal(str(v))) for v in value)
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            return SQL("{} IS NOT NULL").format(path_sql)

        return None
