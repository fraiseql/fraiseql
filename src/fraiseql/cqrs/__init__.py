"""CQRS support for FraiseQL - DEPRECATED.

This module provided v1 Python runtime query execution.
v2 uses Rust fraiseql-server for all query/mutation execution.
Python is now for schema authoring only.
"""


def __getattr__(name):
    """Raise NotImplementedError for CQRS classes."""
    if name in ("CQRSRepository", "CursorPaginator", "PaginationParams"):
        raise NotImplementedError(
            f"{name} requires Python runtime query execution, which has been removed in v2. "
            "Use Rust fraiseql-server with compiled schema instead: fraiseql-cli compile schema.json"
        )
    raise AttributeError(f"module '{__name__}' has no attribute '{name}'")


__all__ = []
