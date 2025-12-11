# Phase 4: Advanced Operators Migration - COMPLETE IMPLEMENTATION PLAN

**Phase:** GREEN (Make Tests Pass)
**Duration:** 6-8 hours (consider splitting into 4a: Advanced + 4b: Fallback if needed)
**Risk:** Medium-High
**Status:** ✅ COMPLETED

---

## Objective

**TDD Phase GREEN:** Implement advanced operator strategies to make all remaining tests pass.

Migrate:
- Array operators (JSONB array operations)
- JSONB operators (overlaps, strictly_contains)
- Coordinate operators (POINT type with distance calculations)
- Null operator (isnull)
- Comparison operators (fallback for basic eq/neq/gt/gte/lt/lte)
- Pattern matching operators (fallback for contains/startswith/endswith)
- List operators (fallback for in/notin)
- Path operators (for generic path/tree operations)

These are the remaining unmigrated operators from `operator_strategies.py` that haven't been covered by Phases 2-3.

---

## Files to Create

### 1. `src/fraiseql/sql/operators/array/__init__.py`

```python
"""Array operator strategies."""

from .array_operators import ArrayOperatorStrategy

__all__ = ["ArrayOperatorStrategy"]
```

### 2. `src/fraiseql/sql/operators/array/array_operators.py`

```python
"""Array operator strategies for JSONB array fields."""

from typing import Any, Optional, get_origin
import json
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class ArrayOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for JSONB array field operators.

    Supports:
        - eq, neq: Array equality/inequality
        - contains: Array contains elements (@> operator)
        - contained_by: Array is contained by another (<@ operator)
        - overlaps: Arrays have common elements (?| operator)
        - len_eq, len_neq, len_gt, len_gte, len_lt, len_lte: Length operations
        - any_eq: Any element equals value
        - all_eq: All elements equal value
    """

    SUPPORTED_OPERATORS = {
        "eq", "neq", "contains", "contained_by", "overlaps",
        "len_eq", "len_neq", "len_gt", "len_gte", "len_lt", "len_lte",
        "any_eq", "all_eq"
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is an array operator on an array field."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Only handle array operations when field_type indicates array
        if field_type is None:
            return False

        # Check if field_type is a list type (e.g., list[str], List[int])
        return get_origin(field_type) is list

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for array operators."""
        # Array operations work directly on JSONB arrays
        # path_sql is already the JSONB path (e.g., data->'tags')

        # Equality/inequality - compare JSONB arrays directly
        if operator == "eq":
            json_str = json.dumps(value)
            return SQL("{} = {}::jsonb").format(path_sql, Literal(json_str))

        if operator == "neq":
            json_str = json.dumps(value)
            return SQL("{} != {}::jsonb").format(path_sql, Literal(json_str))

        # Containment operators
        if operator == "contains":
            # @> operator: left_array @> right_array means left contains right
            json_str = json.dumps(value)
            return SQL("{} @> {}::jsonb").format(path_sql, Literal(json_str))

        if operator == "contained_by":
            # <@ operator: left_array <@ right_array means left is contained by right
            json_str = json.dumps(value)
            return SQL("{} <@ {}::jsonb").format(path_sql, Literal(json_str))

        if operator == "overlaps":
            # ?| operator: check if arrays have any elements in common
            if isinstance(value, list):
                # Build ARRAY['item1', 'item2', ...] syntax
                array_elements = [str(item) for item in value]
                array_str = "{" + ",".join(f'"{elem}"' for elem in array_elements) + "}"
                return SQL("{} ?| {}").format(path_sql, Literal(array_str))
            json_str = json.dumps(value)
            return SQL("{} ?| {}").format(path_sql, Literal(json_str))

        # Length operations using jsonb_array_length()
        if operator == "len_eq":
            return SQL("jsonb_array_length({}) = {}").format(path_sql, Literal(value))

        if operator == "len_neq":
            return SQL("jsonb_array_length({}) != {}").format(path_sql, Literal(value))

        if operator == "len_gt":
            return SQL("jsonb_array_length({}) > {}").format(path_sql, Literal(value))

        if operator == "len_gte":
            return SQL("jsonb_array_length({}) >= {}").format(path_sql, Literal(value))

        if operator == "len_lt":
            return SQL("jsonb_array_length({}) < {}").format(path_sql, Literal(value))

        if operator == "len_lte":
            return SQL("jsonb_array_length({}) <= {}").format(path_sql, Literal(value))

        # Element query operations using jsonb_array_elements_text
        if operator == "any_eq":
            # Check if any element in the array equals the value
            return Composable([
                SQL("EXISTS (SELECT 1 FROM jsonb_array_elements_text("),
                path_sql,
                SQL(") AS elem WHERE elem = "),
                Literal(value),
                SQL(")")
            ])

        if operator == "all_eq":
            # Check if all elements in the array equal the value
            # This means: array_length = count of elements that equal the value
            return Composable([
                SQL("jsonb_array_length("),
                path_sql,
                SQL(") = (SELECT COUNT(*) FROM jsonb_array_elements_text("),
                path_sql,
                SQL(") AS elem WHERE elem = "),
                Literal(value),
                SQL(")")
            ])

        return None
```

### 3. `src/fraiseql/sql/operators/advanced/__init__.py`

```python
"""Advanced operator strategies."""

from .jsonb_operators import JsonbOperatorStrategy
from .coordinate_operators import CoordinateOperatorStrategy

__all__ = [
    "JsonbOperatorStrategy",
    "CoordinateOperatorStrategy",
]
```

### 4. `src/fraiseql/sql/operators/advanced/jsonb_operators.py`

```python
"""JSONB-specific operator strategies."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class JsonbOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for JSONB-specific operators.

    Supports:
        - overlaps: JSONB objects/arrays overlap (&&)
        - strictly_contains: JSONB strictly contains (@> but not equal)
    """

    SUPPORTED_OPERATORS = {"overlaps", "strictly_contains"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a JSONB-specific operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # These are JSONB-specific operators - handle only when we know it's JSONB
        # In practice, these operators are only used with JSONB fields
        return True

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for JSONB operators."""
        if operator == "overlaps":
            # && operator: check if JSONB objects/arrays overlap
            return SQL("{} && {}").format(path_sql, Literal(value))

        if operator == "strictly_contains":
            # @> operator but exclude exact equality
            # Means: contains the value AND is not equal to the value
            return Composable([
                path_sql,
                SQL(" @> "),
                Literal(value),
                SQL(" AND "),
                path_sql,
                SQL(" != "),
                Literal(value)
            ])

        return None
```

### 5. `src/fraiseql/sql/operators/advanced/coordinate_operators.py`

```python
"""Coordinate operator strategies for geographic POINT data."""

from typing import Any, Optional
import os
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class CoordinateOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for geographic coordinate operators with PostgreSQL POINT type casting.

    Provides comprehensive coordinate filtering operations including exact equality,
    distance calculations, and PostgreSQL POINT type integration.

    Basic Operations:
        - eq: Exact coordinate equality
        - neq: Coordinate inequality
        - in: Coordinate in list of coordinates
        - notin: Coordinate not in list of coordinates

    Distance Operations:
        - distance_within: Find coordinates within distance (meters) of center point

    Note: Coordinates are provided as (latitude, longitude) tuples but are
    converted to PostgreSQL POINT(longitude, latitude) format.
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "in", "notin", "distance_within"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a coordinate operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # Define coordinate-specific operators
        coordinate_specific_ops = {"distance_within"}

        # If no field type provided, only handle coordinate-specific operators
        if field_type is None:
            return operator in coordinate_specific_ops

        # Check if field_type is a Coordinate type
        type_name = field_type.__name__ if hasattr(field_type, '__name__') else str(field_type)
        return "Coordinate" in type_name or "coordinate" in type_name.lower()

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for coordinate operators with proper PostgreSQL POINT type casting."""
        # Basic operations: cast to point
        if operator in ("eq", "neq", "in", "notin"):
            casted_path = SQL("({})::point").format(path_sql)

            if operator == "eq":
                if not isinstance(value, tuple) or len(value) != 2:
                    raise TypeError(f"eq operator requires a coordinate tuple (lat, lng), got {value}")
                lat, lng = value
                # PostgreSQL POINT uses (x,y) = (longitude, latitude)
                return SQL("{} = POINT({}, {})").format(casted_path, Literal(lng), Literal(lat))

            if operator == "neq":
                if not isinstance(value, tuple) or len(value) != 2:
                    raise TypeError(f"neq operator requires a coordinate tuple (lat, lng), got {value}")
                lat, lng = value
                return SQL("{} != POINT({}, {})").format(casted_path, Literal(lng), Literal(lat))

            if operator == "in":
                if not isinstance(value, list):
                    raise TypeError(f"'in' operator requires a list, got {type(value)}")

                parts = [casted_path, SQL(" IN (")]
                for i, coord in enumerate(value):
                    if i > 0:
                        parts.append(SQL(", "))
                    if not isinstance(coord, tuple) or len(coord) != 2:
                        raise TypeError(f"in operator requires coordinate tuples (lat, lng), got {coord}")
                    lat, lng = coord
                    parts.extend([SQL("POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")])
                parts.append(SQL(")"))
                return Composable(parts)

            if operator == "notin":
                if not isinstance(value, list):
                    raise TypeError(f"'notin' operator requires a list, got {type(value)}")

                parts = [casted_path, SQL(" NOT IN (")]
                for i, coord in enumerate(value):
                    if i > 0:
                        parts.append(SQL(", "))
                    if not isinstance(coord, tuple) or len(coord) != 2:
                        raise TypeError(f"notin operator requires coordinate tuples (lat, lng), got {coord}")
                    lat, lng = coord
                    parts.extend([SQL("POINT("), Literal(lng), SQL(", "), Literal(lat), SQL(")")])
                parts.append(SQL(")"))
                return Composable(parts)

        # Distance operations
        elif operator == "distance_within":
            # value should be a tuple: (center_coord, distance_meters)
            if not isinstance(value, tuple) or len(value) != 2:
                raise TypeError(
                    f"distance_within operator requires a tuple "
                    f"(center_coord, distance_meters), got {value}"
                )

            center_coord, distance_meters = value
            if not isinstance(center_coord, tuple) or len(center_coord) != 2:
                raise TypeError(
                    f"distance_within center must be a coordinate tuple "
                    f"(lat, lng), got {center_coord}"
                )
            if not isinstance(distance_meters, (int, float)) or distance_meters < 0:
                raise TypeError(
                    f"distance_within distance must be a positive number, got {distance_meters}"
                )

            # Get distance method from environment or use default
            method = os.environ.get("FRAISEQL_COORDINATE_DISTANCE_METHOD", "haversine").lower()

            # Build SQL based on configured method
            if method == "postgis":
                return self._build_distance_postgis(path_sql, center_coord, distance_meters)
            elif method == "earthdistance":
                return self._build_distance_earthdistance(path_sql, center_coord, distance_meters)
            elif method == "haversine":
                return self._build_distance_haversine(path_sql, center_coord, distance_meters)
            else:
                raise ValueError(
                    f"Invalid coordinate_distance_method: '{method}'. "
                    f"Valid options: 'postgis', 'haversine', 'earthdistance'"
                )

        return None

    def _build_distance_postgis(
        self, path_sql: SQL, center: tuple[float, float], distance_meters: float
    ) -> Composable:
        """Build SQL for distance using PostGIS ST_DWithin."""
        lat, lng = center
        casted_path = SQL("({})::point").format(path_sql)

        return Composable([
            SQL("ST_DWithin("),
            casted_path,
            SQL(", POINT("),
            Literal(lng),
            SQL(", "),
            Literal(lat),
            SQL("), "),
            Literal(distance_meters),
            SQL(")")
        ])

    def _build_distance_haversine(
        self, path_sql: SQL, center: tuple[float, float], distance_meters: float
    ) -> Composable:
        """Build SQL for distance using Haversine formula."""
        center_lat, center_lng = center

        # Haversine formula: great-circle distance on spherical Earth
        # d = 2 * R * arcsin(sqrt(sin²((lat2-lat1)/2) + cos(lat1)*cos(lat2)*sin²((lng2-lng1)/2)))
        return Composable([
            SQL("("),
            SQL("6371000 * 2 * ASIN(SQRT("),  # Earth radius in meters * 2 * arcsin(sqrt(...))
            SQL("POWER(SIN(RADIANS("),
            Literal(center_lat),
            SQL(") - RADIANS(ST_Y(("),
            path_sql,
            SQL(")::point))), 2) / 2 + "),
            SQL("COS(RADIANS("),
            Literal(center_lat),
            SQL(")) * COS(RADIANS(ST_Y(("),
            path_sql,
            SQL(")::point))) * "),
            SQL("POWER(SIN(RADIANS("),
            Literal(center_lng),
            SQL(") - RADIANS(ST_X(("),
            path_sql,
            SQL(")::point))), 2) / 2"),
            SQL(")) <= "),
            Literal(distance_meters),
            SQL(")")
        ])

    def _build_distance_earthdistance(
        self, path_sql: SQL, center: tuple[float, float], distance_meters: float
    ) -> Composable:
        """Build SQL for distance using earthdistance extension."""
        lat, lng = center
        casted_path = SQL("({})::point").format(path_sql)

        return Composable([
            SQL("earth_distance(ll_to_earth("),
            Literal(lat),
            SQL(", "),
            Literal(lng),
            SQL("), ll_to_earth(ST_Y("),
            casted_path,
            SQL("), ST_X("),
            casted_path,
            SQL("))) <= "),
            Literal(distance_meters)
        ])
```

### 6. `src/fraiseql/sql/operators/fallback/__init__.py`

```python
"""Fallback operator strategies for generic operations."""

from .null_operators import NullOperatorStrategy
from .comparison_operators import ComparisonOperatorStrategy
from .pattern_operators import PatternOperatorStrategy
from .list_operators import ListOperatorStrategy
from .path_operators import PathOperatorStrategy

__all__ = [
    "NullOperatorStrategy",
    "ComparisonOperatorStrategy",
    "PatternOperatorStrategy",
    "ListOperatorStrategy",
    "PathOperatorStrategy",
]
```

### 7. `src/fraiseql/sql/operators/fallback/null_operators.py`

```python
"""Null checking operator strategy."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL

from ..base import BaseOperatorStrategy


class NullOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for NULL checking operators.

    Supports:
        - isnull: Check if field is NULL or NOT NULL
    """

    SUPPORTED_OPERATORS = {"isnull"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is the isnull operator."""
        return operator == "isnull"

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for null checks."""
        if operator == "isnull":
            if value:
                return SQL("{} IS NULL").format(path_sql)
            else:
                return SQL("{} IS NOT NULL").format(path_sql)

        return None
```

### 8. `src/fraiseql/sql/operators/fallback/comparison_operators.py`

```python
"""Fallback comparison operator strategy."""

from typing import Any, Optional
from decimal import Decimal
from datetime import date, datetime
from uuid import UUID
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class ComparisonOperatorStrategy(BaseOperatorStrategy):
    """
    Fallback strategy for comparison operators (=, !=, <, >, <=, >=).

    This strategy handles comparison operators that weren't caught by
    more specific strategies (like NumericOperatorStrategy, StringOperatorStrategy, etc.).

    Supports:
        - eq, neq: Equality/inequality
        - gt, gte, lt, lte: Comparison operators
    """

    SUPPORTED_OPERATORS = {"eq", "neq", "gt", "gte", "lt", "lte"}

    OPERATOR_MAP = {
        "eq": " = ",
        "neq": " != ",
        "gt": " > ",
        "gte": " >= ",
        "lt": " < ",
        "lte": " <= ",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a comparison operator (fallback - always handles these)."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for comparison operators with proper type casting."""
        # Apply type casting for JSONB fields based on value type
        if jsonb_column:
            casted_path = self._apply_type_cast(path_sql, value)
        else:
            casted_path = path_sql

        sql_op = self.OPERATOR_MAP.get(operator)
        if not sql_op:
            return None

        # Handle boolean values specially for JSONB text comparison
        if isinstance(value, bool) and jsonb_column:
            # JSONB stores booleans as "true"/"false" text when extracted with ->>
            string_val = "true" if value else "false"
            return SQL("{}{}{}").format(casted_path, SQL(sql_op), Literal(string_val))

        # Standard comparison
        return SQL("{}{}{}").format(casted_path, SQL(sql_op), Literal(value))

    def _apply_type_cast(self, path_sql: SQL, value: Any) -> Composable:
        """Apply appropriate type casting to the JSONB path based on value type."""
        # Check bool BEFORE int since bool is subclass of int in Python
        if isinstance(value, bool):
            # For booleans, don't cast - will handle value conversion in build_sql
            return path_sql
        elif isinstance(value, (int, float, Decimal)):
            # All numeric operations need numeric casting
            return SQL("({})::numeric").format(path_sql)
        elif isinstance(value, datetime):
            return SQL("({})::timestamp").format(path_sql)
        elif isinstance(value, date):
            return SQL("({})::date").format(path_sql)
        elif isinstance(value, UUID):
            return SQL("({})::uuid").format(path_sql)
        else:
            # Default: no casting (treat as text)
            return path_sql
```

### 9. `src/fraiseql/sql/operators/fallback/pattern_operators.py`

```python
"""Fallback pattern matching operator strategy."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class PatternOperatorStrategy(BaseOperatorStrategy):
    """
    Fallback strategy for pattern matching operators.

    This strategy handles pattern operators that weren't caught by
    more specific strategies (like StringOperatorStrategy).

    Supports:
        - matches: Regex match (~)
        - imatches: Case-insensitive regex match (~*)
        - not_matches: Negated regex match (!~)
        - startswith: Prefix matching (LIKE)
        - endswith: Suffix matching (LIKE)
        - contains: Substring matching (LIKE)
        - ilike: Case-insensitive substring (ILIKE)
    """

    SUPPORTED_OPERATORS = {
        "matches", "imatches", "not_matches",
        "startswith", "endswith", "contains", "ilike"
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a pattern matching operator (fallback - always handles these)."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for pattern matching operators."""
        # No special type casting needed for pattern matching
        # Text operations work naturally

        if operator == "matches":
            return SQL("{} ~ {}").format(path_sql, Literal(value))

        if operator == "imatches":
            return SQL("{} ~* {}").format(path_sql, Literal(value))

        if operator == "not_matches":
            return SQL("{} !~ {}").format(path_sql, Literal(value))

        if operator == "startswith":
            if isinstance(value, str):
                like_val = f"{value}%"
                return SQL("{} LIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~ {}").format(path_sql, Literal(f"^{value}.*"))

        if operator == "endswith":
            if isinstance(value, str):
                like_val = f"%{value}"
                return SQL("{} LIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~ {}").format(path_sql, Literal(f".*{value}$"))

        if operator == "contains":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} LIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~ {}").format(path_sql, Literal(f".*{value}.*"))

        if operator == "ilike":
            if isinstance(value, str):
                like_val = f"%{value}%"
                return SQL("{} ILIKE {}").format(path_sql, Literal(like_val))
            return SQL("{} ~* {}").format(path_sql, Literal(value))

        return None
```

### 10. `src/fraiseql/sql/operators/fallback/list_operators.py`

```python
"""Fallback list operator strategy."""

from typing import Any, Optional
from decimal import Decimal
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class ListOperatorStrategy(BaseOperatorStrategy):
    """
    Fallback strategy for list-based operators (IN, NOT IN).

    This strategy handles list operators that weren't caught by
    more specific strategies.

    Supports:
        - in: Value in list
        - notin: Value not in list
    """

    SUPPORTED_OPERATORS = {"in", "notin"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a list operator (fallback - always handles these)."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for list operators."""
        if not isinstance(value, list):
            raise TypeError(f"'{operator}' operator requires a list, got {type(value)}")

        # Apply type casting for JSONB fields based on first value type
        if jsonb_column and value:
            casted_path = self._apply_type_cast(path_sql, value[0])
        else:
            casted_path = path_sql

        # Handle value conversion based on type
        if value and all(isinstance(v, bool) for v in value):
            # For boolean lists, use text comparison with converted values
            literals = [Literal("true" if v else "false") for v in value]
        elif value and all(isinstance(v, (int, float, Decimal)) for v in value):
            # For numeric lists
            literals = [Literal(v) for v in value]
        else:
            # For other types (strings, etc.)
            literals = [Literal(v) for v in value]

        # Build the IN/NOT IN clause
        parts = [casted_path]
        if operator == "in":
            parts.append(SQL(" IN ("))
        else:  # notin
            parts.append(SQL(" NOT IN ("))

        for i, lit in enumerate(literals):
            if i > 0:
                parts.append(SQL(", "))
            parts.append(lit)

        parts.append(SQL(")"))
        return Composable(parts)

    def _apply_type_cast(self, path_sql: SQL, value: Any) -> Composable:
        """Apply appropriate type casting to the JSONB path based on value type."""
        # Check bool BEFORE int since bool is subclass of int in Python
        if isinstance(value, bool):
            return path_sql  # No casting for booleans
        elif isinstance(value, (int, float, Decimal)):
            return SQL("({})::numeric").format(path_sql)
        else:
            return path_sql
```

### 11. `src/fraiseql/sql/operators/fallback/path_operators.py`

```python
"""Path/tree operator strategy for generic hierarchical operations."""

from typing import Any, Optional
from psycopg.sql import Composable, SQL, Literal

from ..base import BaseOperatorStrategy


class PathOperatorStrategy(BaseOperatorStrategy):
    """
    Strategy for generic path/tree operators.

    Supports:
        - depth_eq: Path depth equals value
        - depth_gt: Path depth greater than value
        - depth_lt: Path depth less than value
        - isdescendant: Is descendant of path
    """

    SUPPORTED_OPERATORS = {"depth_eq", "depth_gt", "depth_lt", "isdescendant"}

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is a path operator."""
        return operator in self.SUPPORTED_OPERATORS

    def build_sql(
        self,
        operator: str,
        value: Any,
        path_sql: Composable,
        field_type: Optional[type] = None,
        jsonb_column: Optional[str] = None,
    ) -> Optional[Composable]:
        """Build SQL for path operators."""
        if operator == "depth_eq":
            return SQL("nlevel({}) = {}").format(path_sql, Literal(value))

        if operator == "depth_gt":
            return SQL("nlevel({}) > {}").format(path_sql, Literal(value))

        if operator == "depth_lt":
            return SQL("nlevel({}) < {}").format(path_sql, Literal(value))

        if operator == "isdescendant":
            return SQL("{} <@ {}").format(path_sql, Literal(value))

        return None
```

---

## Files to Modify

### 1. Update `src/fraiseql/sql/operators/__init__.py`

Register all Phase 4 strategies:

```python
"""
Operator strategies for WHERE clause SQL generation.
"""

from .base import BaseOperatorStrategy
from .strategy_registry import OperatorRegistry, register_operator, get_default_registry

# Import core strategies (Phase 2)
from .core import (
    StringOperatorStrategy,
    NumericOperatorStrategy,
    BooleanOperatorStrategy,
)

# Import PostgreSQL-specific strategies (Phase 3)
from .postgresql import (
    NetworkOperatorStrategy,
    LTreeOperatorStrategy,
    DateRangeOperatorStrategy,
    MacAddressOperatorStrategy,
)

# Import advanced strategies (Phase 4)
from .array import ArrayOperatorStrategy
from .advanced import JsonbOperatorStrategy, CoordinateOperatorStrategy

# Import fallback strategies (Phase 4)
from .fallback import (
    NullOperatorStrategy,
    ComparisonOperatorStrategy,
    PatternOperatorStrategy,
    ListOperatorStrategy,
    PathOperatorStrategy,
)

# Auto-register core strategies (Phase 2)
register_operator(StringOperatorStrategy())
register_operator(NumericOperatorStrategy())
register_operator(BooleanOperatorStrategy())

# Auto-register PostgreSQL-specific strategies (Phase 3)
register_operator(NetworkOperatorStrategy())
register_operator(LTreeOperatorStrategy())
register_operator(DateRangeOperatorStrategy())
register_operator(MacAddressOperatorStrategy())

# Auto-register advanced strategies (Phase 4)
# CRITICAL: ArrayOperatorStrategy must come BEFORE ComparisonOperatorStrategy
# to handle array-specific operations properly
register_operator(NullOperatorStrategy())  # Always handle isnull first
register_operator(ArrayOperatorStrategy())  # Handle array ops before generic comparison
register_operator(CoordinateOperatorStrategy())  # Handle coordinate ops before generic comparison
register_operator(JsonbOperatorStrategy())  # JSONB-specific operators

# Register fallback strategies LAST (Phase 4)
# These catch any operators not handled by more specific strategies
register_operator(ComparisonOperatorStrategy())
register_operator(PatternOperatorStrategy())
register_operator(ListOperatorStrategy())
register_operator(PathOperatorStrategy())

__all__ = [
    "BaseOperatorStrategy",
    "OperatorRegistry",
    "register_operator",
    "get_default_registry",
    # Core (Phase 2)
    "StringOperatorStrategy",
    "NumericOperatorStrategy",
    "BooleanOperatorStrategy",
    # PostgreSQL (Phase 3)
    "NetworkOperatorStrategy",
    "LTreeOperatorStrategy",
    "DateRangeOperatorStrategy",
    "MacAddressOperatorStrategy",
    # Advanced (Phase 4)
    "ArrayOperatorStrategy",
    "JsonbOperatorStrategy",
    "CoordinateOperatorStrategy",
    # Fallback (Phase 4)
    "NullOperatorStrategy",
    "ComparisonOperatorStrategy",
    "PatternOperatorStrategy",
    "ListOperatorStrategy",
    "PathOperatorStrategy",
]
```

---

## Implementation Steps

**Note:** If time runs long, consider splitting at Step 3 - complete Steps 1-2 as "Phase 4a: Advanced Operators" and Steps 3-4 as "Phase 4b: Fallback Operators"

### Step 1: Create Array Operators (1.5-2 hours)
1. Create `src/fraiseql/sql/operators/array/` directory
2. Implement `ArrayOperatorStrategy` with all 13 operators
3. Write unit tests for each array operator
4. Verify array operations work correctly

### Step 2: Create Advanced Operators (2-3 hours)
1. Create `src/fraiseql/sql/operators/advanced/` directory
2. Implement `JsonbOperatorStrategy` (2 operators)
3. Implement `CoordinateOperatorStrategy` (5 operators + 3 distance methods)
4. Write unit tests for JSONB and coordinate operators
5. Verify distance calculations work correctly

### Step 3: Create Fallback Operators (2-3 hours)
1. Create `src/fraiseql/sql/operators/fallback/` directory
2. Implement `NullOperatorStrategy` (1 operator)
3. Implement `ComparisonOperatorStrategy` (6 operators)
4. Implement `PatternOperatorStrategy` (7 operators)
5. Implement `ListOperatorStrategy` (2 operators)
6. Implement `PathOperatorStrategy` (4 operators)
7. Write unit tests for all fallback operators

### Step 4: Integration & Testing (2 hours)
1. Register all strategies in correct order
2. Run full WHERE clause test suite
3. Verify no regressions
4. Check operator precedence is correct
5. Test JSONB field operations
6. Test coordinate distance calculations

---

## Verification Commands

```bash
# Run array operator tests
uv run pytest tests/unit/sql/where/operators/test_array*.py -v

# Run JSONB operator tests
uv run pytest tests/unit/sql/where/operators/test_jsonb*.py -v

# Run coordinate operator tests
uv run pytest tests/unit/sql/where/operators/test_coordinate*.py -v

# Run fallback operator tests
uv run pytest tests/unit/sql/operators/fallback/ -v

# Run full WHERE clause integration tests
uv run pytest tests/unit/sql/where/ -v

# Run repository integration tests
uv run pytest tests/integration/database/repository/ -k "filter" -v

# Test array operations
uv run pytest tests/ -k "array" -v

# Test coordinate operations
uv run pytest tests/ -k "coordinate or distance" -v

# Run ALL tests to verify nothing broke
uv run pytest tests/ -v
```

---

## Acceptance Criteria

- [ ] `ArrayOperatorStrategy` implemented with all 13 operators
- [ ] `JsonbOperatorStrategy` implemented with 2 operators
- [ ] `CoordinateOperatorStrategy` implemented with 5 operators + 3 distance methods
- [ ] `NullOperatorStrategy` implemented
- [ ] `ComparisonOperatorStrategy` implemented as fallback
- [ ] `PatternOperatorStrategy` implemented as fallback
- [ ] `ListOperatorStrategy` implemented as fallback
- [ ] `PathOperatorStrategy` implemented
- [ ] All strategies registered in correct order (specific before fallback)
- [ ] All unit tests passing
- [ ] All integration tests passing
- [ ] JSONB array operations work correctly
- [ ] Coordinate distance calculations work (all 3 methods)
- [ ] No performance regression
- [ ] Strategy precedence correct (array/coordinate before comparison)

---

## DO NOT

- ❌ Delete code from `operator_strategies.py` yet
- ❌ Change operator behavior or SQL output
- ❌ Break backward compatibility
- ❌ Skip writing tests for any operator
- ❌ Register fallback strategies before specific strategies

---

## Critical Implementation Notes

### Strategy Registration Order

**CRITICAL:** The order of strategy registration matters!

```python
# CORRECT ORDER:
1. NullOperatorStrategy          # Always handle isnull first
2. ArrayOperatorStrategy         # Array ops before generic comparison
3. CoordinateOperatorStrategy    # Coordinate ops before generic comparison
4. JsonbOperatorStrategy         # JSONB-specific operators
5. ComparisonOperatorStrategy    # Fallback for generic comparison
6. PatternOperatorStrategy       # Fallback for pattern matching
7. ListOperatorStrategy          # Fallback for list operations
8. PathOperatorStrategy          # Fallback for path operations
```

**Why?**
- Specific strategies must be checked before fallback strategies
- `ArrayOperatorStrategy` must intercept `eq/neq/contains` for array fields
- `CoordinateOperatorStrategy` must intercept `eq/neq/in/notin` for coordinate fields
- Fallback strategies handle everything else that wasn't caught

### JSONB Array Handling

Arrays in JSONB are handled differently than scalar values:
- Use `@>` for containment (not `=`)
- Use `jsonb_array_length()` for length operations
- Use `jsonb_array_elements_text()` for element queries

### Coordinate Format

Coordinates are tricky:
- **User provides:** `(latitude, longitude)` - natural order
- **PostgreSQL POINT:** `POINT(longitude, latitude)` - reversed!
- **Always swap** when building SQL: `POINT(lng, lat)`

### Distance Calculation Methods

Three methods with different trade-offs:
1. **PostGIS** (most accurate) - requires extension
2. **Haversine** (good accuracy) - pure SQL, no extensions
3. **Earthdistance** (simplest) - requires extension, less accurate

---

## Next Phase

Once Phase 4 is complete:
→ **Phase 5:** Deprecate and remove `operator_strategies.py`
→ **Phase 6:** Update documentation and migration guide
