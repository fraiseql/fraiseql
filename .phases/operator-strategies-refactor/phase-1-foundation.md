# Phase 1: Foundation & Test Infrastructure

**Phase:** RED (Write Failing Tests First)
**Duration:** 4-6 hours
**Risk:** Low

---

## Objective

**TDD Phase RED:** Write the test infrastructure and failing tests BEFORE implementing anything.

Create:
- Base strategy abstract class
- Directory structure
- Strategy registry system
- Comprehensive test suite (FAILING initially)
- Public API exports

**Success Criteria:** Tests are written and FAIL because implementations don't exist yet

---

## Files to Create

### 1. `src/fraiseql/sql/operators/__init__.py`

```python
"""
Operator strategies for WHERE clause SQL generation.

Public API for all operator strategies. This module maintains backward
compatibility with the old `operator_strategies.py` module.
"""

from .base import BaseOperatorStrategy
from .strategy_registry import OperatorRegistry, register_operator

# Re-export for backward compatibility
__all__ = [
    "BaseOperatorStrategy",
    "OperatorRegistry",
    "register_operator",
]
```

### 2. `src/fraiseql/sql/operators/base.py`

```python
"""Base operator strategy abstract class."""

from abc import ABC, abstractmethod
from typing import Any, Optional
from psycopg.sql import Composable, SQL


class BaseOperatorStrategy(ABC):
    """
    Abstract base class for all operator strategies.

    Each operator strategy handles SQL generation for a specific family
    of operators (e.g., string, numeric, array, etc.).
    """

    @abstractmethod
    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """
        Check if this strategy supports the given operator and field type.

        Args:
            operator: Operator name (e.g., "eq", "contains", "isprivate")
            field_type: Python type hint of the field (if available)

        Returns:
            True if this strategy can handle this operator+type combination
        """
        pass

    @abstractmethod
    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """
        Build SQL for the given operator.

        Args:
            operator: Operator name (e.g., "eq", "gt", "contains")
            value: Filter value
            path_sql: SQL fragment for accessing the field
            field_type: Python type hint of the field
            jsonb_column: JSONB column name if this is JSONB-based

        Returns:
            Composable SQL fragment, or None if operator not supported
        """
        pass


class OperatorStrategyError(Exception):
    """Raised when operator strategy encounters an error."""
    pass
```

### 3. `src/fraiseql/sql/operators/strategy_registry.py`

```python
"""Registry for operator strategies."""

from typing import Dict, List, Optional, Any
from psycopg.sql import Composable

from .base import BaseOperatorStrategy


class OperatorRegistry:
    """
    Registry for operator strategies.

    Manages the collection of operator strategies and routes
    operator requests to the appropriate strategy.
    """

    def __init__(self):
        self._strategies: List[BaseOperatorStrategy] = []

    def register(self, strategy: BaseOperatorStrategy) -> None:
        """Register an operator strategy."""
        self._strategies.append(strategy)

    def get_strategy(
        self,
        operator: str,
        field_type: Optional[type] = None
    ) -> Optional[BaseOperatorStrategy]:
        """
        Find the first strategy that supports the given operator.

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
        """
        Build SQL using the appropriate strategy.

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
```

### 4. Create Directory Structure

```bash
mkdir -p src/fraiseql/sql/operators/core
mkdir -p src/fraiseql/sql/operators/array
mkdir -p src/fraiseql/sql/operators/postgresql
mkdir -p src/fraiseql/sql/operators/advanced
mkdir -p src/fraiseql/sql/operators/utils

# Create __init__.py files
touch src/fraiseql/sql/operators/core/__init__.py
touch src/fraiseql/sql/operators/array/__init__.py
touch src/fraiseql/sql/operators/postgresql/__init__.py
touch src/fraiseql/sql/operators/advanced/__init__.py
touch src/fraiseql/sql/operators/utils/__init__.py
```

---

## Files to Modify

### 1. Keep `src/fraiseql/sql/operator_strategies.py` (DO NOT DELETE YET)

Add deprecation notice at top:

```python
"""
DEPRECATED: This module is being refactored into fraiseql.sql.operators.

For backward compatibility, this module still works but new code should use:
    from fraiseql.sql.operators import OperatorRegistry

This file will be removed in a future version after all operators are migrated.
"""

import warnings

warnings.warn(
    "operator_strategies.py is deprecated. Use fraiseql.sql.operators instead.",
    DeprecationWarning,
    stacklevel=2
)

# ... existing code unchanged ...
```

---

## Implementation Steps

### Step 1: Create Base Structure (30 min)
1. Create all directories
2. Create `base.py` with `BaseOperatorStrategy`
3. Create `strategy_registry.py` with `OperatorRegistry`
4. Create `__init__.py` with public API

### Step 2: Add Unit Tests (1 hour)
Create `tests/unit/sql/operators/test_base_strategy.py`:

```python
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
```

### Step 3: Verify Structure (30 min)
1. Run unit tests
2. Verify imports work:
   ```python
   from fraiseql.sql.operators import BaseOperatorStrategy, OperatorRegistry
   ```
3. Verify old imports still work:
   ```python
   from fraiseql.sql.operator_strategies import OperatorStrategy  # Should work
   ```

---

## Verification Commands

```bash
# Run new unit tests
uv run pytest tests/unit/sql/operators/ -v

# Verify no regressions
uv run pytest tests/unit/sql/where/ -v

# Check structure
tree src/fraiseql/sql/operators/
```

---

## Acceptance Criteria

- [ ] `src/fraiseql/sql/operators/` directory created
- [ ] `BaseOperatorStrategy` abstract class implemented
- [ ] `OperatorRegistry` class implemented
- [ ] All subdirectories created with `__init__.py` files
- [ ] Unit tests for base classes passing
- [ ] Old `operator_strategies.py` still works (backward compatibility)
- [ ] No existing tests broken

---

## DO NOT

- ❌ Delete or modify `operator_strategies.py` (keep for backward compatibility)
- ❌ Move any operators yet (that's Phase 2+)
- ❌ Change any existing operator behavior
- ❌ Modify any calling code

---

## Next Phase

Once this phase is complete and all tests pass:
→ **Phase 2:** Migrate core operators (string, numeric, boolean)
