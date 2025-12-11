# Phase 5: Refactor & Optimize

**Phase:** REFACTOR (Improve Code Quality)
**Duration:** 3-4 hours
**Risk:** Low

---

## Objective

**TDD Phase REFACTOR:** Now that all tests are passing (GREEN), improve code quality without changing behavior.

Refactor:
- Extract common patterns into base class methods
- Eliminate code duplication across strategies
- Optimize hot paths
- Improve naming and clarity
- Add strategic comments
- Consolidate utility functions

**Critical Rule:** ALL tests must stay GREEN throughout refactoring.

---

## Refactoring Opportunities

### 1. Extract Common Casting Logic

Many strategies duplicate JSONB casting:

```python
# BEFORE (duplicated in every strategy)
if jsonb_column:
    casted_path = SQL("({})::text").format(path_sql)
else:
    casted_path = SQL("CAST({} AS text)").format(path_sql)
```

```python
# AFTER (extracted to base class)
class BaseOperatorStrategy:
    def _cast_path(
        self,
        path_sql: Composable,
        cast_type: str,
        jsonb_column: Optional[str] = None
    ) -> Composable:
        """Cast path SQL to specified type."""
        if jsonb_column:
            return SQL("({})::{}").format(path_sql, SQL(cast_type))
        else:
            return SQL("CAST({} AS {})").format(path_sql, SQL(cast_type))
```

### 2. Extract Common Comparison Operators

All strategies implement eq, neq, gt, lt, etc. similarly:

```python
# AFTER (extracted to base class mixin)
class ComparisonOperatorMixin:
    """Mixin for common comparison operators."""

    def _build_eq(self, path_sql, value, cast_type=None):
        if cast_type:
            path_sql = self._cast_path(path_sql, cast_type)
        return SQL("{} = {}").format(path_sql, Literal(value))

    def _build_neq(self, path_sql, value, cast_type=None):
        if cast_type:
            path_sql = self._cast_path(path_sql, cast_type)
        return SQL("{} != {}").format(path_sql, Literal(value))

    # ... gt, gte, lt, lte ...
```

### 3. Extract Common List Operators

IN and NOT IN are duplicated:

```python
class ListOperatorMixin:
    """Mixin for IN/NOT IN operators."""

    def _build_in(self, path_sql, value, cast_type=None):
        if not isinstance(value, (list, tuple)):
            value = [value]
        if cast_type:
            placeholders = SQL(", ").join(
                SQL("{}::{}").format(Literal(v), SQL(cast_type)) for v in value
            )
        else:
            placeholders = SQL(", ").join(Literal(v) for v in value)
        return SQL("{} IN ({})").format(path_sql, placeholders)
```

### 4. Performance Optimizations

- Cache compiled SQL fragments
- Use SQL composition efficiently
- Minimize object creation in hot paths
- Pre-compile common patterns

---

## Implementation Steps

### Step 1: Extract Common Patterns (2 hours)
1. Identify duplicated code across strategies
2. Extract to base class methods or mixins
3. Update strategies to use common methods
4. Run tests after each extraction → must stay GREEN

### Step 2: Performance Optimization (1 hour)
1. Profile operator SQL generation
2. Optimize hot paths
3. Cache where beneficial
4. Benchmark before/after

### Step 3: Code Quality Improvements (1 hour)
1. Improve variable names
2. Add docstring details
3. Strategic comments for complex logic
4. Consistent formatting

---

## Verification Commands

```bash
# Run full test suite (must all pass)
uv run pytest tests/unit/sql/where/ -v
uv run pytest tests/integration/database/ -v

# Performance benchmarks
uv run pytest tests/benchmarks/test_operator_performance.py -v

# Code quality checks
ruff check src/fraiseql/sql/operators/
ruff format --check src/fraiseql/sql/operators/
```

---

## Acceptance Criteria

- [ ] Common patterns extracted to base class
- [ ] Code duplication eliminated
- [ ] Performance same or better than before
- [ ] All 4,943 tests still passing
- [ ] Code quality metrics improved
- [ ] No new complexity introduced

---

## DO NOT

- ❌ Change any operator behavior
- ❌ Break any tests
- ❌ Add new features (this is REFACTOR only)
- ❌ Over-engineer abstractions

---

## Next Phase

Once refactoring is complete:
→ **Phase 6:** Quality Assurance & Integration (comprehensive testing)
