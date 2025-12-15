"""Registry for operator strategies."""

from typing import Any, List, Optional

from psycopg.sql import Composable

from .base import BaseOperatorStrategy


class OperatorRegistry:
    """Registry for operator strategies.

    Manages the collection of operator strategies and routes
    operator requests to the appropriate strategy.
    """

    def __init__(self):
        self._strategies: List[BaseOperatorStrategy] = []

    def register(self, strategy: BaseOperatorStrategy) -> None:
        """Register an operator strategy."""
        self._strategies.append(strategy)

    def get_strategy(
        self, operator: str, field_type: Optional[type] = None
    ) -> Optional[BaseOperatorStrategy]:
        """Find the first strategy that supports the given operator.

        Strategies are checked in reverse registration order (last registered wins).
        This allows specialized strategies to override general ones.
        """
        for strategy in reversed(self._strategies):
            if strategy.supports_operator(operator, field_type):
                return strategy
        return None

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL using the appropriate strategy.

        Returns:
            SQL fragment, or None if no strategy supports this operator
        """
        strategy = self.get_strategy(operator, field_type)
        if strategy is None:
            return None

        return strategy.build_sql(
            operator=operator,
            value=value,
            path_sql=path_sql,
            field_type=field_type,
            jsonb_column=jsonb_column,
        )


# Global registry instance
_default_registry = OperatorRegistry()


def register_operator(strategy: BaseOperatorStrategy) -> None:
    """Register an operator strategy with the default registry."""
    _default_registry.register(strategy)


def get_default_registry() -> OperatorRegistry:
    """Get the default operator registry."""
    return _default_registry
