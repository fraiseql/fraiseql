"""Base operator strategy abstract class."""

from abc import ABC, abstractmethod
from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal


class BaseOperatorStrategy(ABC):
    """Abstract base class for all operator strategies.

    Each operator strategy handles SQL generation for a specific family
    of operators (e.g., string, numeric, array, etc.).
    """

    @abstractmethod
    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this strategy supports the given operator and field type.

        Args:
            operator: Operator name (e.g., "eq", "contains", "isprivate")
            field_type: Python type hint of the field (if available)

        Returns:
            True if this strategy can handle this operator+type combination
        """

    @abstractmethod
    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for the given operator.

        Args:
            operator: Operator name (e.g., "eq", "gt", "contains")
            value: Filter value
            path_sql: SQL fragment for accessing the field
            field_type: Python type hint of the field
            jsonb_column: JSONB column name if this is JSONB-based

        Returns:
            Composable SQL fragment, or None if operator not supported
        """

    # Helper methods for common operations

    def _cast_path(
        self,
        path_sql: Composable,
        target_type: str,
        jsonb_column: Optional[str] = None,
        use_postgres_cast: bool = False,
    ) -> Composable:
        """Cast path SQL to specified PostgreSQL type.

        IMPORTANT: This method handles the critical difference between JSONB columns
        and regular columns. JSONB extracts are always text-like and need explicit
        casting to other types, while regular columns may already have the correct type.

        Args:
            path_sql: SQL fragment for accessing the field
            target_type: PostgreSQL type name (e.g., "text", "inet", "integer")
            jsonb_column: JSONB column name if this is JSONB-based
            use_postgres_cast: If True, use ::type syntax, else use CAST(x AS type)

        Returns:
            Casted SQL fragment

        Performance note: use_postgres_cast=True is slightly faster (no function call overhead)
        but CAST() syntax is more SQL-standard. Use :: for hot paths.
        """
        if jsonb_column:
            # JSONB extracts are already text-like, some need explicit casting
            if target_type.lower() in ("text", "varchar", "char"):
                # Already text, no cast needed
                return path_sql
            # Need to cast from JSONB-extracted value
            if use_postgres_cast:
                return SQL("({})::{}").format(path_sql, SQL(target_type))
            return SQL("CAST({} AS {})").format(path_sql, SQL(target_type))
        # Regular column - no cast needed unless explicitly requested
        return path_sql

    def _cast_both_sides(
        self,
        path_sql: Composable,
        value: Any,
        target_type: str,
        use_postgres_cast: bool = True,
    ) -> tuple[Composable, Composable]:
        """Cast both field path and value to PostgreSQL type.

        This method ensures consistent type handling by casting both sides
        of a comparison to the specified PostgreSQL type. Works correctly
        for both JSONB-extracted fields and regular typed columns.

        Args:
            path_sql: SQL fragment for accessing the field
                      Examples: data->>'mac_address', mac_address
            value: Value to cast (will be wrapped in Literal())
            target_type: PostgreSQL type name
                         Examples: macaddr, inet, ltree, daterange, point
            use_postgres_cast: If True, use ::type syntax (faster)
                               If False, use CAST(x AS type) syntax

        Returns:
            Tuple of (casted_path, casted_value)

        Examples:
            >>> path = SQL("data->>'mac'")
            >>> result = self._cast_both_sides(path, "00:11:22:33:44:55", "macaddr")
            >>> # result[0]: (data->>'mac')::macaddr
            >>> # result[1]: '00:11:22:33:44:55'::macaddr

            >>> path = SQL("ip_address")
            >>> casted_path, casted_value = self._cast_both_sides(path, "192.168.1.1", "inet")
            >>> # casted_path: (ip_address)::inet  (redundant but harmless)
            >>> # casted_value: '192.168.1.1'::inet

        Note:
            Casting a value to its own type (e.g., macaddr::macaddr) is
            a no-op in PostgreSQL with negligible performance cost.
            This approach simplifies logic and prevents bugs.
        """
        from psycopg.sql import SQL, Literal

        if use_postgres_cast:
            # Use PostgreSQL :: syntax (slightly faster)
            casted_path = SQL("({})::{}").format(path_sql, SQL(target_type))
            casted_value = SQL("{}::{}").format(Literal(value), SQL(target_type))
        else:
            # Use standard SQL CAST() syntax
            casted_path = SQL("CAST({} AS {})").format(path_sql, SQL(target_type))
            casted_value = SQL("CAST({} AS {})").format(Literal(value), SQL(target_type))

        return casted_path, casted_value

    def _cast_list_values(
        self,
        values: list[Any],
        target_type: str,
        use_postgres_cast: bool = True,
    ) -> list[Composable]:
        """Cast a list of values to PostgreSQL type.

        Helper for IN/NOT IN operators that need to cast multiple values.

        Args:
            values: List of values to cast
            target_type: PostgreSQL type name
            use_postgres_cast: If True, use ::type syntax

        Returns:
            List of casted SQL fragments

        Example:
            >>> values = ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff"]
            >>> casted = self._cast_list_values(values, "macaddr")
            >>> # ['00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr]
        """
        from psycopg.sql import SQL, Literal

        casted_values = []
        for value in values:
            if use_postgres_cast:
                casted = SQL("{}::{}").format(Literal(value), SQL(target_type))
            else:
                casted = SQL("CAST({} AS {})").format(Literal(value), SQL(target_type))
            casted_values.append(casted)

        return casted_values

    def _build_comparison(
        self, operator: str, casted_path: Composable, value: Any
    ) -> Optional[Composable]:
        """Build SQL for common comparison operators.

        Args:
            operator: One of: eq, neq, gt, gte, lt, lte
            casted_path: Already-casted path SQL
            value: Comparison value

        Returns:
            SQL comparison fragment, or None if operator not supported
        """
        if operator == "eq":
            return SQL("{} = {}").format(casted_path, Literal(value))

        if operator == "neq":
            return SQL("{} != {}").format(casted_path, Literal(value))

        if operator == "gt":
            return SQL("{} > {}").format(casted_path, Literal(value))

        if operator == "gte":
            return SQL("{} >= {}").format(casted_path, Literal(value))

        if operator == "lt":
            return SQL("{} < {}").format(casted_path, Literal(value))

        if operator == "lte":
            return SQL("{} <= {}").format(casted_path, Literal(value))

        return None

    def _build_in_operator(
        self,
        casted_path: Composable,
        value: Any,
        negate: bool = False,
        cast_values: Optional[str] = None,
    ) -> Composable:
        """Build SQL for IN or NOT IN operators.

        Args:
            casted_path: Already-casted path SQL
            value: List of values (will be normalized to list if single value)
            negate: If True, use NOT IN, else use IN
            cast_values: Optional PostgreSQL type to cast each value

        Returns:
            SQL IN/NOT IN fragment
        """
        # Normalize to list
        if isinstance(value, list):
            value_list = value
        elif isinstance(value, tuple):
            value_list = list(value)
        else:
            value_list = [value]

        # Build placeholders
        if cast_values:
            cast_sql = SQL("::{}").format(SQL(cast_values))
            placeholders = SQL(", ").join(
                SQL("{}{}").format(Literal(v), cast_sql) for v in value_list
            )
        else:
            placeholders = SQL(", ").join(Literal(v) for v in value_list)

        # Build IN or NOT IN
        if negate:
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)
        return SQL("{} IN ({})").format(casted_path, placeholders)

    def _build_null_check(self, path_sql: Composable, value: Any) -> Composable:
        """Build SQL for IS NULL / IS NOT NULL checks.

        Args:
            path_sql: Original path SQL (NOT casted - NULL checks don't need casting)
            value: Boolean indicating if checking for NULL (True) or NOT NULL (False)

        Returns:
            SQL IS NULL or IS NOT NULL fragment
        """
        if value:
            return SQL("{} IS NULL").format(path_sql)
        return SQL("{} IS NOT NULL").format(path_sql)


class OperatorStrategyError(Exception):
    """Raised when operator strategy encounters an error."""
