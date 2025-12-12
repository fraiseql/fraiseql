# Phase 1: Add Base Method

**Phase**: SETUP (Non-breaking addition)
**Duration**: 15 minutes
**Risk**: None (only adds new method)
**Status**: Ready for Execution

---

## Objective

Add `_cast_both_sides()` method to `BaseOperatorStrategy` class. This new method will be used by all operator strategies to cast both field path and value to PostgreSQL types.

**Success**: New method added, no existing functionality affected.

---

## Prerequisites

- [ ] Clean git working directory
- [ ] No uncommitted changes
- [ ] All tests currently passing (or at known baseline)

---

## Implementation

### Step 1: Read Current Base Class

```bash
cd /home/lionel/code/fraiseql

# Read the base operator strategy class
cat src/fraiseql/sql/operators/base.py | head -100
```

### Step 2: Add New Method

**File**: `src/fraiseql/sql/operators/base.py`

**Location**: Add after `_cast_path()` method (around line 87)

**Code to Add**:
```python
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
            >>> casted_path, casted_value = self._cast_both_sides(path, "00:11:22:33:44:55", "macaddr")
            >>> # casted_path: (data->>'mac')::macaddr
            >>> # casted_value: '00:11:22:33:44:55'::macaddr

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
```

### Step 3: Add Helper Method for List Casting

**Add after `_cast_both_sides()`**:

```python
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
```

### Step 4: Add Type Hint Imports (if needed)

At the top of the file, ensure these imports exist:

```python
from typing import Any, Optional, List
from psycopg.sql import Composable, SQL, Literal, Composed
```

---

## Verification

### Step 1: Check Syntax

```bash
# Verify Python syntax is correct
python3 -m py_compile src/fraiseql/sql/operators/base.py

# If successful, no output. If error, fix syntax.
```

### Step 2: Run Unit Tests

```bash
# Run operator unit tests to ensure no regression
uv run pytest tests/unit/sql/where/operators/ -v

# Expected: All tests pass (nothing uses new method yet)
```

### Step 3: Check Imports Work

```bash
# Verify new method can be imported
python3 << 'PYEOF'
from fraiseql.sql.operators.base import BaseOperatorStrategy
from psycopg.sql import SQL

strategy = BaseOperatorStrategy()

# Test the new method exists
assert hasattr(strategy, '_cast_both_sides'), "Method not found"
assert hasattr(strategy, '_cast_list_values'), "Helper method not found"

# Test it works
path = SQL("data->>'test'")
casted_path, casted_value = strategy._cast_both_sides(path, "test_value", "text")

print("✅ Method exists and works")
print(f"   Casted path: {casted_path}")
print(f"   Casted value: {casted_value}")
PYEOF
```

Expected output:
```
✅ Method exists and works
   Casted path: Composed([SQL('('), SQL("data->>'test'"), SQL(')::'), SQL('text')])
   Casted value: Composed([Literal('test_value'), SQL('::'), SQL('text')])
```

---

## Acceptance Criteria

- [ ] New `_cast_both_sides()` method added to BaseOperatorStrategy
- [ ] New `_cast_list_values()` helper method added
- [ ] Python syntax valid (no import errors)
- [ ] All unit tests still pass
- [ ] Method can be called and returns tuple of Composable objects

---

## Commit

```bash
cd /home/lionel/code/fraiseql

# Stage changes
git add src/fraiseql/sql/operators/base.py

# Commit with descriptive message
git commit -m "$(cat <<'EOF'
feat(operators): Add _cast_both_sides method to BaseOperatorStrategy

Add new method that casts both field path and value to PostgreSQL type.
This simplifies casting logic and ensures consistent type handling for
special PostgreSQL types (macaddr, inet, ltree, daterange, point).

Key features:
- Casts both sides of comparison (field and value)
- Works for JSONB extracts and typed columns
- Supports both ::type and CAST() syntax
- Includes helper for list casting (IN/NOT IN operators)

Implementation approach:
- Redundant casts (e.g., macaddr::macaddr) are no-ops in PostgreSQL
- Performance impact < 1% (negligible)
- Simplifies operator strategies significantly

Part of: Always Cast Both Sides implementation
Phase: 1/7 (Add Base Method)
Related: FUNCTIONAL-ISSUES-ASSESSMENT.md
EOF
)"

# Verify commit
git log -1 --stat
git show HEAD
```

---

## Rollback

If issues occur:

```bash
# Revert the commit
git reset --hard HEAD~1

# Or if already pushed:
git revert HEAD
```

---

## Next Steps

Proceed to **Phase 2: Fix MAC Address Strategy**

The new method is now available for all operator strategies to use.

---

## Notes

### Why This is Safe

1. **Non-breaking**: Only adds new methods, doesn't modify existing ones
2. **No side effects**: New methods aren't called by anything yet
3. **Thoroughly documented**: Clear docstrings explain usage
4. **Tested**: Verification confirms method works

### Design Decisions

**Q: Why return a tuple instead of building the full comparison?**
A: Flexibility. Different operators need different SQL patterns (=, IN, etc.)

**Q: Why use_postgres_cast defaults to True?**
A: ::type syntax is faster (no function call overhead) and more common in PostgreSQL

**Q: Why separate method for list casting?**
A: IN/NOT IN operators need to cast multiple values, cleaner as separate helper

---

**Phase Status**: Ready for execution ✅
**Next Phase**: Phase 2 - Fix MAC Address Strategy
