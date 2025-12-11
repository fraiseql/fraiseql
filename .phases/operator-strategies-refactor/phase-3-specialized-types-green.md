# Phase 3: Specialized PostgreSQL Types Migration

**Phase:** GREEN (Make Tests Pass)
**Duration:** 6-8 hours
**Risk:** Medium

---

## Objective

**TDD Phase GREEN:** Implement specialized PostgreSQL type operators to make existing tests pass.

Migrate:
- Network type operators (IPv4, IPv6, CIDR, INET)
- LTree hierarchical operators
- DateRange operators
- MAC address operators

These are PostgreSQL-specific types that have specialized operators.

---

## Files to Create

### 1. `src/fraiseql/sql/operators/postgresql/network_operators.py`

```python
"""Network type operator strategies (INET, CIDR, IPv4, IPv6)."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy
from ...where.core.field_detection import looks_like_ip_address


class NetworkOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for PostgreSQL network type operators.

    Supports INET, CIDR types with operators:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - isprivate: Is private network
        - ispublic: Is public network
        - insubnet: Network contains address
        - overlaps: Networks overlap
        - strictleft, strictright: Ordering
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {
        "eq", "neq", "in", "nin",
        "isprivate", "ispublic", "insubnet", "overlaps",
        "strictleft", "strictright",
        "isnull"
    }

    NETWORK_TYPES = {"IPv4Address", "IPv6Address", "IPv4Network", "IPv6Network"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a network operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Check field type
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, '__name__') else str(field_type)
            if any(net_type in type_name for net_type in self.NETWORK_TYPES):
                return True

        # Network-specific operators
        if operator in {"isprivate", "ispublic", "insubnet", "overlaps", "strictleft", "strictright"}:
            return True

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for network operators."""
        # Cast to inet for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::inet").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS inet)").format(path_sql)

        # Equality operators
        if operator == "eq":
            if looks_like_ip_address(str(value)):
                return SQL("{} = {}::inet").format(casted_path, Literal(str(value)))
            return SQL("{} = {}").format(casted_path, Literal(str(value)))

        if operator == "neq":
            if looks_like_ip_address(str(value)):
                return SQL("{} != {}::inet").format(casted_path, Literal(str(value)))
            return SQL("{} != {}").format(casted_path, Literal(str(value)))

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::inet").format(Literal(str(v))) for v in value
            )
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::inet").format(Literal(str(v))) for v in value
            )
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # Network-specific operators
        if operator == "isprivate":
            return SQL("NOT inet_public({})").format(casted_path)

        if operator == "ispublic":
            return SQL("inet_public({})").format(casted_path)

        if operator == "insubnet":
            return SQL("{} <<= {}::inet").format(casted_path, Literal(str(value)))

        if operator == "overlaps":
            return SQL("{} && {}::inet").format(casted_path, Literal(str(value)))

        if operator == "strictleft":
            return SQL("{} << {}::inet").format(casted_path, Literal(str(value)))

        if operator == "strictright":
            return SQL("{} >> {}::inet").format(casted_path, Literal(str(value)))

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 2. `src/fraiseql/sql/operators/postgresql/ltree_operators.py`

```python
"""LTree hierarchical path operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class LTreeOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for PostgreSQL ltree (label tree) operators.

    Supports hierarchical path operators:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - ancestor_of: Is ancestor of path
        - descendant_of: Is descendant of path
        - matches_lquery: Matches lquery pattern
        - matches_ltxtquery: Matches ltxtquery pattern
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {
        "eq", "neq", "in", "nin",
        "ancestor_of", "descendant_of",
        "matches_lquery", "matches_ltxtquery",
        "isnull"
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is an ltree operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # LTree-specific operators always handled by this strategy
        if operator in {"ancestor_of", "descendant_of", "matches_lquery", "matches_ltxtquery"}:
            return True

        # With type hint, check if it's an LTree type
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, '__name__') else str(field_type)
            if "LTree" in type_name or "ltree" in type_name.lower():
                return True

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for ltree operators."""
        # Cast to ltree for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::ltree").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS ltree)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}::ltree").format(casted_path, Literal(str(value)))

        # Hierarchical operators
        if operator == "ancestor_of":
            return SQL("{} @> {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "descendant_of":
            return SQL("{} <@ {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "matches_lquery":
            return SQL("{} ~ {}::lquery").format(casted_path, Literal(str(value)))

        if operator == "matches_ltxtquery":
            return SQL("{} @ {}::ltxtquery").format(casted_path, Literal(str(value)))

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            # For ltree, need to check if any of the values match
            # Use array contains operator
            parts = [SQL("ARRAY[")]
            for i, path in enumerate(value):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(str(path)), SQL("::ltree")])
            parts.extend([SQL("] @> "), casted_path])
            return Composable(parts)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            # Negate the contains check
            parts = [SQL("NOT (ARRAY[")]
            for i, path in enumerate(value):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(str(path)), SQL("::ltree")])
            parts.extend([SQL("] @> "), casted_path, SQL(")")])
            return Composable(parts)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 3. `src/fraiseql/sql/operators/postgresql/daterange_operators.py`

```python
"""DateRange operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class DateRangeOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for PostgreSQL daterange operators.

    Supports range operators:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - contains_date: Range contains specific date
        - overlaps: Ranges overlap
        - adjacent: Ranges are adjacent
        - strictly_left: Range is strictly left of another
        - strictly_right: Range is strictly right of another
        - not_left: Range does not extend left
        - not_right: Range does not extend right
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {
        "eq", "neq", "in", "nin",
        "contains_date", "overlaps", "adjacent",
        "strictly_left", "strictly_right",
        "not_left", "not_right",
        "isnull"
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a daterange operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # DateRange-specific operators
        if operator in {"contains_date", "overlaps", "adjacent",
                       "strictly_left", "strictly_right", "not_left", "not_right"}:
            return True

        # With type hint
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, '__name__') else str(field_type)
            if "DateRange" in type_name or "daterange" in type_name.lower():
                return True

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for daterange operators."""
        # Cast to daterange for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::daterange").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS daterange)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}::daterange").format(casted_path, Literal(str(value)))

        # Range operators
        if operator == "contains_date":
            return SQL("{} @> {}::date").format(casted_path, Literal(str(value)))

        if operator == "overlaps":
            return SQL("{} && {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "adjacent":
            return SQL("{} -|- {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "strictly_left":
            return SQL("{} << {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "strictly_right":
            return SQL("{} >> {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "not_left":
            return SQL("{} &> {}::daterange").format(casted_path, Literal(str(value)))

        if operator == "not_right":
            return SQL("{} &< {}::daterange").format(casted_path, Literal(str(value)))

        # List operators (check if range is in list)
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::daterange").format(Literal(str(v))) for v in value
            )
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::daterange").format(Literal(str(v))) for v in value
            )
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        # Pattern operators explicitly rejected
        if operator in {"contains", "startswith", "endswith"}:
            raise ValueError(
                f"Pattern operator '{operator}' is not supported for DateRange fields. "
                f"Use range operators: contains_date, overlaps, adjacent, strictly_left, "
                f"strictly_right, not_left, not_right, or basic: eq, neq, in, nin, isnull."
            )

        return None
```

### 4. `src/fraiseql/sql/operators/postgresql/macaddr_operators.py`

```python
"""MAC address operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class MacAddressOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for PostgreSQL macaddr/macaddr8 operators.

    Supports:
        - eq, neq: Equality/inequality
        - in, nin: List membership
        - isnull: NULL checking
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "in", "nin", "isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a MAC address operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, '__name__') else str(field_type)
            if "MacAddr" in type_name or "macaddr" in type_name.lower():
                return True

        return False

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for MAC address operators."""
        # Cast to macaddr for JSONB columns
        if jsonb_column:
            casted_path = SQL("({})::macaddr").format(path_sql)
        else:
            casted_path = SQL("CAST({} AS macaddr)").format(path_sql)

        # Equality operators
        if operator == "eq":
            return SQL("{} = {}::macaddr").format(casted_path, Literal(str(value)))

        if operator == "neq":
            return SQL("{} != {}::macaddr").format(casted_path, Literal(str(value)))

        # List operators
        if operator == "in":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::macaddr").format(Literal(str(v))) for v in value
            )
            return SQL("{} IN ({})").format(casted_path, placeholders)

        if operator == "nin":
            if not isinstance(value, (list, tuple)):
                value = [value]
            placeholders = SQL(", ").join(
                SQL("{}::macaddr").format(Literal(str(v))) for v in value
            )
            return SQL("{} NOT IN ({})").format(casted_path, placeholders)

        # NULL checking
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 5. Update `src/fraiseql/sql/operators/postgresql/__init__.py`

```python
"""PostgreSQL-specific operator strategies."""

from .network_operators import NetworkOperatorStrategy
from .ltree_operators import LTreeOperatorStrategy
from .daterange_operators import DateRangeOperatorStrategy
from .macaddr_operators import MacAddressOperatorStrategy

__all__ = [
    "NetworkOperatorStrategy",
    "LTreeOperatorStrategy",
    "DateRangeOperatorStrategy",
    "MacAddressOperatorStrategy",
]
```

---

## Files to Modify

### Update `src/fraiseql/sql/operators/__init__.py`

Register PostgreSQL-specific strategies:

```python
# Import PostgreSQL-specific strategies
from .postgresql import (
    NetworkOperatorStrategy,
    LTreeOperatorStrategy,
    DateRangeOperatorStrategy,
    MacAddressOperatorStrategy,
)

# Register PostgreSQL-specific strategies
register_operator(NetworkOperatorStrategy())
register_operator(LTreeOperatorStrategy())
register_operator(DateRangeOperatorStrategy())
register_operator(MacAddressOperatorStrategy())
```

---

## Verification Commands

```bash
# Run PostgreSQL operator tests
uv run pytest tests/unit/sql/where/operators/test_*.py -v -k "network or ltree or daterange or macaddr"

# Run integration tests for special types
uv run pytest tests/integration/database/sql/test_*_filter_operations.py -v

# Run specific type tests
uv run pytest tests/integration/database/sql/test_network_*.py -v
uv run pytest tests/integration/database/sql/test_ltree_*.py -v
uv run pytest tests/integration/database/sql/test_daterange_*.py -v
uv run pytest tests/integration/database/sql/test_mac_address_*.py -v

# Full WHERE clause test suite
uv run pytest tests/unit/sql/where/ -v
```

---

## Acceptance Criteria

- [ ] Network operator strategy implemented (all network operators working)
- [ ] LTree operator strategy implemented (hierarchical paths working)
- [ ] DateRange operator strategy implemented (range operators working)
- [ ] MacAddress operator strategy implemented (MAC addresses working)
- [ ] All strategies registered
- [ ] All PostgreSQL special type tests passing
- [ ] No regressions in existing tests
- [ ] JSONB-based special types work correctly

---

## DO NOT

- ❌ Change SQL output or operator behavior
- ❌ Delete old operator_strategies.py yet
- ❌ Skip testing any operator type
- ❌ Break JSONB path handling

---

## Next Phase

Once specialized PostgreSQL types are migrated:
→ **Phase 4:** Migrate advanced operators (array, JSONB, fulltext, vector)
