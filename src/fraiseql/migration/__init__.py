"""Migration utilities for FraiseQL.

This module provides utilities to help migrate from other GraphQL frameworks,
particularly Strawberry GraphQL.
"""

from .strawberry_migration import check_strawberry_compatibility

__all__ = [
    "check_strawberry_compatibility",
]
