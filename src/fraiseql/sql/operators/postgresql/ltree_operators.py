"""LTree hierarchical path operator strategies."""

from typing import Any, Optional

from psycopg.sql import SQL, Composable, Literal

from fraiseql.sql.operators.base import BaseOperatorStrategy


class LTreeOperatorStrategy(BaseOperatorStrategy):
    """Strategy for PostgreSQL ltree (label tree) operators.

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
        "eq",
        "neq",
        "in",
        "nin",
        "ancestor_of",
        "descendant_of",
        "matches_lquery",
        "matches_ltxtquery",
        "matches_any_lquery",  # Array matching
        "in_array",  # Path in array
        "array_contains",  # Array contains path
        "concat",  # Path concatenation
        "lca",  # Lowest common ancestor
        "isnull",
    }

    def supports_operator(self, operator: str, field_type: Optional[type]) -> bool:
        """Check if this is an ltree operator."""
        if operator not in self.SUPPORTED_OPERATORS:
            return False

        # LTree-specific operators always handled by this strategy
        if operator in {
            "ancestor_of",
            "descendant_of",
            "matches_lquery",
            "matches_ltxtquery",
            "matches_any_lquery",
            "in_array",
            "array_contains",
            "concat",
            "lca",
        }:
            return True

        # With type hint, check if it's an LTree type
        if field_type is not None:
            type_name = field_type.__name__ if hasattr(field_type, "__name__") else str(field_type)
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
        # Comparison operators (need ltree casting on both sides)
        if operator == "eq":
            # Always cast to ltree (handles both JSONB and regular columns)
            casted_path = SQL("({})::ltree").format(path_sql)
            return SQL("{} = {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "neq":
            # Always cast to ltree (handles both JSONB and regular columns)
            casted_path = SQL("({})::ltree").format(path_sql)
            return SQL("{} != {}::ltree").format(casted_path, Literal(str(value)))

        # Always cast to ltree for all ltree-specific operators
        casted_path = SQL("({})::ltree").format(path_sql)

        # Hierarchical operators
        if operator == "ancestor_of":
            return SQL("{} @> {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "descendant_of":
            return SQL("{} <@ {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "matches_lquery":
            return SQL("{} ~ {}::lquery").format(casted_path, Literal(str(value)))

        if operator == "matches_ltxtquery":
            return SQL("{} @ {}::ltxtquery").format(casted_path, Literal(str(value)))

        # List operators (path needs casting, values cast by _build_in_operator)
        if operator == "in":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                cast_values="ltree",
            )

        if operator == "nin":
            return self._build_in_operator(
                casted_path,
                [str(v) for v in (value if isinstance(value, (list, tuple)) else [value])],
                negate=True,
                cast_values="ltree",
            )

        # Array operators
        if operator == "matches_any_lquery":
            # path ? ARRAY[lquery1, lquery2, ...]
            if not isinstance(value, list):
                raise TypeError(f"matches_any_lquery requires a list, got {type(value)}")
            if not value:
                raise ValueError("matches_any_lquery requires at least one pattern")

            # Build: (path)::ltree ? ARRAY['pattern1', 'pattern2', ...]
            from psycopg.sql import Composed

            parts = [casted_path, SQL(" ? ARRAY[")]
            for i, pattern in enumerate(value):
                if i > 0:
                    parts.append(SQL(", "))
                parts.append(Literal(str(pattern)))
            parts.append(SQL("]"))
            return Composed(parts)

        if operator == "in_array":
            # path <@ ARRAY[path1, path2, ...]
            if not isinstance(value, list):
                raise TypeError(f"in_array requires a list, got {type(value)}")

            # Build: (path)::ltree <@ ARRAY['path1'::ltree, 'path2'::ltree, ...]
            from psycopg.sql import Composed

            parts = [casted_path, SQL(" <@ ARRAY[")]
            for i, path in enumerate(value):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(str(path)), SQL("::ltree")])
            parts.append(SQL("]"))
            return Composed(parts)

        if operator == "array_contains":
            # ARRAY[path1, path2, ...] @> target_path
            # value can be (array, target) or (array, ignored_path_sql, target)
            if not isinstance(value, tuple):
                raise TypeError(f"array_contains requires a tuple, got {type(value)}")

            # Handle both (array, target) and (array, path_sql, target) formats
            if len(value) == 2:
                paths_array, target_path = value
            elif len(value) == 3:
                paths_array, _, target_path = value  # Middle element ignored
            else:
                raise TypeError(f"array_contains requires tuple of length 2 or 3, got {len(value)}")

            if not isinstance(paths_array, list):
                raise TypeError(
                    f"array_contains first element must be a list, got {type(paths_array)}"
                )

            # Build: ARRAY['path1'::ltree, 'path2'::ltree, ...] @> 'target'::ltree
            from psycopg.sql import Composed

            parts = [SQL("ARRAY[")]
            for i, path in enumerate(paths_array):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(str(path)), SQL("::ltree")])
            parts.extend([SQL("] @> "), Literal(str(target_path)), SQL("::ltree")])
            return Composed(parts)

        # Path manipulation operators
        if operator == "concat":
            # path1 || path2 - concatenate two ltree paths
            return SQL("{} || {}::ltree").format(casted_path, Literal(str(value)))

        if operator == "lca":
            # lca(ARRAY[path1, path2, ...]) - lowest common ancestor
            if not isinstance(value, list):
                raise TypeError(f"lca operator requires a list of paths, got {type(value)}")
            if not value:
                raise ValueError("lca operator requires at least one path")

            # Build: lca(ARRAY['path1'::ltree, 'path2'::ltree, ...])
            from psycopg.sql import Composed

            parts = [SQL("lca(ARRAY[")]
            for i, path in enumerate(value):
                if i > 0:
                    parts.append(SQL(", "))
                parts.extend([Literal(str(path)), SQL("::ltree")])
            parts.append(SQL("])"))
            return Composed(parts)

        # NULL checking
        if operator == "isnull":
            return self._build_null_check(path_sql, value)

        return None
