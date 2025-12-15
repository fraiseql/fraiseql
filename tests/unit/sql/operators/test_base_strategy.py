"""Tests for base operator strategy."""

import pytest
from psycopg.sql import SQL, Identifier, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy
from fraiseql.sql.operators.strategy_registry import OperatorRegistry


class MockStrategy(BaseOperatorStrategy):
    """Mock strategy for testing."""

    def supports_operator(self, operator: str, field_type: type | None) -> bool:
        return operator == "mock_op"

    def build_sql(self, operator, value, path_sql, field_type=None, jsonb_column=None):
        return SQL("{} = {}").format(path_sql, Literal(value))


class TestBaseStrategy:
    """Test base operator strategy."""

    def test_abstract_methods_must_be_implemented(self):
        """Test that abstract methods must be implemented."""
        with pytest.raises(TypeError):
            BaseOperatorStrategy()

    def test_mock_strategy_works(self):
        """Test mock strategy implementation."""
        strategy = MockStrategy()

        assert strategy.supports_operator("mock_op", None)
        assert not strategy.supports_operator("other_op", None)

        sql = strategy.build_sql("mock_op", "test", Identifier("field"))
        assert sql is not None


class TestOperatorRegistry:
    """Test operator registry."""

    def test_register_strategy(self):
        """Test registering a strategy."""
        registry = OperatorRegistry()
        strategy = MockStrategy()

        registry.register(strategy)

        found = registry.get_strategy("mock_op")
        assert found is strategy

    def test_strategy_not_found(self):
        """Test when no strategy supports operator."""
        registry = OperatorRegistry()

        found = registry.get_strategy("unknown_op")
        assert found is None

    def test_last_registered_wins(self):
        """Test that last registered strategy takes precedence."""
        registry = OperatorRegistry()

        strategy1 = MockStrategy()
        strategy2 = MockStrategy()

        registry.register(strategy1)
        registry.register(strategy2)

        found = registry.get_strategy("mock_op")
        assert found is strategy2  # Last one wins

    def test_build_sql_with_registry(self):
        """Test building SQL through registry."""
        registry = OperatorRegistry()
        strategy = MockStrategy()
        registry.register(strategy)

        sql = registry.build_sql("mock_op", "test_value", Identifier("field"))
        assert sql is not None

    def test_build_sql_returns_none_for_unknown_operator(self):
        """Test that build_sql returns None for unknown operators."""
        registry = OperatorRegistry()

        sql = registry.build_sql("unknown_op", "value", Identifier("field"))
        assert sql is None
