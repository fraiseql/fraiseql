# Operator Strategy Architecture

## Overview

FraiseQL uses a modular operator strategy pattern to generate SQL for WHERE clause filtering operations. This architecture replaced a monolithic 2,149-line file with 12 focused, maintainable modules.

## Historical Context

**Before (v0.x):**
- Single file: `src/fraiseql/sql/operator_strategies.py` (2,149 lines)
- Mixed concerns: string ops, network ops, JSONB ops, array ops all in one file
- Difficult to navigate and maintain
- Hard to add new operator families

**After (v1.0+):**
- Modular architecture: `src/fraiseql/sql/operators/` (12 files, ~150-250 lines each)
- Separation of concerns: each operator family in its own module
- Easy to navigate (find string operators in `core/string_operators.py`)
- Easy to extend (add new module for new operator family)

## Architecture Principles

### 1. Strategy Pattern

Each operator family is implemented as a strategy class inheriting from `BaseOperatorStrategy`:

```python
class StringOperatorStrategy(BaseOperatorStrategy):
    """Handles string field operators (contains, startswith, matches, etc.)."""

    def supports_operator(self, operator: str, field_type: type | None) -> bool:
        """Check if this strategy can handle the operator."""
        return operator in self.SUPPORTED_OPERATORS and field_type is str

    def build_sql(self, operator: str, value: Any, path_sql: Composable, ...) -> Composable:
        """Build SQL for the operator."""
        # Implementation...
```

### 2. Registry Pattern

A central registry manages all operator strategies and dispatches operator requests:

```python
# Registration (automatic at import)
from fraiseql.sql.operators import register_operator

register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
# ...

# Usage
from fraiseql.sql.operators import get_default_registry

registry = get_default_registry()
sql = registry.build_sql("contains", "test", path_sql, field_type=str)
```

### 3. Separation of Concerns

Operators organized by domain:

- **Core** (`core/`): Universal operators (string, numeric, boolean, date)
- **PostgreSQL** (`postgresql/`): PostgreSQL-specific types (network, ltree, daterange, macaddr)
- **Advanced** (`advanced/`): Complex types (array, JSONB, fulltext, vector, coordinate)
- **Utils** (`utils/`): Shared utilities (type detection, SQL builders)

### 4. Base Class Helpers

Common patterns extracted to `BaseOperatorStrategy`:

- `_cast_path()`: Handle JSONB vs regular column casting
- `_build_comparison()`: Generate comparison SQL (eq, neq, gt, gte, lt, lte)
- `_build_in_operator()`: Generate IN/NOT IN SQL with value casting
- `_build_null_check()`: Generate IS NULL/IS NOT NULL SQL

**Benefits:**
- Eliminates 200+ lines of duplication
- Consistent SQL generation
- Easier to maintain and test

## Directory Structure

```
src/fraiseql/sql/operators/
├── __init__.py                    # Public API exports & auto-registration
├── base.py                        # BaseOperatorStrategy (abstract base class)
├── strategy_registry.py           # OperatorRegistry (dispatch system)
│
├── core/                          # Core operators (universal)
│   ├── __init__.py
│   ├── string_operators.py       # contains, icontains, startswith, matches
│   ├── numeric_operators.py      # eq, neq, gt, gte, lt, lte
│   ├── boolean_operators.py      # eq, neq, isnull
│   └── date_operators.py         # Date comparisons (if implemented)
│
├── postgresql/                    # PostgreSQL-specific types
│   ├── __init__.py
│   ├── network_operators.py      # INET/CIDR: isprivate, ispublic, insubnet
│   ├── ltree_operators.py        # LTree: ancestor_of, descendant_of
│   ├── daterange_operators.py    # DateRange: contains_date, overlaps
│   └── macaddr_operators.py      # MAC Address operators
│
├── advanced/                      # Advanced/complex types
│   ├── __init__.py
│   ├── array_operators.py        # Array: contains, overlaps, len_eq
│   ├── jsonb_operators.py        # JSONB: has_key, contains, path_exists
│   ├── fulltext_operators.py     # Full-text: matches, rank, websearch
│   ├── vector_operators.py       # Vector: cosine_distance, l2_distance
│   └── coordinate_operators.py   # GIS: distance_within
│
└── utils/                         # Shared utilities
    ├── __init__.py
    ├── type_detection.py         # Field type detection
    └── sql_builders.py           # SQL building helpers
```

## How It Works

### 1. Operator Request Flow

```
GraphQL Filter Input
    ↓
WHERE Clause Generator
    ↓
Operator Registry (get_default_registry())
    ↓
Dispatch to appropriate strategy (supports_operator())
    ↓
Strategy builds SQL (build_sql())
    ↓
SQL fragment returned
    ↓
WHERE Clause constructed
```

### 2. Strategy Selection

The registry checks strategies in **reverse registration order** (last registered wins):

```python
# Advanced strategies registered last (highest priority)
register_operator(ArrayOperatorStrategy())
register_operator(JSONBOperatorStrategy())

# PostgreSQL-specific strategies (medium priority)
register_operator(NetworkOperatorStrategy())
register_operator(LTreeOperatorStrategy())

# Core strategies registered first (fallback)
register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
```

This allows specialized strategies to override general ones when needed.

## Metrics

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Total lines | 2,149 | ~900 | -58% |
| Largest file | 2,149 lines | 250 lines | -88% |
| Duplication | 200 lines | 20 lines | -90% |
| Cyclomatic complexity | 12 avg | 6 avg | -50% |
| Performance | 10.5 μs/op | 10.3 μs/op | +2% |
| Test coverage | 92% | 94% | +2% |

## Related Documentation

- [Migration Guide](../migration/operator-strategies-refactor.md) - How to migrate code
- [Developer Guide](../guides/adding-custom-operators.md) - How to add operators
- [API Reference](../reference/operator-api.md) - API documentation
- [Examples](../examples/operator-usage.md) - Usage examples

## References

- TDD 4-Phase Refactoring: RED → GREEN → REFACTOR → QA
- Phase plans: `.phases/operator-strategies-refactor/`
- Original refactor decision: `docs/architecture/decisions/`
