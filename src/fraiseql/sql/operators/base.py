"""Base operator strategy abstract class."""

from abc import ABC, abstractmethod
from typing import Any, Optional

from psycopg.sql import Composable


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


class OperatorStrategyError(Exception):
    """Raised when operator strategy encounters an error."""
