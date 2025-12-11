# Phase 5: Refactor & Optimize - COMPLETE IMPLEMENTATION PLAN

**Phase:** REFACTOR (Improve Code Quality)
**Duration:** 3-4 hours
**Risk:** Low
**Status:** Ready for Execution

---

## Objective

**TDD Phase REFACTOR:** Now that all tests are passing (GREEN phases 1-4 complete), improve code quality without changing behavior.

This phase extracts common patterns, eliminates duplication, optimizes performance, and improves code clarity across all operator strategy modules while maintaining 100% test coverage.

**Critical Rule:** ALL tests must stay GREEN throughout refactoring. Run tests after each extraction.

---

## Context

After analyzing the completed operator strategies, we've identified significant code duplication:

**Duplication Analysis:**
- **7 implementations** of `eq` operator with identical patterns
- **6 implementations** of `in`/`nin` operators with identical list handling
- **7 implementations** of `isnull` operator with identical NULL checking
- **5 implementations** of JSONB casting logic with similar patterns
- **4 implementations** of comparison operators (gt, gte, lt, lte)

**Files Already Implemented:**
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py` - Base class
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/string_operators.py` (165 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/numeric_operators.py` (97 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/boolean_operators.py` (~80 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/network_operators.py` (125 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/ltree_operators.py` (~150 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/daterange_operators.py` (~130 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/macaddr_operators.py` (~100 lines)

---

## Refactoring Opportunities

### 1. Extract Common Casting Logic to Base Class

**Problem:** Every strategy duplicates JSONB casting with slight variations:

```python
# String operators (string_operators.py:78-82)
if jsonb_column:
    casted_path = path_sql
else:
    casted_path = SQL("CAST({} AS TEXT)").format(path_sql)

# Network operators (network_operators.py:74-77)
if jsonb_column:
    casted_path = SQL("({})::inet").format(path_sql)
else:
    casted_path = SQL("CAST({} AS inet)").format(path_sql)

# Numeric operators (numeric_operators.py:49-56)
if jsonb_column:
    if field_type is int or isinstance(value, int):
        casted_path = SQL("({})::integer").format(path_sql)
    else:
        casted_path = SQL("({})::numeric").format(path_sql)
else:
    casted_path = path_sql
```

**Solution:** Add helper method to `BaseOperatorStrategy`:

```python
# File: /home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py

from psycopg.sql import SQL, Composable

class BaseOperatorStrategy(ABC):
    """Abstract base class for all operator strategies."""

    # ... existing methods ...

    def _cast_path(
        self,
        path_sql: Composable,
        target_type: str,
        jsonb_column: Optional[str] = None,
        use_postgres_cast: bool = False
    ) -> Composable:
        """Cast path SQL to specified PostgreSQL type.

        Args:
            path_sql: SQL fragment for accessing the field
            target_type: PostgreSQL type name (e.g., "text", "inet", "integer")
            jsonb_column: JSONB column name if this is JSONB-based
            use_postgres_cast: If True, use ::type syntax, else use CAST(x AS type)

        Returns:
            Casted SQL fragment

        Examples:
            # Text casting for string operators
            _cast_path(path_sql, "text", jsonb_column="data")
            # Result: path_sql (JSONB already text)

            # Network casting for INET operators
            _cast_path(path_sql, "inet", jsonb_column="data", use_postgres_cast=True)
            # Result: (path_sql)::inet

            # Integer casting for numeric operators
            _cast_path(path_sql, "integer", use_postgres_cast=True)
            # Result: (path_sql)::integer
        """
        if jsonb_column:
            # JSONB extracts are already text-like, some need explicit casting
            if target_type.lower() in ("text", "varchar", "char"):
                # Already text, no cast needed
                return path_sql
            else:
                # Need to cast from JSONB-extracted value
                if use_postgres_cast:
                    return SQL("({})::{}").format(path_sql, SQL(target_type))
                else:
                    return SQL("CAST({} AS {})").format(path_sql, SQL(target_type))
        else:
            # Regular column, cast if needed
            if use_postgres_cast:
                return SQL("({})::{}").format(path_sql, SQL(target_type))
            else:
                return SQL("CAST({} AS {})").format(path_sql, SQL(target_type))
```

### 2. Extract Common Comparison Operators

**Problem:** Comparison operators (eq, neq, gt, gte, lt, lte) are duplicated across strategies.

**Solution:** Add comparison mixin to base class:

```python
# File: /home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py

class BaseOperatorStrategy(ABC):
    """Abstract base class for all operator strategies."""

    # ... existing methods ...

    def _build_comparison(
        self,
        operator: str,
        casted_path: Composable,
        value: Any
    ) -> Optional[Composable]:
        """Build SQL for common comparison operators.

        Args:
            operator: One of: eq, neq, gt, gte, lt, lte
            casted_path: Already-casted path SQL
            value: Comparison value

        Returns:
            SQL comparison fragment, or None if operator not supported

        Examples:
            casted = _cast_path(path_sql, "integer", jsonb_column="data")
            sql = _build_comparison("gt", casted, 42)
            # Result: casted_path > 42
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
```

### 3. Extract Common List Operators (IN/NOT IN)

**Problem:** IN and NOT IN operators duplicated 6+ times.

**Solution:** Add list operator helper to base class:

```python
# File: /home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py

class BaseOperatorStrategy(ABC):
    """Abstract base class for all operator strategies."""

    # ... existing methods ...

    def _build_in_operator(
        self,
        casted_path: Composable,
        value: Any,
        negate: bool = False,
        cast_values: Optional[str] = None
    ) -> Composable:
        """Build SQL for IN or NOT IN operators.

        Args:
            casted_path: Already-casted path SQL
            value: List of values (will be normalized to list if single value)
            negate: If True, use NOT IN, else use IN
            cast_values: Optional PostgreSQL type to cast each value

        Returns:
            SQL IN/NOT IN fragment

        Examples:
            # Simple IN
            casted = _cast_path(path_sql, "text")
            sql = _build_in_operator(casted, ["foo", "bar"])
            # Result: casted_path IN ('foo', 'bar')

            # NOT IN with value casting
            casted = _cast_path(path_sql, "inet")
            sql = _build_in_operator(casted, ["192.168.1.1"], negate=True, cast_values="inet")
            # Result: casted_path NOT IN ('192.168.1.1'::inet)
        """
        # Normalize to list
        if not isinstance(value, (list, tuple)):
            value = [value]

        # Build placeholder list
        if cast_values:
            placeholders = SQL(", ").join(
                SQL("{}::{}").format(Literal(v), SQL(cast_values)) for v in value
            )
        else:
            placeholders = SQL(", ").join(Literal(v) for v in value)

        # Build IN or NOT IN
        if negate:
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)
        else:
            return SQL("{} IN ({})").format(casted_path, placeholders)
```

### 4. Extract Common NULL Checking

**Problem:** `isnull` operator duplicated 7 times identically.

**Solution:** Add NULL checking helper to base class:

```python
# File: /home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py

class BaseOperatorStrategy(ABC):
    """Abstract base class for all operator strategies."""

    # ... existing methods ...

    def _build_null_check(
        self,
        path_sql: Composable,
        value: Any
    ) -> Composable:
        """Build SQL for IS NULL / IS NOT NULL checks.

        Args:
            path_sql: Original path SQL (NOT casted - NULL checks don't need casting)
            value: Boolean indicating if checking for NULL (True) or NOT NULL (False)

        Returns:
            SQL IS NULL or IS NOT NULL fragment

        Examples:
            sql = _build_null_check(path_sql, True)
            # Result: path_sql IS NULL

            sql = _build_null_check(path_sql, False)
            # Result: path_sql IS NOT NULL
        """
        if value:
            return SQL("{} IS NULL").format(path_sql)
        else:
            return SQL("{} IS NOT NULL").format(path_sql)
```

### 5. Simplify Strategy Implementations

**Before (string_operators.py:84-89):**
```python
if operator == "eq":
    return SQL("{} = {}").format(casted_path, Literal(str(value)))

if operator == "neq":
    return SQL("{} != {}").format(casted_path, Literal(str(value)))
```

**After:**
```python
# Comparison operators (eq, neq, gt, gte, lt, lte)
casted_path = self._cast_path(path_sql, "text", jsonb_column)
comparison_sql = self._build_comparison(operator, casted_path, str(value))
if comparison_sql is not None:
    return comparison_sql
```

**Before (network_operators.py:87-97):**
```python
if operator == "in":
    if not isinstance(value, (list, tuple)):
        value = [value]
    placeholders = SQL(", ").join(SQL("{}::inet").format(Literal(str(v))) for v in value)
    return SQL("{} IN ({})").format(casted_path, placeholders)

if operator == "nin":
    if not isinstance(value, (list, tuple)):
        value = [value]
    placeholders = SQL(", ").join(SQL("{}::inet").format(Literal(str(v))) for v in value)
    return SQL("{} NOT IN ({})").format(casted_path, placeholders)
```

**After:**
```python
# List operators
if operator == "in":
    casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
    return self._build_in_operator(casted_path, value, cast_values="inet")

if operator == "nin":
    casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
    return self._build_in_operator(casted_path, value, negate=True, cast_values="inet")
```

---

## Implementation Steps

### Step 1: Add Helper Methods to Base Class (1 hour)

**File:** `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py`

**Actions:**
1. Import `Optional` from typing (if not already imported)
2. Import `Literal` from `psycopg.sql` (if not already imported)
3. Add `_cast_path()` method to `BaseOperatorStrategy` class
4. Add `_build_comparison()` method to `BaseOperatorStrategy` class
5. Add `_build_in_operator()` method to `BaseOperatorStrategy` class
6. Add `_build_null_check()` method to `BaseOperatorStrategy` class

**Verification after this step:**
```bash
# Verify base class still works
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/test_base_strategy.py -v

# Check imports
python3 -c "from fraiseql.sql.operators.base import BaseOperatorStrategy; print('Base class OK')"
```

### Step 2: Refactor String Operators (30 min)

**File:** `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/string_operators.py`

**Current duplication count:**
- Lines 78-82: JSONB casting logic
- Lines 85-89: eq/neq operators
- Lines 146-156: in/nin operators
- Lines 159-162: isnull operator

**Refactoring:**

```python
def build_sql(
    self,
    operator: str,
    value: Any,
    path_sql: Composable,
    field_type: Optional[type] = None,
    jsonb_column: Optional[str] = None,
) -> Optional[Composable]:
    """Build SQL for string operators."""

    # Simple operators that use base class helpers
    if operator in ("eq", "neq", "gt", "gte", "lt", "lte"):
        casted_path = self._cast_path(path_sql, "text", jsonb_column)
        return self._build_comparison(operator, casted_path, str(value))

    if operator == "in":
        casted_path = self._cast_path(path_sql, "text", jsonb_column)
        return self._build_in_operator(casted_path, [str(v) for v in value])

    if operator == "nin":
        casted_path = self._cast_path(path_sql, "text", jsonb_column)
        return self._build_in_operator(casted_path, [str(v) for v in value], negate=True)

    if operator == "isnull":
        return self._build_null_check(path_sql, value)

    # Pattern matching operators (keep as-is, they're string-specific)
    casted_path = self._cast_path(path_sql, "text", jsonb_column)

    if operator == "contains":
        # ... existing logic ...

    # ... rest of string-specific operators ...
```

**Verification:**
```bash
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/core/test_string_operators.py -v
```

**Expected:** All tests pass, no regressions.

### Step 3: Refactor Numeric Operators (30 min)

**File:** `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/numeric_operators.py`

**Current duplication count:**
- Lines 49-56: JSONB numeric casting
- Lines 59-75: Comparison operators (eq, neq, gt, gte, lt, lte)
- Lines 78-88: in/nin operators
- Lines 91-94: isnull operator

**Refactoring:**

```python
def build_sql(
    self,
    operator: str,
    value: Any,
    path_sql: Composable,
    field_type: Optional[type] = None,
    jsonb_column: Optional[str] = None,
) -> Optional[Composable]:
    """Build SQL for numeric operators."""

    # Determine numeric cast type
    if jsonb_column:
        # JSONB numeric values need casting
        cast_type = "integer" if (field_type is int or isinstance(value, int)) else "numeric"
        casted_path = self._cast_path(path_sql, cast_type, jsonb_column, use_postgres_cast=True)
    else:
        casted_path = path_sql

    # Comparison operators
    comparison_sql = self._build_comparison(operator, casted_path, value)
    if comparison_sql is not None:
        return comparison_sql

    # List operators
    if operator == "in":
        return self._build_in_operator(casted_path, value)

    if operator == "nin":
        return self._build_in_operator(casted_path, value, negate=True)

    # NULL checking
    if operator == "isnull":
        return self._build_null_check(path_sql, value)

    return None
```

**Verification:**
```bash
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/core/test_numeric_operators.py -v
```

**Expected:** All tests pass.

### Step 4: Refactor Boolean Operators (20 min)

**File:** `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/boolean_operators.py`

**Refactoring:** Similar pattern - use `_build_comparison()` and `_build_null_check()`.

**Verification:**
```bash
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/core/test_boolean_operators.py -v
```

### Step 5: Refactor Network Operators (30 min)

**File:** `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/network_operators.py`

**Current duplication count:**
- Lines 74-77: JSONB inet casting
- Lines 80-84: eq/neq operators
- Lines 87-97: in/nin operators with inet casting
- Lines 119-122: isnull operator

**Refactoring:**

```python
def build_sql(
    self,
    operator: str,
    value: Any,
    path_sql: Composable,
    field_type: Optional[type] = None,
    jsonb_column: Optional[str] = None,
) -> Optional[Composable]:
    """Build SQL for network operators."""

    # Comparison operators
    if operator in ("eq", "neq"):
        casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
        # Convert value to string for inet casting
        return self._build_comparison(operator, casted_path, str(value))

    # List operators
    if operator == "in":
        casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
        return self._build_in_operator(casted_path, [str(v) for v in value], cast_values="inet")

    if operator == "nin":
        casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)
        return self._build_in_operator(casted_path, [str(v) for v in value], negate=True, cast_values="inet")

    # NULL checking
    if operator == "isnull":
        return self._build_null_check(path_sql, value)

    # Network-specific operators (keep as-is)
    casted_path = self._cast_path(path_sql, "inet", jsonb_column, use_postgres_cast=True)

    if operator == "isprivate":
        return SQL("NOT inet_public({})").format(casted_path)

    # ... rest of network-specific operators ...
```

**Verification:**
```bash
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/postgresql/ -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_network_*.py -v
```

### Step 6: Refactor Remaining PostgreSQL Operators (30 min)

**Files to refactor:**
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/ltree_operators.py`
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/daterange_operators.py`
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/macaddr_operators.py`

**Pattern:** Same refactoring approach - extract common operators.

**Verification:**
```bash
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/postgresql/ -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_ltree_*.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_daterange_*.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_mac_*.py -v
```

### Step 7: Performance Optimization Pass (30 min)

**Goal:** Identify and optimize hot paths.

**Actions:**

1. **Cache SQL operator strings:**

```python
# File: /home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py

# Add at module level
_SQL_OPERATORS = {
    "eq": SQL(" = "),
    "neq": SQL(" != "),
    "gt": SQL(" > "),
    "gte": SQL(" >= "),
    "lt": SQL(" < "),
    "lte": SQL(" <= "),
}

class BaseOperatorStrategy(ABC):
    def _build_comparison(self, operator: str, casted_path: Composable, value: Any) -> Optional[Composable]:
        """Build SQL for common comparison operators (optimized)."""
        op_sql = _SQL_OPERATORS.get(operator)
        if op_sql is None:
            return None

        # Use pre-built SQL fragments to avoid repeated SQL() construction
        return SQL("{}{}{}").format(casted_path, op_sql, Literal(value))
```

2. **Optimize list normalization:**

```python
def _build_in_operator(self, casted_path: Composable, value: Any, negate: bool = False, cast_values: Optional[str] = None) -> Composable:
    """Build SQL for IN or NOT IN operators (optimized)."""
    # Normalize to list (avoid list() call overhead for already-lists)
    if isinstance(value, list):
        value_list = value
    elif isinstance(value, tuple):
        value_list = list(value)
    else:
        value_list = [value]

    # Build placeholders efficiently
    if cast_values:
        cast_sql = SQL("::{}").format(SQL(cast_values))
        placeholders = SQL(", ").join(
            SQL("{}{}").format(Literal(v), cast_sql) for v in value_list
        )
    else:
        placeholders = SQL(", ").join(Literal(v) for v in value_list)

    # Use cached SQL fragments
    if negate:
        return SQL("{} NOT IN ({})").format(casted_path, placeholders)
    else:
        return SQL("{} IN ({})").format(casted_path, placeholders)
```

**Verification:**
```bash
# Run performance benchmarks (if they exist)
uv run pytest /home/lionel/code/fraiseql/tests/benchmarks/ -v 2>/dev/null || echo "No benchmark tests found"

# Run full test suite to ensure no regressions
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/ -v
```

### Step 8: Code Quality Improvements (20 min)

**Actions:**

1. **Add strategic comments to base class helpers:**

```python
def _cast_path(self, path_sql: Composable, target_type: str, jsonb_column: Optional[str] = None, use_postgres_cast: bool = False) -> Composable:
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
```

2. **Improve variable names:**

```python
# BEFORE
def _build_comparison(self, operator: str, casted_path: Composable, value: Any):
    if operator == "eq":
        return SQL("{} = {}").format(casted_path, Literal(value))

# AFTER (more descriptive)
def _build_comparison(self, operator: str, casted_path: Composable, filter_value: Any):
    """Build SQL comparison expression."""
    if operator == "eq":
        value_literal = Literal(filter_value)
        return SQL("{} = {}").format(casted_path, value_literal)
```

3. **Run code quality checks:**

```bash
# Linting
ruff check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/ --fix

# Formatting
ruff format /home/lionel/code/fraiseql/src/fraiseql/sql/operators/

# Check complexity (should be lower after refactoring)
radon cc /home/lionel/code/fraiseql/src/fraiseql/sql/operators/ -a
```

---

## Verification Commands

Run after **each step** to ensure tests stay GREEN:

```bash
# Quick verification (unit tests only, ~10 seconds)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/ -v

# Full verification (includes integration tests, ~1-2 minutes)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/where/ -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/ -v

# Specific operator family verification
uv run pytest /home/lionel/code/fraiseql/tests/ -k "string" -v
uv run pytest /home/lionel/code/fraiseql/tests/ -k "numeric" -v
uv run pytest /home/lionel/code/fraiseql/tests/ -k "network" -v
uv run pytest /home/lionel/code/fraiseql/tests/ -k "ltree" -v

# Code quality checks
ruff check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/
ruff format --check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/
```

**Expected results after refactoring:**
- All tests passing (same count as before refactoring)
- No new lint errors
- Lower cyclomatic complexity scores
- Reduced line count in strategy files (20-30% reduction expected)

---

## Metrics

**Before Refactoring:**
- Total operator strategy lines: ~900 lines (estimated across 8 files)
- Duplication: ~200 lines of duplicated code
- Average complexity per file: 8-12

**After Refactoring (Expected):**
- Total operator strategy lines: ~650 lines (30% reduction)
- Duplication: ~20 lines (90% reduction)
- Average complexity per file: 5-8 (40% improvement)

**Files modified in this phase:**
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py` (add 120 lines of helpers)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/string_operators.py` (reduce by ~40 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/numeric_operators.py` (reduce by ~30 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/core/boolean_operators.py` (reduce by ~20 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/network_operators.py` (reduce by ~30 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/ltree_operators.py` (reduce by ~25 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/daterange_operators.py` (reduce by ~25 lines)
- `/home/lionel/code/fraiseql/src/fraiseql/sql/operators/postgresql/macaddr_operators.py` (reduce by ~20 lines)

---

## Acceptance Criteria

- [ ] `_cast_path()` method added to `BaseOperatorStrategy`
- [ ] `_build_comparison()` method added to `BaseOperatorStrategy`
- [ ] `_build_in_operator()` method added to `BaseOperatorStrategy`
- [ ] `_build_null_check()` method added to `BaseOperatorStrategy`
- [ ] String operators refactored to use base class helpers
- [ ] Numeric operators refactored to use base class helpers
- [ ] Boolean operators refactored to use base class helpers
- [ ] Network operators refactored to use base class helpers
- [ ] LTree operators refactored to use base class helpers
- [ ] DateRange operators refactored to use base class helpers
- [ ] MacAddress operators refactored to use base class helpers
- [ ] All 4,943+ tests still passing (zero new failures)
- [ ] Code duplication reduced by 80%+ (measured by lines)
- [ ] Average cyclomatic complexity reduced
- [ ] All files pass `ruff check` with zero errors
- [ ] All files pass `ruff format --check` (consistent formatting)
- [ ] Performance same or better (no regressions in hot paths)

---

## DO NOT

- ❌ Change any operator behavior or SQL output
- ❌ Break any tests (all tests must stay GREEN)
- ❌ Add new features (this is REFACTOR only, not enhancement)
- ❌ Over-engineer abstractions (keep it simple)
- ❌ Refactor advanced operators (Phase 4 not yet complete)
- ❌ Delete old `operator_strategies.py` (that's Phase 7)
- ❌ Change public API (backward compatibility required)

---

## Rollback Plan

If refactoring breaks tests:

```bash
# Rollback specific file
git checkout HEAD -- /home/lionel/code/fraiseql/src/fraiseql/sql/operators/base.py

# Rollback entire operators directory
git checkout HEAD -- /home/lionel/code/fraiseql/src/fraiseql/sql/operators/

# Verify tests pass after rollback
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/ -v
```

**Prevention:** Commit after each successful step (Step 1 → commit, Step 2 → commit, etc.)

---

## Commit Strategy

**Commit after each major step:**

```bash
# After Step 1 (base class helpers)
git add src/fraiseql/sql/operators/base.py
git commit -m "refactor(operators): add common helpers to BaseOperatorStrategy [REFACTOR]

Add reusable helper methods to eliminate duplication:
- _cast_path(): Handle JSONB vs regular column casting
- _build_comparison(): Common comparison operators (eq, neq, gt, gte, lt, lte)
- _build_in_operator(): IN/NOT IN with value casting
- _build_null_check(): IS NULL/IS NOT NULL

Tests: All passing (base class tests only)"

# After Step 2 (string operators)
git add src/fraiseql/sql/operators/core/string_operators.py
git commit -m "refactor(operators): simplify StringOperatorStrategy using base helpers [REFACTOR]

Reduce duplication by using base class methods:
- Use _cast_path() for text casting
- Use _build_comparison() for eq/neq
- Use _build_in_operator() for in/nin
- Use _build_null_check() for isnull

Line reduction: 165 → 130 lines (-35 lines, -21%)
Tests: All string operator tests passing"

# After Steps 3-6 (all operators refactored)
git add src/fraiseql/sql/operators/
git commit -m "refactor(operators): refactor all operator strategies to use base helpers [REFACTOR]

Apply base class helper methods across all strategies:
- NumericOperatorStrategy: 97 → 70 lines (-28%)
- BooleanOperatorStrategy: 80 → 60 lines (-25%)
- NetworkOperatorStrategy: 125 → 95 lines (-24%)
- LTreeOperatorStrategy: 150 → 115 lines (-23%)
- DateRangeOperatorStrategy: 130 → 100 lines (-23%)
- MacAddressOperatorStrategy: 100 → 75 lines (-25%)

Total duplication eliminated: ~190 lines (80% reduction)
Tests: All 4,943+ tests passing, zero regressions"

# After Step 7 (performance optimization)
git add src/fraiseql/sql/operators/base.py
git commit -m "perf(operators): optimize SQL fragment construction [REFACTOR]

Performance improvements:
- Cache SQL operator strings (avoid repeated SQL() construction)
- Optimize list normalization in _build_in_operator()
- Use :: cast syntax for hot paths (faster than CAST())

Tests: All passing, no behavioral changes"

# After Step 8 (code quality)
git add src/fraiseql/sql/operators/
git commit -m "refactor(operators): improve code quality and documentation [REFACTOR]

Code quality improvements:
- Add strategic comments to helper methods
- Improve variable names for clarity
- Run ruff format across all operator modules
- All files pass linting

Tests: All passing"
```

---

## Next Phase

Once refactoring is complete and all tests are GREEN:

→ **Phase 6:** Quality Assurance & Integration (`/tmp/phase-6-qa-COMPLETE.md`)

**Prerequisites for Phase 6:**
- All acceptance criteria met ✅
- All tests passing ✅
- Code quality metrics improved ✅
- Changes committed ✅

---

## Notes

**Why refactor now?**
- Patterns have emerged across Phases 1-3 implementations
- Duplication is clear and measurable
- Tests provide safety net for refactoring
- Easier to refactor before adding more complexity (Phase 4 advanced operators)

**What about Phase 4 operators?**
- Phase 4 (advanced operators) not yet implemented
- This refactoring will make Phase 4 implementation easier
- Advanced operators (array, JSONB, fulltext, vector) can use base helpers from start

**Performance considerations:**
- SQL composition is already fast (psycopg.sql is well-optimized)
- Main benefit of refactoring is maintainability, not performance
- Performance optimizations in Step 7 are minor (~5% improvement expected)
- No performance regressions acceptable (verify with benchmarks if available)
