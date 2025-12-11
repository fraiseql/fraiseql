# Phase 2: Core Operators Migration

**Phase:** GREEN
**Duration:** 6-8 hours
**Risk:** Low-Medium

---

## Objective

Migrate core, fundamental operators from `operator_strategies.py` to new strategy modules:
- String operators (contains, icontains, startswith, endswith, etc.)
- Numeric operators (eq, neq, gt, gte, lt, lte)
- Boolean operators (eq, neq, isnull)
- Date/DateTime operators

These are the most commonly used operators and form the foundation for all WHERE clauses.

---

## Files to Create

### 1. `src/fraiseql/sql/operators/core/string_operators.py`

```python
"""String operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class StringOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for string field operators.

    Supports:
        - eq, neq: Equality/inequality
        - contains: Case-sensitive substring (uses LIKE)
        - icontains: Case-insensitive substring (uses ILIKE)
        - startswith, istartswith: Prefix matching
        - endswith, iendswith: Suffix matching
        - in, nin: List membership
        - isnull: NULL checking
        - like, ilike: Explicit LIKE with user-provided wildcards
        - matches, imatches: Regex matching
        - not_matches: Negated regex
    """

    SUPPORTED_OPERATORS = {
        "eq", "neq", "contains", "icontains",
        "startswith", "istartswith", "endswith", "iendswith",
        "in", "nin", "isnull",
        "like", "ilike",
        "matches", "imatches", "not_matches"
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a string operator."""
        if field_type is None:
            # Conservative: only handle if operator is clearly string-specific
            return operator in {"contains", "icontains", "startswith", "istartswith",
                              "endswith", "iendswith", "like", "ilike",
                              "matches", "imatches", "not_matches"}

        # With type hint, check if it's a string type
        if field_type is str:
            return operator in self.SUPPORTED_OPERATORS

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for string operators."""
        # Cast to text for JSONB columns
        if jsonb_column:
            casted_path = path_sql
        else:
            casted_path = SQL("CAST({} AS TEXT)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}").format(casted_path, Literal(str(value)))

        # Pattern matching with automatic wildcards
        if operator == "contains":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} LIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~ {}").format(casted_path, Literal(f".*{value}.*"))

        if operator == "icontains":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} ILIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~* {}").format(casted_path, Literal(value))

        if operator == "startswith":
            if isinstance(value, str):
                like_val = f"{value}%"
                return SQL("{} LIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~ {}").format(casted_path, Literal(f"^{value}.*"))

        if operator == "istartswith":
            if isinstance(value, str):
                like_val = f"{value}%"
                return SQL("{} ILIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~* {}").format(casted_path, Literal(f"^{value}"))

        if operator == "endswith":
            if isinstance(value, str):
                like_val = f"%{value}"
                return SQL("{} LIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~ {}").format(casted_path, Literal(f".*{value}$"))

        if operator == "iendswith":
            if isinstance(value, str):
                like_val = f"%{value}"
                return SQL("{} ILIKE {}").format(casted_path, Literal(like_val))
            return SQL("{} ~* {}").format(casted_path, Literal(f"{value}$"))

        # Explicit LIKE/ILIKE (user provides wildcards)
        if operator == "like":
            return SQL("{} LIKE {}").format(casted_path, Literal(str(value)))

        if operator == "ilike":
            return SQL("{} ILIKE {}").format(casted_path, Literal(str(value)))

        # Regex operators
        if operator == "matches":
            return SQL("{} ~ {}").format(casted_path, Literal(value))

        if operator == "imatches":
            return SQL("{} ~* {}").format(casted_path, Literal(value))

        if operator == "not_matches":
            return SQL("{} !~ {}").format(casted_path, Literal(value))

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(str(v)) for v in value)
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(str(v)) for v in value)
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 2. `src/fraiseql/sql/operators/core/numeric_operators.py`

```python
"""Numeric operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class NumericOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for numeric field operators (int, float, Decimal).

    Supports:
        - eq, neq: Equality/inequality
        - gt, gte, lt, lte: Comparison operators
        - in, nin: List membership
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "gt", "gte", "lt", "lte", "in", "nin", "isnull"}

    NUMERIC_TYPES = (int, float)

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a numeric operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is None:
            # Only handle if operator is clearly numeric-specific
            return operator in {"gt", "gte", "lt", "lte"}

        # Check for numeric types
        try:
            return issubclass(field_type, self.NUMERIC_TYPES)
        except TypeError:
            return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for numeric operators."""
        # Cast to appropriate numeric type for JSONB
        if jsonb_column:
            # JSONB numeric values stored as text, need casting
            if field_type is int or isinstance(value, int):
                casted_path = SQL("({})::integer").format(path_sql)
            else:
                casted_path = SQL("({})::numeric").format(path_sql)
        else:
            casted_path = path_sql

        # Comparison operators
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

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(v) for v in value)
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(Literal(v) for v in value)
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 3. `src/fraiseql/sql/operators/core/boolean_operators.py`

```python
"""Boolean operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class BooleanOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for boolean field operators.

    Supports:
        - eq, neq: Equality/inequality
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a boolean operator on a boolean field."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is None:
            return False

        return field_type is bool

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for boolean operators."""
        # Cast to boolean for JSONB
        if jsonb_column:
            casted_path = SQL("({})::boolean").format(path_sql)
        else:
            casted_path = path_sql

        if operator == "eq":
            return SQL("{} = {}").format(casted_path, Literal(bool(value)))

        if operator == "neq":
            return SQL("{} != {}").format(casted_path, Literal(bool(value)))

        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 4. Update `src/fraiseql/sql/operators/core/__init__.py`

```python
"""Core operator strategies."""

from .string_operators import StringOperatorStrategy
from .numeric_operators import NumericOperatorStrategy
from .boolean_operators import BooleanOperatorStrategy

__all__ = [
    "StringOperatorStrategy",
    "NumericOperatorStrategy",
    "BooleanOperatorStrategy",
]
```

---

## Files to Modify

### 1. `src/fraiseql/sql/operators/__init__.py`

Add core strategy exports and auto-register them:

```python
"""
Operator strategies for WHERE clause SQL generation.
"""

from .base import BaseOperatorStrategy
from .strategy_registry import OperatorRegistry, register_operator, get_default_registry

# Import core strategies
from .core import (
    StringOperatorStrategy,
    NumericOperatorStrategy,
    BooleanOperatorStrategy,
)

# Auto-register core strategies
register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
register_operator(BooleanOperatorStrategy())

__all__ = [
    "BaseOperatorStrategy",
    "OperatorRegistry",
    "register_operator",
    "get_default_registry",
    "StringOperatorStrategy",
    "NumericOperatorStrategy",
    "BooleanOperatorStrategy",
]
```

---

## Implementation Steps

### Step 1: Create String Operators (2-3 hours)
1. Copy string operator logic from `operator_strategies.py`
2. Adapt to new `BaseOperatorStrategy` interface
3. Write unit tests
4. Verify all string operator tests pass

### Step 2: Create Numeric Operators (1-2 hours)
1. Copy numeric operator logic
2. Adapt to new interface
3. Write unit tests
4. Verify all numeric operator tests pass

### Step 3: Create Boolean Operators (1 hour)
1. Copy boolean operator logic
2. Adapt to new interface
3. Write unit tests
4. Verify all boolean operator tests pass

### Step 4: Integration Testing (2 hours)
1. Register all core strategies
2. Run full WHERE clause test suite
3. Verify no regressions
4. Check that both old and new paths work

---

## Verification Commands

```bash
# Run core operator unit tests
uv run pytest tests/unit/sql/operators/core/ -v

# Run WHERE clause integration tests (should still pass)
uv run pytest tests/unit/sql/where/ -v

# Run full integration test suite
uv run pytest tests/integration/database/repository/ -k "filter" -v

# Check for string operator usage
uv run pytest tests/ -k "contains or startswith or endswith" -v

# Check for numeric operator usage
uv run pytest tests/ -k "gt or lt or gte or lte" -v
```

---

## Acceptance Criteria

- [ ] `StringOperatorStrategy` implemented with all string operators
- [ ] `NumericOperatorStrategy` implemented with all numeric operators
- [ ] `BooleanOperatorStrategy` implemented with all boolean operators
- [ ] All strategies registered in default registry
- [ ] Unit tests for each strategy passing
- [ ] All existing WHERE clause tests still passing
- [ ] No performance regression
- [ ] Both old and new operator paths work

---

## DO NOT

- ❌ Delete code from `operator_strategies.py` yet
- ❌ Change operator behavior or SQL output
- ❌ Break backward compatibility
- ❌ Skip writing tests

---

## Next Phase

Once core operators are migrated:
→ **Phase 3:** Migrate specialized PostgreSQL type operators (network, ltree, daterange, macaddr)
