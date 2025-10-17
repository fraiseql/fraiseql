"""Advanced patterns for FraiseQL.

This module provides optional advanced patterns that can be layered on top
of the core FraiseQL Rust-first architecture.
"""

from fraiseql.patterns.trinity import (
    TrinityMixin,
    trinity_field,
    get_pk_column_name,
    get_identifier_from_slug,
)

__all__ = [
    "TrinityMixin",
    "trinity_field",
    "get_pk_column_name",
    "get_identifier_from_slug",
]
