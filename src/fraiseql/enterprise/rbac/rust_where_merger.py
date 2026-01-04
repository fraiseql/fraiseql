"""Rust-based WHERE Clause Merger.

Safe merging of explicit user WHERE clauses with row-level auth filters.
Ensures auth filters always apply and detects conflicts.

This module handles the safe composition of:
- User-provided WHERE clauses (from GraphQL arguments)
- Auto-injected row-level filters (from RBAC constraints)

Performance: WHERE merging operations complete in <0.05ms
"""

import json
import logging
from typing import Any, Optional

logger = logging.getLogger(__name__)

# Try to import Rust implementation
try:
    from fraiseql._fraiseql_rs import PyWhereMerger

    HAS_RUST_WHERE_MERGER = True
except ImportError:
    HAS_RUST_WHERE_MERGER = False
    logger.warning("Rust WHERE merger not available. Install with 'pip install fraiseql[rust]'")


class ConflictError(Exception):
    """Raised when WHERE clause merge detects a conflict."""


class InvalidStructureError(Exception):
    """Raised when WHERE clause structure is invalid."""


class RustWhereMerger:
    """WHERE clause merger using Rust implementation.

    This provides safe composition of explicit WHERE clauses with row-level
    auth filters, with configurable conflict handling strategies.

    Example:
        ```python
        from fraiseql.enterprise.rbac.rust_where_merger import RustWhereMerger

        # Merge user's WHERE with auth filter
        explicit = {"status": {"eq": "active"}}
        auth_filter = {"tenant_id": {"eq": "tenant-123"}}

        merged = RustWhereMerger.merge_where(
            explicit, auth_filter, strategy="error"
        )
        # Result: {"AND": [{"status": {"eq": "active"}}, {"tenant_id": {"eq": "tenant-123"}}]}
        ```
    """

    @staticmethod
    def merge_where(
        explicit_where: Optional[dict[str, Any]],
        auth_filter: Optional[dict[str, Any]],
        strategy: str = "error",
    ) -> Optional[dict[str, Any]]:
        """Merge explicit WHERE clause with row-level auth filter.

        Combines user-provided WHERE with auto-injected row-level constraints
        using safe AND composition. Detects conflicts based on strategy.

        Args:
            explicit_where: User-provided WHERE clause (None = no user filter)
            auth_filter: Row-level filter from RBAC (None = no constraint)
            strategy: Conflict handling strategy:
                - "error" (default): Raise ConflictError on conflict
                - "override": Auth filter takes precedence (user's WHERE ignored)
                - "log": Continue despite conflicts (both applied via AND)

        Returns:
            Merged WHERE clause, or None if neither filter exists

        Raises:
            ConflictError: If strategy="error" and conflict detected
            InvalidStructureError: If WHERE clause structure is invalid
            ValueError: If strategy is not recognized

        Examples:
            ```python
            # Case 1: Only auth filter
            result = RustWhereMerger.merge_where(None, {"tenant_id": {"eq": "t1"}})
            # → {"tenant_id": {"eq": "t1"}}

            # Case 2: Only explicit WHERE
            result = RustWhereMerger.merge_where({"status": {"eq": "active"}}, None)
            # → {"status": {"eq": "active"}}

            # Case 3: Both (no conflict)
            result = RustWhereMerger.merge_where(
                {"status": {"eq": "active"}},
                {"tenant_id": {"eq": "t1"}}
            )
            # → {"AND": [{"status": {"eq": "active"}}, {"tenant_id": {"eq": "t1"}}]}

            # Case 4: Conflict detected (same field, different operators)
            result = RustWhereMerger.merge_where(
                {"owner_id": {"eq": "user1"}},
                {"owner_id": {"eq": "user2"}},
                strategy="error"
            )
            # → Raises ConflictError

            # Case 5: Conflict with "override" strategy
            result = RustWhereMerger.merge_where(
                {"owner_id": {"eq": "user1"}},
                {"owner_id": {"eq": "user2"}},
                strategy="override"
            )
            # → {"owner_id": {"eq": "user2"}}  (user's WHERE ignored)
            ```
        """
        if not HAS_RUST_WHERE_MERGER:
            raise RuntimeError(
                "Rust WHERE merger not available. Install with 'pip install fraiseql[rust]'"
            )

        # Validate strategy
        if strategy not in ("error", "override", "log"):
            raise ValueError(f"Invalid strategy: {strategy}. Must be 'error', 'override', or 'log'")

        try:
            # Convert Python dicts to JSON strings for Rust
            explicit_json = json.dumps(explicit_where) if explicit_where else None
            auth_json = json.dumps(auth_filter) if auth_filter else None

            # Call Rust merger
            merged_json = PyWhereMerger.merge_where(explicit_json, auth_json, strategy)

            # Convert result back to Python dict
            return json.loads(merged_json) if merged_json else None

        except Exception as e:
            error_msg = str(e)

            # Map Rust errors to Python exceptions
            if "conflict" in error_msg.lower():
                raise ConflictError(f"WHERE clause conflict: {error_msg}") from e

            if "structure" in error_msg.lower():
                raise InvalidStructureError(f"Invalid WHERE structure: {error_msg}") from e

            # Re-raise as-is for other errors
            raise

    @staticmethod
    def validate_where(where_clause: dict[str, Any]) -> bool:
        """Validate WHERE clause structure.

        Checks that:
        - WHERE is an object (not array or primitive)
        - Fields contain operator objects
        - AND/OR contain arrays of valid clauses
        - Nested structures are properly formed

        Args:
            where_clause: WHERE clause to validate

        Returns:
            True if valid (or raises exception)

        Raises:
            InvalidStructureError: If structure is invalid

        Examples:
            ```python
            # Valid: simple field with operator
            RustWhereMerger.validate_where({"status": {"eq": "active"}})
            # → True

            # Valid: AND composition
            RustWhereMerger.validate_where({
                "AND": [
                    {"status": {"eq": "active"}},
                    {"id": {"in": ["1", "2"]}}
                ]
            })
            # → True

            # Invalid: AND not an array
            RustWhereMerger.validate_where({"AND": "not_an_array"})
            # → Raises InvalidStructureError

            # Invalid: field missing operators
            RustWhereMerger.validate_where({"status": "active"})
            # → Raises InvalidStructureError
            ```
        """
        if not HAS_RUST_WHERE_MERGER:
            raise RuntimeError(
                "Rust WHERE merger not available. Install with 'pip install fraiseql[rust]'"
            )

        try:
            # Convert to JSON and validate via Rust
            where_json = json.dumps(where_clause)
            PyWhereMerger.validate_where(where_json)
            return True

        except Exception as e:
            raise InvalidStructureError(f"Invalid WHERE clause structure: {e!s}") from e

    @staticmethod
    def to_row_filter_where(field: str, value: str, operator: str = "eq") -> dict[str, Any]:
        """Convert RowFilter to WHERE clause fragment.

        Helper method to convert row constraint to WHERE structure
        compatible with merge_where().

        Args:
            field: Column name (e.g., "owner_id", "tenant_id")
            value: Value to match (e.g., UUID as string)
            operator: Comparison operator (default: "eq")

        Returns:
            WHERE clause fragment ready for merging

        Examples:
            ```python
            # Convert row filter to WHERE
            where = RustWhereMerger.to_row_filter_where("owner_id", "user-123")
            # → {"owner_id": {"eq": "user-123"}}

            # With custom operator
            where = RustWhereMerger.to_row_filter_where(
                "tenant_id", "t1", operator="eq"
            )
            # → {"tenant_id": {"eq": "t1"}}
            ```
        """
        return {field: {operator: value}}


# Convenience function for easy access
def merge_where_clauses(
    explicit_where: Optional[dict[str, Any]],
    auth_filter: Optional[dict[str, Any]],
    strategy: str = "error",
) -> Optional[dict[str, Any]]:
    """Merge WHERE clauses with row-level auth filter.

    Convenience function wrapping RustWhereMerger.merge_where() for easy use
    in middleware and resolvers.

    Args:
        explicit_where: User's WHERE clause from GraphQL
        auth_filter: Row constraint filter from RBAC
        strategy: Conflict handling ("error", "override", "log")

    Returns:
        Merged WHERE clause or None

    Raises:
        ConflictError: On conflict with strategy="error"
        InvalidStructureError: If structure is invalid

    Example:
        ```python
        from fraiseql.enterprise.rbac.rust_where_merger import merge_where_clauses

        merged = merge_where_clauses(user_where, row_filter, strategy="error")
        # Use merged WHERE in query execution
        ```
    """
    return RustWhereMerger.merge_where(explicit_where, auth_filter, strategy)
